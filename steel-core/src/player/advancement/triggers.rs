//! Typed gameplay events and compiled advancement listeners.

use crate::entity::Entity;
use crate::player::Player;
use steel_registry::advancement::registry::AdvancementRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::Identifier;

/// Event arguments remain borrowed until all matching listeners have been selected.
pub enum CriterionEvent<'a> {
    /// The resulting enchanted stack and the number of levels paid.
    EnchantedItem {
        /// Resulting enchanted item, not the pre-enchantment input.
        item: &'a ItemStack,
        /// Levels consumed by the selected offer.
        levels: i32,
    },
    /// Active effects have changed, optionally because of another entity.
    EffectsChanged {
        /// Entity that applied the effect; absent for removal or natural expiration.
        source: Option<&'a dyn Entity>,
    },
}

impl CriterionEvent<'_> {
    /// Registry key of the event's trigger.
    #[must_use]
    pub const fn trigger(&self) -> Identifier {
        match self {
            Self::EnchantedItem { .. } => Identifier::vanilla_static("enchanted_item"),
            Self::EffectsChanged { .. } => Identifier::vanilla_static("effects_changed"),
        }
    }
}

/// A compiled criterion predicate with access to live gameplay context.
/// Kept in core so the registry does not depend on player or world implementations.
pub struct CriterionListener {
    /// Advancement to award when this listener matches.
    pub advancement: AdvancementRef,
    /// Criterion name within the advancement.
    pub criterion: &'static str,
    /// Predicate evaluated without holding advancement or inventory locks.
    pub matches: fn(&Player, &CriterionEvent<'_>) -> bool,
}

include!(concat!(env!("OUT_DIR"), "/advancement_triggers.rs"));

impl Player {
    /// Evaluates the compiled listeners before awarding any matched criteria.
    pub fn trigger_criterion(&self, event: CriterionEvent<'_>) {
        self.trigger_criterion_listeners(&event, vanilla_listeners());
    }

    /// Dispatches a typed event to a supplied listener set (including plugin listeners).
    pub fn trigger_criterion_listeners(
        &self,
        event: &CriterionEvent<'_>,
        listeners: &[CriterionListener],
    ) {
        let trigger = event.trigger();
        let candidates = {
            let progress = self.advancements.lock();
            listeners
                .iter()
                .filter(|listener| {
                    listener
                        .advancement
                        .get_criterion(listener.criterion)
                        .is_some_and(|criterion| criterion.id() == trigger)
                        && !progress
                            .progress
                            .get_progress(listener.advancement)
                            .is_some_and(|progress| {
                                progress.is_done() || progress.is_criterion_done(listener.criterion)
                            })
                })
                .collect::<Vec<_>>()
        };
        let matches = candidates
            .into_iter()
            .filter(|listener| (listener.matches)(self, event))
            .map(|listener| (listener.advancement, listener.criterion))
            .collect::<Vec<_>>();
        for (advancement, criterion) in matches {
            self.advancements.lock().award(self, advancement, criterion);
        }
    }
}

#[cfg(test)]
#[path = "triggers/tests.rs"]
mod tests;
