use std::f64::consts::PI;

use glam::Vec3;
use steel_math::trig;
use steel_registry::entity_data::{EntityData::Vector3, Vector3f};
use steel_utils::{
    Downcast,
    random::{Random, legacy_random::LegacyRandom},
};

use crate::entity::{
    Entity, LivingEntity,
    ai::goal::{
        random_stroll::RandomStrollGoal,
        selector::{Goal, GoalControls},
    },
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
    fn controls(&self) -> super::selector::GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, _mob: &dyn crate::entity::PathfinderMob) -> bool {
        // Always true in Squid.java
        true
    }

    fn tick(&mut self, mob: &dyn crate::entity::PathfinderMob) {
        let Some(squid) = mob.downcast_ref::<SquidEntity>() else {
            tracing::warn!("SquidRandomMovementGoal assigned to non-squid entity");
            return;
        };

        if squid.no_action_time() > 100 {
            squid.set_movement_vector(Vec3::ZERO);
            return;
        }

        if squid.random_next_i32_bounded(50) != 0
            && squid.is_in_water() // TODO: Should be was_in_water() according to vanilla code
            && squid.has_movement_vector()
        {
            return;
        }

        let angle = f64::from(squid.random_next_f32()) * PI * 2.0;
        let movement = Vec3::new(
            trig::cos(angle) as f32 * 0.2,
            -0.1 + squid.random_next_f32() * 0.2,
            trig::sin(angle) as f32 * 0.2,
        );

        squid.set_movement_vector(movement);
    }
}
