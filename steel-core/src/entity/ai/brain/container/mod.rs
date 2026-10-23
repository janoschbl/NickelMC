//! The brain itself.

mod provider;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::ptr;
use std::sync::Arc;

use steel_registry::activity::ActivityRef;

pub use super::{Activities, ActivityData, ActivityRequirements};
pub use provider::BrainProvider;

use super::behavior::{BehaviorControl, BehaviorStatus};
use super::context::BrainContext;
use super::memory::{Memories, MemoryModuleTypeRef, PackedMemories};
use super::sensor::{Sensor, WrappedSensor};
use crate::entity::Entity;
use crate::world::World;

/// Slots in a Java `HashMap` of activities before its first resize.
const JAVA_MAP_INITIAL_CAPACITY: u32 = 16;

/// One activity's behaviors at a single priority.
struct ActivityBehaviors {
    activity: ActivityRef,
    behaviors: Vec<Box<dyn BehaviorControl>>,
}

/// A mob's memory-driven AI.
///
/// Mirrors vanilla `Brain`. The schedule is not stored here: the mob passes the
/// activity its schedule asks for to
/// [`update_activity_from_schedule`](Self::update_activity_from_schedule).
pub struct Brain {
    memories: Memories,
    sensors: Vec<WrappedSensor>,
    behaviors_by_priority: BTreeMap<i32, Vec<ActivityBehaviors>>,
    activities: Activities,
}

impl Brain {
    /// An empty brain running its default activity alongside the core one.
    #[must_use]
    pub fn new() -> Self {
        let mut brain = Self {
            memories: Memories::new(),
            sensors: Vec::new(),
            behaviors_by_priority: BTreeMap::new(),
            activities: Activities::new(),
        };
        brain.use_default_activity();
        brain
    }

    /// The mob's memories.
    #[must_use]
    pub const fn memories(&self) -> &Memories {
        &self.memories
    }

    /// The mob's memories, for reads and writes outside a brain tick.
    pub const fn memories_mut(&mut self) -> &mut Memories {
        &mut self.memories
    }

    /// The mob's activities.
    #[must_use]
    pub const fn activities(&self) -> &Activities {
        &self.activities
    }

    /// Registers a memory this mob holds.
    pub fn register_memory(&mut self, memory: MemoryModuleTypeRef) {
        self.memories.register(memory);
    }

    /// Adds a sensor, registering every memory it writes.
    pub fn add_sensor(&mut self, sensor: Box<dyn Sensor>) {
        let sensor = WrappedSensor::new(sensor);
        for &memory in sensor.requires() {
            self.memories.register(memory);
        }
        self.sensors.push(sensor);
    }

    /// Adds an activity's behaviors, registering every memory they need.
    ///
    /// Re-adding an activity replaces its requirements and the memories it
    /// erases when stopped, and appends its behaviors.
    pub fn add_activity(&mut self, data: ActivityData) {
        let ActivityData {
            activity,
            behaviors,
            requirements,
            memories_to_erase_when_stopped,
        } = data;

        self.activities
            .register(activity, requirements, memories_to_erase_when_stopped);

        for (priority, behavior) in behaviors {
            let memories = &mut self.memories;
            behavior.visit_required_memories(&mut |memory| memories.register(memory));

            let groups = self.behaviors_by_priority.entry(priority).or_default();
            if let Some(group) = groups
                .iter_mut()
                .find(|group| ptr::eq(group.activity, activity))
            {
                group.behaviors.push(behavior);
                continue;
            }

            groups.push(ActivityBehaviors {
                activity,
                behaviors: vec![behavior],
            });
            sort_in_java_map_order(groups);
        }
    }

    /// Collects the persistent memories for saving.
    ///
    /// Restore them with [`BrainProvider::make_brain`].
    #[must_use]
    pub fn pack(&self) -> PackedMemories {
        self.memories.pack()
    }

    /// Whether the brain has no memories, sensors, or behaviors.
    #[must_use]
    pub fn is_brain_dead(&self) -> bool {
        self.memories.is_empty() && self.sensors.is_empty() && self.behaviors_by_priority.is_empty()
    }

    /// Sets the activities that stay active alongside the current one.
    pub fn set_core_activities(&mut self, activities: Vec<ActivityRef>) {
        self.activities.set_core_activities(activities);
    }

    /// Sets the activity the brain falls back to.
    pub const fn set_default_activity(&mut self, activity: ActivityRef) {
        self.activities.set_default_activity(activity);
    }

    /// Switches to the default activity.
    pub fn use_default_activity(&mut self) {
        self.activities.use_default_activity(&mut self.memories);
    }

    /// Switches to `activity`, or to the default when its requirements fail.
    pub fn set_active_activity_if_possible(&mut self, activity: ActivityRef) {
        self.activities
            .set_active_activity_if_possible(&mut self.memories, activity);
    }

    /// Switches to the first activity whose requirements are met, if any.
    pub fn set_active_activity_to_first_valid(&mut self, activities: &[ActivityRef]) {
        self.activities
            .set_active_activity_to_first_valid(&mut self.memories, activities);
    }

    /// Re-evaluates the scheduled activity; see
    /// [`Activities::update_activity_from_schedule`].
    pub fn update_activity_from_schedule(
        &mut self,
        game_time: i64,
        scheduled: impl FnOnce() -> ActivityRef,
    ) {
        self.activities
            .update_activity_from_schedule(&mut self.memories, game_time, scheduled);
    }

    /// Whether `activity` is currently active.
    #[must_use]
    pub fn is_active(&self, activity: ActivityRef) -> bool {
        self.activities.is_active(activity)
    }

    /// The active activity that is not a core one.
    #[must_use]
    pub fn active_non_core_activity(&self) -> Option<ActivityRef> {
        self.activities.active_non_core_activity()
    }

    /// Runs one brain tick.
    ///
    /// Expired memories are dropped before anything reads them, sensors then
    /// write what the mob perceives, stopped behaviors get their chance to
    /// start, and running ones tick or stop.
    pub fn tick(&mut self, mob: &dyn Entity, level: &Arc<World>) {
        self.memories.forget_outdated();
        self.tick_sensors(mob, level);
        self.start_each_non_running_behavior(mob, level);
        self.tick_each_running_behavior(mob, level);
    }

    /// Stops every running behavior.
    pub fn stop_all(&mut self, mob: &dyn Entity, level: &Arc<World>) {
        let Self {
            memories,
            activities,
            behaviors_by_priority,
            ..
        } = self;
        let mut context = BrainContext::new(level, mob, memories, activities);
        for behavior in running_behaviors(behaviors_by_priority) {
            behavior.do_stop(&mut context);
        }
    }

    fn tick_sensors(&mut self, mob: &dyn Entity, level: &Arc<World>) {
        let Self {
            memories,
            activities,
            sensors,
            ..
        } = self;
        let mut context = BrainContext::new(level, mob, memories, activities);
        for sensor in sensors {
            sensor.tick(&mut context);
        }
    }

    /// Starts every stopped behavior of an active activity, lowest priority first.
    ///
    /// The active activities are read live, so a behavior that switches activity
    /// affects the groups that follow it.
    fn start_each_non_running_behavior(&mut self, mob: &dyn Entity, level: &Arc<World>) {
        let Self {
            memories,
            activities,
            behaviors_by_priority,
            ..
        } = self;
        let mut context = BrainContext::new(level, mob, memories, activities);

        for groups in behaviors_by_priority.values_mut() {
            for group in groups {
                if !context.activities.is_active(group.activity) {
                    continue;
                }

                for behavior in &mut group.behaviors {
                    if behavior.status() == BehaviorStatus::Stopped {
                        behavior.try_start(&mut context);
                    }
                }
            }
        }
    }

    /// Ticks every running behavior, whatever activity it belongs to.
    ///
    /// A behavior that started under an activity the brain has since left keeps
    /// running until it times out or gives up. Switching activity erases that
    /// activity's memories instead of cutting its behaviors off mid-run.
    fn tick_each_running_behavior(&mut self, mob: &dyn Entity, level: &Arc<World>) {
        let Self {
            memories,
            activities,
            behaviors_by_priority,
            ..
        } = self;
        let mut context = BrainContext::new(level, mob, memories, activities);
        for behavior in running_behaviors(behaviors_by_priority) {
            behavior.tick_or_stop(&mut context);
        }
    }
}

impl Default for Brain {
    fn default() -> Self {
        Self::new()
    }
}

/// Orders the activities sharing a priority as a Java `HashMap` keyed by
/// activity iterates them.
///
/// Vanilla keeps them in a `HashMap<Activity, _>` whose hash is the activity
/// name's `String.hashCode`, so that is the order behaviors of equal priority
/// start and tick in. Java walks buckets low to high and keeps insertion order
/// within a bucket, so a stable sort by bucket reproduces it. The table doubles
/// each time it grows past three quarters full.
fn sort_in_java_map_order(groups: &mut [ActivityBehaviors]) {
    let mut capacity = JAVA_MAP_INITIAL_CAPACITY;
    while groups.len() > (capacity * 3 / 4) as usize {
        capacity *= 2;
    }

    groups.sort_by_key(|group| {
        let hash = java_string_hash(&group.activity.key.path);
        (hash ^ (hash >> 16)) & (capacity - 1)
    });
}

/// Java `String.hashCode`, as the unsigned bits `HashMap` spreads.
fn java_string_hash(value: &str) -> u32 {
    value.encode_utf16().fold(0u32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(u32::from(unit))
    })
}

/// Every running behavior, in priority order.
fn running_behaviors(
    behaviors_by_priority: &mut BTreeMap<i32, Vec<ActivityBehaviors>>,
) -> impl Iterator<Item = &mut Box<dyn BehaviorControl>> {
    behaviors_by_priority
        .values_mut()
        .flatten()
        .flat_map(|group| group.behaviors.iter_mut())
        .filter(|behavior| behavior.status() == BehaviorStatus::Running)
}
