//! Navigation consumes walk-target memories; path snapshots never own entities.
use super::super::{
    BrainContext,
    memory::{
        EntityTracker, Long, MemoryModuleTypeRef, MemoryStatus, NavigationPath,
        vanilla_memories as memories,
    },
};
use super::{Behavior, BehaviorDuration};
use crate::entity::ai::goal::random_pos::default_random_pos_towards;
use crate::entity::ai::path::Path;
use steel_utils::{BlockPos, Downcast as _};

#[derive(Default)]
pub struct MoveToTargetSink {
    remaining_cooldown: i32,
    path: Option<Path>,
    last_target_pos: Option<BlockPos>,
    speed_modifier: f32,
}

impl MoveToTargetSink {
    fn reached(context: &BrainContext<'_>) -> bool {
        context
            .memories
            .get(&memories::WALK_TARGET)
            .is_some_and(|target| {
                target.target.current_block_position().is_some_and(|pos| {
                    let body = context.mob.block_position();
                    (pos.x() - body.x()).abs()
                        + (pos.y() - body.y()).abs()
                        + (pos.z() - body.z()).abs()
                        <= target.close_enough_dist
                })
            })
    }
    fn compute_path(&mut self, context: &mut BrainContext<'_>) -> bool {
        let Some(body) = context.mob.as_pathfinder_mob() else {
            return false;
        };
        let Some(target) = context.memories.get(&memories::WALK_TARGET) else {
            return false;
        };
        let Some(pos) = target.target.current_block_position() else {
            return false;
        };
        self.path = body.create_path_to(pos, 0);
        self.speed_modifier = target.speed_modifier;
        if Self::reached(context) {
            context
                .memories
                .erase(memories::CANT_REACH_WALK_TARGET_SINCE.entry());
            return false;
        }
        if self.path.as_ref().is_some_and(Path::can_reach) {
            context
                .memories
                .erase(memories::CANT_REACH_WALK_TARGET_SINCE.entry());
        } else if !context
            .memories
            .has_value(memories::CANT_REACH_WALK_TARGET_SINCE.entry())
        {
            context
                .memories
                .set(&memories::CANT_REACH_WALK_TARGET_SINCE, Long(context.time));
        }
        if self.path.is_some() {
            return true;
        }
        let (x, y, z) = pos.get_bottom_center();
        if let Some(partial) = default_random_pos_towards(
            body,
            10,
            7,
            glam::DVec3::new(x, y, z),
            f64::from(std::f32::consts::FRAC_PI_2),
        ) {
            self.path = body.create_path_to(BlockPos::from(partial), 0);
        }
        self.path.is_some()
    }
}
impl Behavior for MoveToTargetSink {
    fn duration(&self) -> BehaviorDuration {
        BehaviorDuration::range(150, 250)
    }
    fn entry_condition(&self) -> &[(MemoryModuleTypeRef, MemoryStatus)] {
        &[
            (
                memories::CANT_REACH_WALK_TARGET_SINCE.entry(),
                MemoryStatus::Registered,
            ),
            (memories::PATH.entry(), MemoryStatus::ValueAbsent),
            (memories::WALK_TARGET.entry(), MemoryStatus::ValuePresent),
        ]
    }
    fn check_extra_start_conditions(&mut self, context: &mut BrainContext<'_>) -> bool {
        if context.mob.as_pathfinder_mob().is_none() {
            return false;
        }
        if self.remaining_cooldown > 0 {
            self.remaining_cooldown -= 1;
            return false;
        }
        let reached = Self::reached(context);
        if !reached && self.compute_path(context) {
            self.last_target_pos = context
                .memories
                .get(&memories::WALK_TARGET)
                .and_then(|target| target.target.current_block_position());
            return true;
        }
        context.memories.erase(memories::WALK_TARGET.entry());
        if reached {
            context
                .memories
                .erase(memories::CANT_REACH_WALK_TARGET_SINCE.entry());
        }
        false
    }
    fn can_still_use(&mut self, context: &mut BrainContext<'_>) -> bool {
        let Some(body) = context.mob.as_pathfinder_mob() else {
            return false;
        };
        let spectator = context
            .memories
            .get(&memories::WALK_TARGET)
            .is_some_and(|target| {
                target
                    .target
                    .downcast_ref::<EntityTracker>()
                    .is_some_and(|tracker| {
                        tracker
                            .entity
                            .resolve()
                            .is_none_or(|entity| entity.is_spectator())
                    })
            });
        self.path.is_some()
            && self.last_target_pos.is_some()
            && !body.mob_base().navigation().lock().is_done()
            && context.memories.has_value(memories::WALK_TARGET.entry())
            && !Self::reached(context)
            && !spectator
    }
    fn start(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_pathfinder_mob() else {
            return;
        };
        context
            .memories
            .set_optional(&memories::PATH, self.path.clone().map(NavigationPath));
        body.move_to_path(self.path.clone(), f64::from(self.speed_modifier));
    }
    fn stop(&mut self, context: &mut BrainContext<'_>) {
        if let Some(body) = context.mob.as_pathfinder_mob() {
            if context.memories.has_value(memories::WALK_TARGET.entry())
                && !Self::reached(context)
                && body.mob_base().navigation().lock().is_stuck()
            {
                self.remaining_cooldown = rand::random_range(0..40);
            }
            body.mob_base().navigation().lock().stop();
        }
        context.memories.erase(memories::WALK_TARGET.entry());
        context.memories.erase(memories::PATH.entry());
        self.path = None;
    }
    fn tick(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_pathfinder_mob() else {
            return;
        };
        let path = body.mob_base().navigation().lock().path().cloned();
        if self.path != path {
            self.path = path;
            context
                .memories
                .set_optional(&memories::PATH, self.path.clone().map(NavigationPath));
        }
        let Some(target) = context
            .memories
            .get(&memories::WALK_TARGET)
            .and_then(|target| target.target.current_block_position())
        else {
            return;
        };
        if self.path.is_some()
            && self.last_target_pos.is_some_and(|old| {
                let delta = glam::IVec3::new(
                    target.x() - old.x(),
                    target.y() - old.y(),
                    target.z() - old.z(),
                );
                delta.length_squared() > 4
            })
            && self.compute_path(context)
        {
            self.last_target_pos = Some(target);
            self.start(context);
        }
    }
}
