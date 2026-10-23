//! Building a mob's brain from its type's declarations and its saved memories.

use super::{ActivityData, Brain};
use crate::entity::PathfinderMob;
use crate::entity::ai::brain::memory::{MemoryModuleTypeRef, PackedMemories};
use crate::entity::ai::brain::sensor::SensorTypeRef;

/// Creates the activities a mob's brain runs.
pub type ActivitySupplier = fn(&dyn PathfinderMob) -> Vec<ActivityData>;

/// What an entity type declares about its brain: the memories it holds, the
/// sensors it runs, and the activities it supplies.
///
/// Every mob gets a fresh brain from [`make_brain`](Self::make_brain), so
/// sensors and behaviors never share state between mobs.
pub struct BrainProvider {
    memory_types: Vec<MemoryModuleTypeRef>,
    sensor_types: Vec<SensorTypeRef>,
    activities: ActivitySupplier,
}

impl BrainProvider {
    /// Declares a brain of `sensor_types` running the activities `activities`
    /// builds for each mob.
    ///
    /// Sensors and behaviors register the memories they need on their own, so
    /// `memory_types` only lists the ones nothing else declares.
    #[must_use]
    pub const fn new(
        memory_types: Vec<MemoryModuleTypeRef>,
        sensor_types: Vec<SensorTypeRef>,
        activities: ActivitySupplier,
    ) -> Self {
        Self {
            memory_types,
            sensor_types,
            activities,
        }
    }

    /// Builds a brain for `mob`, restoring `packed` into its memories.
    #[must_use]
    pub fn make_brain(&self, mob: &dyn PathfinderMob, packed: &PackedMemories) -> Brain {
        let mut brain = Brain::new();

        for &memory in &self.memory_types {
            brain.register_memory(memory);
        }
        for sensor_type in &self.sensor_types {
            brain.add_sensor(sensor_type.create());
        }
        for activity in (self.activities)(mob) {
            brain.add_activity(activity);
        }
        brain.memories_mut().restore(packed);
        brain
    }
}
