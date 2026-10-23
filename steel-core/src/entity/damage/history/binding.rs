use std::{
    ptr,
    sync::{Arc, Weak},
};

use steel_utils::locks::SyncMutex;

use super::{
    CombatChanges, CombatEntry, CombatRecord, CombatTracker, DamageHistory, DamageRecord,
    DamageSource, RecentDamageSource, WeakDamageSource,
};
use crate::entity::EntityGeneration;
use crate::level_data::GameTime;

/// Serializes history writes with removal and gameplay ownership changes.
#[derive(Default)]
pub(crate) struct DamageHistoryBinding {
    state: SyncMutex<BindingState>,
}

#[derive(Default)]
struct BindingState {
    victim: Option<Arc<DamageHistoryVictim>>,
    owner: Weak<()>,
    removed: bool,
}

/// Owned only by the entity's binding; records observe its lifetime weakly.
pub(super) struct DamageHistoryVictim {
    generation: EntityGeneration,
    history: Weak<DamageHistory>,
}

impl DamageHistoryBinding {
    /// The marker must live in a gameplay owner, never in the entity itself.
    pub(crate) fn retain_owner(&self) -> Arc<()> {
        let mut state = self.state.lock();
        if let Some(owner) = state.owner.upgrade() {
            return owner;
        }
        let owner = Arc::new(());
        state.owner = Arc::downgrade(&owner);
        owner
    }

    pub(super) fn record(
        &self,
        history: &Arc<DamageHistory>,
        generation: EntityGeneration,
        source: &DamageSource,
        clock: &Arc<GameTime>,
    ) {
        let descriptor = WeakDamageSource::new(source);
        let replaced = {
            let mut state = self.state.lock();
            let victim_lifetime = Self::bind(&mut state, history, generation);
            let record = DamageRecord {
                victim_lifetime,
                owner: Weak::clone(&state.owner),
                retained: (!state.removed && state.owner.strong_count() > 0)
                    .then(|| source.clone()),
                source: descriptor,
                timestamp: clock.ticks(),
                clock: Arc::clone(clock),
            };
            history.records.lock().insert(generation, record)
        };
        // Dropping sources can destroy entities; release both locks first.
        drop(replaced);
    }

    /// Both the source and the upgraded history can own entities. Drop them outside
    /// the entity lifecycle lock, including when this is the last history handle.
    pub(crate) fn release_on_removal(
        &self,
    ) -> Option<(Arc<DamageHistory>, Option<DamageSource>, Vec<CombatEntry>)> {
        let mut state = self.state.lock();
        state.removed = true;
        let victim = state.victim.as_ref()?;
        let history = victim.history.upgrade()?;
        let released = history
            .records
            .lock()
            .get_mut(&victim.generation)
            .and_then(|record| record.retained.take());
        let combat = history
            .combat_records
            .lock()
            .remove(&victim.generation)
            .map_or_else(Vec::new, |record| record.tracker.entries);
        Some((history, released, combat))
    }

    pub(crate) fn clear_removed(&self) {
        self.state.lock().removed = false;
    }

    pub(crate) fn last_damage_source(&self) -> Option<RecentDamageSource> {
        let (history, victim) = self.bound_history()?;
        history.last_damage_source(victim)
    }

    fn bind(
        state: &mut BindingState,
        history: &Arc<DamageHistory>,
        generation: EntityGeneration,
    ) -> Weak<DamageHistoryVictim> {
        if let Some(bound) = &state.victim
            && ptr::eq(bound.history.as_ptr(), Arc::as_ptr(history))
        {
            return Arc::downgrade(bound);
        }
        let bound = Arc::new(DamageHistoryVictim {
            generation,
            history: Arc::downgrade(history),
        });
        let lifetime = Arc::downgrade(&bound);
        state.victim = Some(bound);
        lifetime
    }

    pub(crate) fn record_combat(
        &self,
        history: &Arc<DamageHistory>,
        generation: EntityGeneration,
        entry: CombatEntry,
        tick: i32,
        alive: bool,
    ) -> CombatChanges {
        let mut state = self.state.lock();
        if state.removed {
            return CombatChanges::default();
        }
        let victim_lifetime = Self::bind(&mut state, history, generation);
        let mut records = history.combat_records.lock();
        let record = records.entry(generation).or_insert_with(|| CombatRecord {
            victim_lifetime,
            owner: Weak::clone(&state.owner),
            tracker: CombatTracker::default(),
        });
        record.owner = Weak::clone(&state.owner);
        record.tracker.record_damage(entry, tick, alive)
    }

    pub(crate) fn recheck_combat(&self, tick: i32, alive: bool) -> CombatChanges {
        let Some((history, victim)) = self.bound_history() else {
            return CombatChanges::default();
        };
        history
            .combat_records
            .lock()
            .get_mut(&victim)
            .map_or_else(CombatChanges::default, |record| {
                record.tracker.recheck_status(tick, alive)
            })
    }

    pub(crate) fn combat_tracker(&self) -> CombatTracker {
        let Some((history, victim)) = self.bound_history() else {
            return CombatTracker::default();
        };
        history
            .combat_records
            .lock()
            .get(&victim)
            .map_or_else(CombatTracker::default, |record| record.tracker.clone())
    }

    /// Clears transient history when a target domain's state is restored.
    pub(crate) fn clear(&self) {
        if let Some((history, victim)) = self.bound_history() {
            history.clear_victim(victim);
        }
    }

    fn bound_history(&self) -> Option<(Arc<DamageHistory>, EntityGeneration)> {
        let state = self.state.lock();
        let bound = state.victim.as_ref()?;
        Some((bound.history.upgrade()?, bound.generation))
    }
}
