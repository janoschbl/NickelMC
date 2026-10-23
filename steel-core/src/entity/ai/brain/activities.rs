//! Which activities a brain may run and which are running.

use std::ptr;

use steel_registry::activity::ActivityRef;
use steel_registry::vanilla_activities;

use super::behavior::BehaviorControl;
use super::memory::{Memories, MemoryModuleTypeRef, MemoryStatus};

/// Vanilla `Brain.SCHEDULE_UPDATE_DELAY`.
const SCHEDULE_UPDATE_DELAY: i64 = 20;

/// Memory states a brain requires before it switches to an activity.
pub type ActivityRequirements = Vec<(MemoryModuleTypeRef, MemoryStatus)>;

/// One activity's behaviors, requirements, and memories to erase.
///
/// Mirrors vanilla `ActivityData`.
pub struct ActivityData {
    pub(super) activity: ActivityRef,
    pub(super) behaviors: Vec<(i32, Box<dyn BehaviorControl>)>,
    pub(super) requirements: ActivityRequirements,
    pub(super) memories_to_erase_when_stopped: Vec<MemoryModuleTypeRef>,
}

impl ActivityData {
    /// Behaviors at consecutive priorities starting at `priority_of_first_behavior`.
    #[must_use]
    pub fn new(
        activity: ActivityRef,
        priority_of_first_behavior: i32,
        behaviors: Vec<Box<dyn BehaviorControl>>,
    ) -> Self {
        let mut next_priority = priority_of_first_behavior;
        let behaviors = behaviors
            .into_iter()
            .map(|behavior| {
                let prioritized = (next_priority, behavior);
                next_priority += 1;
                prioritized
            })
            .collect();

        Self::prioritized(activity, behaviors)
    }

    /// Behaviors at explicit priorities, which several behaviors may share.
    #[must_use]
    pub fn prioritized(
        activity: ActivityRef,
        behaviors: Vec<(i32, Box<dyn BehaviorControl>)>,
    ) -> Self {
        Self {
            activity,
            behaviors,
            requirements: Vec::new(),
            memories_to_erase_when_stopped: Vec::new(),
        }
    }

    /// Memory states the brain requires before it will switch to this activity.
    #[must_use]
    pub fn requiring(mut self, requirements: ActivityRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    /// Memories cleared when the brain switches away from this activity.
    #[must_use]
    pub fn erasing_when_stopped(mut self, memories: Vec<MemoryModuleTypeRef>) -> Self {
        self.memories_to_erase_when_stopped = memories;
        self
    }
}

/// The activity state of a brain: what each activity needs, and which are running.
///
/// Activities are compared by the address of their registry entry, like
/// [memories](Memories). Switching activity reads and erases memories, so those
/// methods take the mob's [`Memories`]. Behaviors reach this through
/// [`BrainContext`](super::BrainContext).
pub struct Activities {
    requirements: Vec<(ActivityRef, ActivityRequirements)>,
    memories_to_erase_when_stopped: Vec<(ActivityRef, Vec<MemoryModuleTypeRef>)>,
    core: Vec<ActivityRef>,
    active: Vec<ActivityRef>,
    default: ActivityRef,
    last_schedule_update: i64,
}

impl Activities {
    /// No registered activities, with the core activity set to
    /// [`CORE`](vanilla_activities::CORE) and the default to
    /// [`IDLE`](vanilla_activities::IDLE).
    ///
    /// Nothing is active until the brain performs its first activity switch.
    #[must_use]
    pub fn new() -> Self {
        Self {
            requirements: Vec::new(),
            memories_to_erase_when_stopped: Vec::new(),
            core: vec![&vanilla_activities::CORE],
            active: Vec::new(),
            default: &vanilla_activities::IDLE,
            last_schedule_update: -9999,
        }
    }

    /// Registers an activity's requirements and memories to erase, replacing any
    /// registered earlier.
    pub(super) fn register(
        &mut self,
        activity: ActivityRef,
        requirements: ActivityRequirements,
        memories_to_erase_when_stopped: Vec<MemoryModuleTypeRef>,
    ) {
        replace_entry(&mut self.requirements, activity, requirements);
        if !memories_to_erase_when_stopped.is_empty() {
            replace_entry(
                &mut self.memories_to_erase_when_stopped,
                activity,
                memories_to_erase_when_stopped,
            );
        }
    }

    /// Sets the activities that stay active alongside the current one.
    pub fn set_core_activities(&mut self, activities: Vec<ActivityRef>) {
        self.core = activities;
    }

    /// Sets the activity the brain falls back to.
    pub const fn set_default_activity(&mut self, activity: ActivityRef) {
        self.default = activity;
    }

    /// Whether `activity` is currently active.
    #[must_use]
    pub fn is_active(&self, activity: ActivityRef) -> bool {
        contains(&self.active, activity)
    }

    /// The active activity that is not a core one.
    #[must_use]
    pub fn active_non_core_activity(&self) -> Option<ActivityRef> {
        self.active
            .iter()
            .copied()
            .find(|&active| !contains(&self.core, active))
    }

    /// Switches to the default activity.
    pub fn use_default_activity(&mut self, memories: &mut Memories) {
        self.set_active_activity(memories, self.default);
    }

    /// Switches to `activity`, or to the default when its requirements fail.
    pub fn set_active_activity_if_possible(
        &mut self,
        memories: &mut Memories,
        activity: ActivityRef,
    ) {
        if self.requirements_are_met(memories, activity) {
            self.set_active_activity(memories, activity);
        } else {
            self.use_default_activity(memories);
        }
    }

    /// Switches to the first activity whose requirements are met.
    pub fn set_active_activity_to_first_valid(
        &mut self,
        memories: &mut Memories,
        activities: &[ActivityRef],
    ) {
        for &activity in activities {
            if self.requirements_are_met(memories, activity) {
                self.set_active_activity(memories, activity);
                return;
            }
        }
    }

    /// Re-evaluates the scheduled activity at most every 20 ticks.
    ///
    /// `scheduled` yields the activity the mob's schedule asks for at its
    /// position, and runs only when the throttle allows an update.
    pub fn update_activity_from_schedule(
        &mut self,
        memories: &mut Memories,
        game_time: i64,
        scheduled: impl FnOnce() -> ActivityRef,
    ) {
        if game_time - self.last_schedule_update <= SCHEDULE_UPDATE_DELAY {
            return;
        }

        self.last_schedule_update = game_time;
        let scheduled = scheduled();
        if !self.is_active(scheduled) {
            self.set_active_activity_if_possible(memories, scheduled);
        }
    }

    fn set_active_activity(&mut self, memories: &mut Memories, activity: ActivityRef) {
        if self.is_active(activity) {
            return;
        }

        self.erase_memories_for_other_activities_than(memories, activity);
        self.active.clear();
        self.active.extend(self.core.iter().copied());
        self.active.push(activity);
    }

    /// Erases memories for activities that are no longer active.
    fn erase_memories_for_other_activities_than(
        &self,
        memories: &mut Memories,
        activity: ActivityRef,
    ) {
        for &stopped in &self.active {
            if ptr::eq(stopped, activity) {
                continue;
            }

            let to_erase = self
                .memories_to_erase_when_stopped
                .iter()
                .find(|(registered, _)| ptr::eq(*registered, stopped));
            for &memory in to_erase.into_iter().flat_map(|(_, memories)| memories) {
                memories.erase(memory);
            }
        }
    }

    /// Whether the brain may switch to `activity`.
    ///
    /// An activity with no registered behaviors has no requirements entry and is
    /// unreachable, so an unknown activity falls back to the default rather than
    /// becoming silently available.
    fn requirements_are_met(&self, memories: &Memories, activity: ActivityRef) -> bool {
        let Some((_, requirements)) = self
            .requirements
            .iter()
            .find(|(registered, _)| ptr::eq(*registered, activity))
        else {
            return false;
        };

        requirements
            .iter()
            .all(|&(memory, status)| memories.check(memory, status))
    }
}

impl Default for Activities {
    fn default() -> Self {
        Self::new()
    }
}

fn contains(activities: &[ActivityRef], activity: ActivityRef) -> bool {
    activities.iter().any(|&other| ptr::eq(other, activity))
}

fn replace_entry<T>(entries: &mut Vec<(ActivityRef, T)>, activity: ActivityRef, value: T) {
    match entries
        .iter_mut()
        .find(|(registered, _)| ptr::eq(*registered, activity))
    {
        Some((_, existing)) => *existing = value,
        None => entries.push((activity, value)),
    }
}
