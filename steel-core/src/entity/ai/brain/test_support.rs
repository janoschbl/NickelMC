//! Shared setup for brain tests.

use std::sync::Arc;

use glam::DVec3;
use simdnbt::owned::NbtTag;
use steel_registry::vanilla_entities;
use steel_utils::{DowncastType, DowncastTypeKey, Identifier};

use super::Activities;
use super::context::BrainContext;
use super::memory::{Memories, MemoryModuleType, MemoryValue, PersistentMemoryValue};
use crate::bootstrap::init_globals;
use crate::entity::registry::ENTITIES;
use crate::entity::{PathfinderMob, SharedEntity, next_entity_id};
use crate::test_support::test_world;

/// A memory value for tests.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Ticks(pub(super) i32);

// SAFETY: This test-only key is distinct and identifies `Ticks` within the test
// process.
unsafe impl DowncastType for Ticks {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:test/memory_value/ticks");
}

impl MemoryValue for Ticks {}

impl PersistentMemoryValue for Ticks {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::Int(self.0)
    }

    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        tag.int().map(Self)
    }
}

/// Declares a memory module type under a test-owned namespace.
pub(super) const fn test_memory<V: MemoryValue + DowncastType>(
    path: &'static str,
) -> MemoryModuleType<V> {
    MemoryModuleType::new(Identifier::new_static("steel_test", path))
}

/// Declares a persistent memory module type under a test-owned namespace.
pub(super) const fn test_persistent_memory<V: PersistentMemoryValue + DowncastType>(
    path: &'static str,
) -> MemoryModuleType<V> {
    MemoryModuleType::new_persistent(Identifier::new_static("steel_test", path))
}

/// Spawns a pig, detached from any chunk.
pub(super) fn spawn_pig() -> SharedEntity {
    init_globals();
    ENTITIES
        .create(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::ZERO,
            Arc::downgrade(test_world()),
        )
        .expect("pig should have a registered entity factory")
}

/// A mob and its memories, for driving behaviors and sensors directly.
pub(super) struct TestBrain {
    entity: SharedEntity,
    memories: Memories,
    activities: Activities,
    time: i64,
}

impl TestBrain {
    pub(super) fn new() -> Self {
        Self {
            entity: spawn_pig(),
            memories: Memories::new(),
            activities: Activities::new(),
            time: 0,
        }
    }

    pub(super) const fn memories(&mut self) -> &mut Memories {
        &mut self.memories
    }

    pub(super) fn mob(&self) -> &dyn PathfinderMob {
        self.entity
            .as_pathfinder_mob()
            .expect("pig should be a pathfinder mob")
    }

    pub(super) const fn set_time(&mut self, time: i64) {
        self.time = time;
    }

    pub(super) fn context(&mut self) -> BrainContext<'_> {
        let Self {
            entity,
            memories,
            activities,
            time,
        } = self;

        BrainContext {
            level: test_world(),
            mob: entity
                .as_pathfinder_mob()
                .expect("pig should be a pathfinder mob"),
            memories,
            activities,
            time: *time,
        }
    }
}
