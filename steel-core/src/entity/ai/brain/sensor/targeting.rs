//! Sensor targeting conditions evaluated with the body's current follow range.
use super::super::{BrainContext, memory::vanilla_memories as memories};
use crate::entity::{LivingEntity, ai::targeting::TargetingConditions};
use steel_registry::vanilla_attributes;

pub fn follow_range(body: &dyn LivingEntity) -> f64 {
    body.attributes()
        .lock()
        .required_value(vanilla_attributes::FOLLOW_RANGE)
}

pub fn is_entity_targetable(context: &BrainContext<'_>, target: &dyn LivingEntity) -> bool {
    test(context, target, false, false)
}
pub fn is_entity_attackable(context: &BrainContext<'_>, target: &dyn LivingEntity) -> bool {
    test(context, target, true, false)
}
pub fn is_entity_attackable_ignoring_line_of_sight(
    context: &BrainContext<'_>,
    target: &dyn LivingEntity,
) -> bool {
    test(context, target, true, true)
}
fn test(
    context: &BrainContext<'_>,
    target: &dyn LivingEntity,
    combat: bool,
    ignore_sight: bool,
) -> bool {
    let Some(body) = context.mob.as_living_entity() else {
        return false;
    };
    let mut conditions = if combat {
        TargetingConditions::for_combat()
    } else {
        TargetingConditions::for_non_combat()
    }
    .range(follow_range(body));
    if context
        .memories
        .get_internal(&memories::ATTACK_TARGET)
        .flatten()
        .is_some_and(|remembered| remembered.uuid == target.uuid())
    {
        conditions = conditions.ignore_invisibility_testing();
    }
    if ignore_sight {
        conditions = conditions.ignore_line_of_sight();
    }
    conditions.test(context.level.as_ref(), Some(body), target)
}

/// Keeps a positive result for the next `invocations` negative evaluations.
#[derive(Default)]
pub struct RememberPositives {
    remaining: i32,
}
impl RememberPositives {
    pub fn test(&mut self, invocations: i32, predicate: impl FnOnce() -> bool) -> bool {
        if predicate() {
            self.remaining = invocations;
            true
        } else {
            self.remaining = self.remaining.saturating_sub(1);
            self.remaining >= 0
        }
    }
}
