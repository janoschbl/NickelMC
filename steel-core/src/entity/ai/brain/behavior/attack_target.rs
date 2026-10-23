//! Target acquisition, validation and pursuit memory writers.
use super::super::{
    BrainContext,
    memory::{
        EntityTracker, LookTarget, MemoryModuleTypeRef, MemoryStatus, RememberedEntity, WalkTarget,
        vanilla_memories as memories,
    },
};
use super::Behavior;
use crate::entity::SharedEntity;
use std::sync::Arc;

type TargetFinder = Box<dyn Fn(&BrainContext<'_>) -> Option<SharedEntity> + Send>;
pub struct StartAttacking {
    find_target: TargetFinder,
}
impl StartAttacking {
    pub fn new(
        find_target: impl Fn(&BrainContext<'_>) -> Option<SharedEntity> + Send + 'static,
    ) -> Self {
        Self {
            find_target: Box::new(find_target),
        }
    }
}
impl Behavior for StartAttacking {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        &[
            (memories::ATTACK_TARGET.entry(), MemoryStatus::ValueAbsent),
            (
                memories::CANT_REACH_WALK_TARGET_SINCE.entry(),
                MemoryStatus::Registered,
            ),
        ]
    }
    fn check_extra_start_conditions(&mut self, context: &mut BrainContext<'_>) -> bool {
        let Some(body) = context.mob.as_mob() else {
            return false;
        };
        let Some(target) = (self.find_target)(context) else {
            return false;
        };
        let Some(living) = target.as_living_entity() else {
            return false;
        };
        if !crate::entity::Mob::can_attack(body, living) {
            return false;
        }
        context
            .memories
            .set(&memories::ATTACK_TARGET, RememberedEntity::new(&target));
        context
            .memories
            .erase(memories::CANT_REACH_WALK_TARGET_SINCE.entry());
        true
    }
}

type StopCondition = Box<dyn Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send>;
pub struct StopAttackingIfTargetInvalid {
    stop_when: StopCondition,
}
impl StopAttackingIfTargetInvalid {
    pub fn new(
        stop_when: impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static,
    ) -> Self {
        Self {
            stop_when: Box::new(stop_when),
        }
    }
}
impl Default for StopAttackingIfTargetInvalid {
    fn default() -> Self {
        Self::new(|_, _| false)
    }
}
impl Behavior for StopAttackingIfTargetInvalid {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        &[
            (memories::ATTACK_TARGET.entry(), MemoryStatus::ValuePresent),
            (
                memories::CANT_REACH_WALK_TARGET_SINCE.entry(),
                MemoryStatus::Registered,
            ),
        ]
    }
    fn check_extra_start_conditions(&mut self, context: &mut BrainContext<'_>) -> bool {
        let Some(body) = context.mob.as_mob() else {
            return false;
        };
        let valid = context
            .memories
            .get(&memories::ATTACK_TARGET)
            .and_then(RememberedEntity::resolve)
            .is_some_and(|target| {
                target.as_living_entity().is_some_and(|living| {
                    living.is_alive() && crate::entity::Mob::can_attack(body, living)
                }) && target
                    .level()
                    .is_some_and(|level| Arc::ptr_eq(&level, context.level))
                    && !context
                        .memories
                        .get(&memories::CANT_REACH_WALK_TARGET_SINCE)
                        .is_some_and(|since| context.time - since.0 > 200)
                    && !(self.stop_when)(context, &target)
            });
        if !valid {
            context.memories.erase(memories::ATTACK_TARGET.entry());
        }
        true
    }
}

/// Species supplies attack-range policy (melee or usable ranged weapon).
pub struct SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    speed: f32,
    in_attack_range: StopCondition,
}
impl SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    pub fn new(
        speed: f32,
        in_attack_range: impl Fn(&BrainContext<'_>, &SharedEntity) -> bool + Send + 'static,
    ) -> Self {
        Self {
            speed,
            in_attack_range: Box::new(in_attack_range),
        }
    }
}
impl Behavior for SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        &[
            (memories::WALK_TARGET.entry(), MemoryStatus::Registered),
            (memories::LOOK_TARGET.entry(), MemoryStatus::Registered),
            (memories::ATTACK_TARGET.entry(), MemoryStatus::ValuePresent),
            (
                memories::NEAREST_VISIBLE_LIVING_ENTITIES.entry(),
                MemoryStatus::Registered,
            ),
        ]
    }
    fn check_extra_start_conditions(&mut self, context: &mut BrainContext<'_>) -> bool {
        if context.mob.as_mob().is_none() {
            return false;
        }
        let Some(target) = context
            .memories
            .get(&memories::ATTACK_TARGET)
            .and_then(RememberedEntity::resolve)
        else {
            return false;
        };
        if context
            .memories
            .get(&memories::NEAREST_VISIBLE_LIVING_ENTITIES)
            .is_some_and(|visible| visible.contains(&target))
            && (self.in_attack_range)(context, &target)
        {
            context.memories.erase(memories::WALK_TARGET.entry());
        } else {
            context.memories.set(
                &memories::LOOK_TARGET,
                LookTarget(Box::new(EntityTracker {
                    entity: RememberedEntity::new(&target),
                    track_eye_height: true,
                    target_eye_height: false,
                })),
            );
            context.memories.set(
                &memories::WALK_TARGET,
                WalkTarget {
                    target: Box::new(EntityTracker {
                        entity: RememberedEntity::new(&target),
                        track_eye_height: false,
                        target_eye_height: false,
                    }),
                    speed_modifier: self.speed,
                    close_enough_dist: 0,
                },
            );
        }
        true
    }
}
