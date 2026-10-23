//! What a behavior or sensor is handed when the brain runs it.

use std::sync::Arc;

use super::Activities;
use super::memory::Memories;
use crate::entity::Entity;
use crate::world::World;

/// The mob a brain is running on, its memories, and the current tick.
pub struct BrainContext<'brain> {
    /// The world the mob is in.
    pub level: &'brain Arc<World>,
    /// The mob whose brain is running.
    pub mob: &'brain dyn Entity,
    /// The mob's memories.
    pub memories: &'brain mut Memories,
    /// The mob's activities, which behaviors may switch.
    pub activities: &'brain mut Activities,
    /// Vanilla `level.getGameTime()` for this tick.
    pub time: i64,
}

impl<'brain> BrainContext<'brain> {
    /// A context for `mob` at the level's current game time.
    pub fn new(
        level: &'brain Arc<World>,
        mob: &'brain dyn Entity,
        memories: &'brain mut Memories,
        activities: &'brain mut Activities,
    ) -> Self {
        Self {
            level,
            mob,
            memories,
            activities,
            time: level.game_time(),
        }
    }
}
