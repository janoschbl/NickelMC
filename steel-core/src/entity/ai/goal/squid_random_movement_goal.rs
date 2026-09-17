use std::f64::consts::PI;

use glam::{DVec3, Vec3};
use steel_math::trig;
use steel_utils::Downcast;

use crate::entity::{
    Entity, LivingEntity, PathfinderMob,
    ai::goal::selector::{Goal, GoalControls},
    entities::SquidEntity,
};

pub struct SquidRandomMovementGoal {}

impl SquidRandomMovementGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {}
    }
}

impl Goal for SquidRandomMovementGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, _mob: &dyn PathfinderMob) -> bool {
        // Always true in Squid.java
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(squid) = mob.downcast_ref::<SquidEntity>() else {
            tracing::warn!("SquidRandomMovementGoal assigned to non-squid entity");
            return;
        };

        if squid.no_action_time() > 100 {
            squid.set_movement_vector(DVec3::ZERO);
            return;
        }

        if squid.random_next_i32_bounded(50) != 0
            && squid.is_in_water() // WasInWater() in java seems to be functionally equivilent to this?
            && squid.has_movement_vector()
        {
            return;
        }

        let angle = f64::from(squid.random_next_f32()) * PI * 2.0;
        let movement = Vec3::new(
            trig::cos(angle) as f32 * 0.2,
            -0.1 + squid.random_next_f32() * 0.2,
            trig::sin(angle) as f32 * 0.2,
        )
        .into();

        squid.set_movement_vector(movement);
    }
}
