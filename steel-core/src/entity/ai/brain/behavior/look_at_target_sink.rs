use super::super::{
    BrainContext,
    memory::{
        MemoryModuleTypeRef, MemoryStatus, NearestVisibleLivingEntities,
        vanilla_memories as memories,
    },
};
use super::{Behavior, BehaviorDuration};

pub struct LookAtTargetSink {
    duration: BehaviorDuration,
}
impl LookAtTargetSink {
    pub fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            duration: BehaviorDuration::range(min_duration, max_duration),
        }
    }
}
impl Behavior for LookAtTargetSink {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        const { &[(memories::LOOK_TARGET.entry(), MemoryStatus::ValuePresent)] }
    }
    fn duration(&self) -> BehaviorDuration {
        self.duration
    }
    fn check_extra_start_conditions(&mut self, context: &mut BrainContext<'_>) -> bool {
        context.mob.as_mob().is_some()
    }
    fn can_still_use(&mut self, context: &mut BrainContext<'_>) -> bool {
        context
            .memories
            .get(&memories::LOOK_TARGET)
            .is_some_and(|target| {
                target.0.is_visible_by(
                    context
                        .memories
                        .get_internal(&memories::NEAREST_VISIBLE_LIVING_ENTITIES)
                        .flatten()
                        .unwrap_or_else(|| NearestVisibleLivingEntities::empty()),
                    context.memories,
                )
            })
    }
    fn stop(&mut self, context: &mut BrainContext<'_>) {
        context.memories.erase(memories::LOOK_TARGET.entry());
    }
    fn tick(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_mob() else {
            return;
        };
        if let Some(position) = context
            .memories
            .get(&memories::LOOK_TARGET)
            .and_then(|target| target.0.current_position())
        {
            body.mob_base().controls().lock().look_control.set_look_at(
                position,
                body.max_head_y_rot(),
                body.max_head_x_rot(),
            );
        }
    }
}
