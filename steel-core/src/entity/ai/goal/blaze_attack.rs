use glam::DVec3;
use std::sync::Arc;
use steel_registry::{level_events, vanilla_attributes};
use steel_utils::Downcast;

use crate::entity::{
    Entity, LivingEntity, Mob, PathfinderMob, Projectile, SharedEntity,
    ai::goal::{
        GoalControl,
        selector::{Goal, GoalControls},
    },
    entities::{BlazeEntity, SmallFireballEntity},
    next_entity_id,
    projectile::{AbstractHurtingProjectile, triangle_random},
};

pub struct BlazeAttackGoal {
    attack_step: i32,
    attack_time: i32,
    last_seen: i32,
}

impl BlazeAttackGoal {
    pub fn new() -> Self {
        Self {
            attack_step: 0,
            attack_time: 0,
            last_seen: 0,
        }
    }

    fn get_follow_distance(&self, mob: &dyn PathfinderMob) -> f64 {
        mob.attributes()
            .lock()
            .get_value(vanilla_attributes::FOLLOW_RANGE)
            .unwrap_or_default()
    }
}

impl Goal for BlazeAttackGoal {
    fn controls(&self) -> GoalControls {
        let mut controls = GoalControls::MOVE;
        controls.insert(GoalControl::Look);
        controls
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let target = mob.target();
        if let Some(target) = target.as_deref().and_then(Entity::as_living_entity)
            && LivingEntity::is_alive(target)
            && Mob::can_attack(mob, target)
        {
            return true;
        }
        false
    }

    fn start(&mut self, _mob: &dyn PathfinderMob) {
        self.attack_step = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(blaze) = mob.downcast_ref::<BlazeEntity>() {
            blaze.set_charged(false);
        }
        self.last_seen = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn PathfinderMob, entity: &SharedEntity) {
        self.attack_time -= 1;
        let Some(blaze) = mob.downcast_ref::<BlazeEntity>() else {
            return;
        };
        if let Some(target) = blaze.target() {
            let has_line_of_sight = blaze.has_line_of_sight_cached(&*target);
            if has_line_of_sight {
                self.last_seen = 0
            } else {
                self.last_seen += 1;
            }

            let distance = blaze.distance_to_sqr(target.position());
            if distance < 4.0 {
                if !has_line_of_sight {
                    return;
                }

                if let Some(world) = blaze.level()
                    && self.attack_time <= 0
                {
                    self.attack_time = 20;
                    let _ = blaze.do_hurt_target(entity, &world, &target);
                }

                blaze.set_wanted_position(target.position(), 1.0);
            } else if distance < self.get_follow_distance(mob) * self.get_follow_distance(mob)
                && has_line_of_sight
            {
                if self.attack_time <= 0 {
                    self.attack_step += 1;
                    if self.attack_step == 1 {
                        self.attack_time = 60;
                        blaze.set_charged(true);
                    } else if self.attack_step <= 4 {
                        self.attack_time = 6;
                    } else {
                        self.attack_time = 100;
                        self.attack_step = 0;
                        blaze.set_charged(false);
                    }

                    if self.attack_step > 1 {
                        if let Some(level) = blaze.level()
                            && !blaze.is_silent()
                        {
                            level.level_event(
                                level_events::SOUND_BLAZE_FIREBALL,
                                blaze.block_position(),
                                0,
                                None,
                            );
                        }

                        let xd = target.position().x - blaze.position().x;
                        let yd = (target.position().y + target.bounding_box().height() * 0.5)
                            - (blaze.position().y + blaze.bounding_box().height() * 0.5);
                        let zd = target.position().z - blaze.position().z;
                        let sqr_dist = distance.sqrt().sqrt() * 0.5;
                        let direction = DVec3::new(
                            triangle_random(xd, 2.297 * sqr_dist),
                            yd,
                            triangle_random(zd, 2.297 * sqr_dist),
                        );
                        if let Some(world) = blaze.level() {
                            let position = blaze.position()
                                + DVec3::Y * (blaze.bounding_box().height() * 0.5 + 0.5);
                            let fireball = SmallFireballEntity::new(
                                &steel_registry::vanilla_entities::SMALL_FIREBALL,
                                next_entity_id(),
                                position,
                                Arc::downgrade(&world),
                            );
                            fireball.set_owner_entity(Some(entity));
                            fireball.set_rotation(blaze.rotation());
                            fireball.assign_directional_movement(direction.normalize_or_zero());
                            if let Err(error) = world.try_add_entity(Arc::new(fireball)) {
                                log::debug!("failed to spawn blaze fireball: {error}");
                            }
                        }
                    }
                }

                blaze.mob_base().controls().lock().look_control.set_look_at(
                    DVec3::new(target.position().x, target.get_eye_y(), target.position().z),
                    10.0,
                    10.0,
                );
            } else if self.last_seen < 5 {
                blaze.set_wanted_position(target.position(), 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        behavior::init_behaviors,
        entity::entities::PigEntity,
        test_support::{fresh_test_world, insert_ready_full_chunk},
    };
    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;

    #[test]
    fn blaze_volley_charges_fires_three_shots_then_cools_down() {
        init_vanilla_registry();
        init_behaviors();
        let fixture = fresh_test_world("blaze_volley_timing");
        let world = &fixture.world;
        insert_ready_full_chunk(world, ChunkPos::new(0, 0));
        let blaze = Arc::new(BlazeEntity::new(
            &vanilla_entities::BLAZE,
            next_entity_id(),
            DVec3::new(4.0, 80.0, 8.0),
            Arc::downgrade(world),
        ));
        let entity: SharedEntity = Arc::clone(&blaze) as SharedEntity;
        let target: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(12.0, 80.0, 8.0),
            Arc::downgrade(world),
        ));
        world
            .try_add_entity(Arc::clone(&target))
            .expect("attach target");
        blaze.set_target(Some(&target));
        let mut goal = BlazeAttackGoal::new();
        assert!(goal.can_use(blaze.as_ref()));
        goal.start(blaze.as_ref());
        let mut fired_ticks = Vec::new();
        let mut previous = 0;
        for tick in 0..179 {
            goal.tick(blaze.as_ref(), &entity);
            let count = world
                .get_entities_in_aabb(&blaze.bounding_box().inflate(2.0))
                .iter()
                .filter(|entity| entity.entity_type() == &vanilla_entities::SMALL_FIREBALL)
                .count();
            if count != previous {
                fired_ticks.push(tick);
                previous = count;
            }
            if tick == 0 {
                assert!(blaze.is_charged());
                assert_eq!(goal.attack_time, 60);
            }
            if tick == 78 {
                assert!(!blaze.is_charged());
                assert_eq!(goal.attack_time, 100);
            }
        }
        assert_eq!(fired_ticks, [60, 66, 72]);
        assert!(blaze.is_charged());
        goal.stop(blaze.as_ref());
        assert!(!blaze.is_charged());
    }
}
