//! Combat history is owned by the server, never by its victim or attackers.

use std::mem;

use steel_registry::{
    REGISTRY, TaggedRegistryExt, blocks::block_state_ext::BlockStateExt, vanilla_block_tags,
    vanilla_blocks,
};

use super::DamageSource;
use crate::entity::LivingEntity;
use crate::world::LevelReader;

mod message;

#[derive(Clone)]
pub(crate) struct CombatEntry {
    pub source: DamageSource,
    pub damage: f32,
    pub fall_location: Option<&'static str>,
    pub fall_distance: f32,
}

impl CombatEntry {
    pub fn new(victim: &dyn LivingEntity, source: &DamageSource, damage: f32) -> Self {
        Self {
            source: source.clone(),
            damage,
            fall_location: Self::fall_location(victim),
            fall_distance: victim.fall_distance() as f32,
        }
    }

    fn fall_location(victim: &dyn LivingEntity) -> Option<&'static str> {
        let Some(pos) = victim.last_climbable_pos() else {
            return victim.is_in_water().then_some("water");
        };
        let world = victim.level()?;
        let block = world.get_block_state(pos).get_block();
        Some(
            if block == &vanilla_blocks::LADDER
                || REGISTRY
                    .blocks
                    .is_in_tag(block, &vanilla_block_tags::BlockTag::TRAPDOORS)
            {
                "ladder"
            } else if block == &vanilla_blocks::VINE {
                "vines"
            } else if block == &vanilla_blocks::WEEPING_VINES
                || block == &vanilla_blocks::WEEPING_VINES_PLANT
            {
                "weeping_vines"
            } else if block == &vanilla_blocks::TWISTING_VINES
                || block == &vanilla_blocks::TWISTING_VINES_PLANT
            {
                "twisting_vines"
            } else if block == &vanilla_blocks::SCAFFOLDING {
                "scaffolding"
            } else {
                "other_climbable"
            },
        )
    }
}

/// Released entries must be dropped after history and lifecycle locks are released.
#[derive(Default)]
pub(crate) struct CombatChanges {
    pub entered: bool,
    pub left: Option<i32>,
    pub released: Vec<CombatEntry>,
}

impl CombatChanges {
    pub fn apply(self, victim: &dyn LivingEntity) {
        if let Some(duration) = self.left {
            victim.on_leave_combat(duration);
        }
        if self.entered {
            victim.on_enter_combat();
        }
        drop(self.released);
    }
}

#[derive(Clone, Default)]
pub(crate) struct CombatTracker {
    pub entries: Vec<CombatEntry>,
    last_damage_time: i32,
    combat_start_time: i32,
    combat_end_time: i32,
    in_combat: bool,
    taking_damage: bool,
}

impl CombatTracker {
    pub fn record_damage(&mut self, entry: CombatEntry, tick: i32, alive: bool) -> CombatChanges {
        let mut changes = self.recheck_status(tick, alive);
        let living_attacker = entry
            .source
            .causing_entity()
            .is_some_and(|entity| entity.as_living_entity().is_some());
        self.entries.push(entry);
        self.last_damage_time = tick;
        self.taking_damage = true;
        if !self.in_combat && alive && living_attacker {
            self.in_combat = true;
            self.combat_start_time = tick;
            self.combat_end_time = tick;
            changes.entered = true;
        }
        changes
    }

    pub const fn duration(&self, tick: i32) -> i32 {
        let end = if self.in_combat {
            tick
        } else {
            self.combat_end_time
        };
        end.wrapping_sub(self.combat_start_time)
    }

    pub fn recheck_status(&mut self, tick: i32, alive: bool) -> CombatChanges {
        let reset = if self.in_combat { 300 } else { 100 };
        if !self.taking_damage || (alive && tick.wrapping_sub(self.last_damage_time) <= reset) {
            return CombatChanges::default();
        }
        let was_in_combat = self.in_combat;
        self.taking_damage = false;
        self.in_combat = false;
        self.combat_end_time = tick;
        CombatChanges {
            entered: false,
            left: was_in_combat.then(|| self.duration(tick)),
            released: mem::take(&mut self.entries),
        }
    }
}

#[cfg(test)]
mod tests;
