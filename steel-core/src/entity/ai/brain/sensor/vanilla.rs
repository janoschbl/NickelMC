//! General-purpose vanilla sensors, independent of species-specific AI.
use super::super::{
    BrainContext,
    memory::{
        MemoryModuleTypeRef, NearestVisibleLivingEntities, RememberedDamage, RememberedEntities,
        RememberedEntity, Unit, vanilla_memories as memories,
    },
};
use super::{Sensor, targeting};
use crate::entity::{Entity, LivingEntity, ai::targeting::TargetingConditions};
use std::sync::Arc;

pub struct DummySensor;
impl Sensor for DummySensor {
    fn do_tick(&mut self, _context: &mut BrainContext<'_>) {}
}

pub struct PlayerSensor;
impl Sensor for PlayerSensor {
    fn requires(&self) -> &[MemoryModuleTypeRef] {
        super::vanilla_sensors::NEAREST_PLAYERS_MEMORIES
    }
    fn do_tick(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_living_entity() else {
            return;
        };
        let range = targeting::follow_range(body);
        let mut players: Vec<crate::entity::SharedEntity> = Vec::new();
        context.level.players.iter_players(|_, player| {
            if !player.is_spectator()
                && body.position().distance_squared(player.position()) < range * range
            {
                let entity: crate::entity::SharedEntity = player.clone();
                players.push(entity);
            }
            true
        });
        players.sort_by(|a, b| {
            body.position()
                .distance_squared(a.position())
                .total_cmp(&body.position().distance_squared(b.position()))
        });
        let visible: Vec<_> = players
            .iter()
            .filter(|player| {
                player
                    .as_living_entity()
                    .is_some_and(|living| targeting::is_entity_targetable(context, living))
            })
            .collect();
        let attackable: Vec<_> = visible
            .iter()
            .copied()
            .filter(|player| {
                player
                    .as_living_entity()
                    .is_some_and(|living| targeting::is_entity_attackable(context, living))
            })
            .collect();
        let nearest = visible.first().map(|entity| RememberedEntity::new(entity));
        let nearest_attackable = attackable
            .first()
            .map(|entity| RememberedEntity::new(entity));
        let attackable: RememberedEntities = attackable.into_iter().collect();
        context
            .memories
            .set(&memories::NEAREST_PLAYERS, players.iter().collect());
        context
            .memories
            .set_optional(&memories::NEAREST_VISIBLE_PLAYER, nearest);
        context.memories.set_optional(
            &memories::NEAREST_VISIBLE_ATTACKABLE_PLAYER,
            nearest_attackable,
        );
        context
            .memories
            .set(&memories::NEAREST_VISIBLE_ATTACKABLE_PLAYERS, attackable);
    }
}

pub struct NearestLivingEntitySensor;
impl Sensor for NearestLivingEntitySensor {
    fn requires(&self) -> &[MemoryModuleTypeRef] {
        super::vanilla_sensors::NEAREST_LIVING_ENTITIES_MEMORIES
    }
    fn do_tick(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_living_entity() else {
            return;
        };
        let range = targeting::follow_range(body);
        let mut nearby = context.level.get_entities_in_aabb_matching(
            &body.bounding_box().inflate(range),
            |entity| {
                entity.uuid() != body.uuid()
                    && entity
                        .as_living_entity()
                        .is_some_and(LivingEntity::is_alive)
            },
        );
        nearby.sort_by(|a, b| {
            body.position()
                .distance_squared(a.position())
                .total_cmp(&body.position().distance_squared(b.position()))
        });
        let weak_body = context
            .level
            .get_entity_by_id(body.id())
            .map(|entity| Arc::downgrade(&entity));
        let weak_level = Arc::downgrade(context.level);
        let visible = NearestVisibleLivingEntities::new(
            nearby.iter().collect(),
            move |target, current_memories| {
                let Some(body) = weak_body.as_ref().and_then(std::sync::Weak::upgrade) else {
                    return false;
                };
                let Some(level) = weak_level.upgrade() else {
                    return false;
                };
                let Some(living) = body.as_living_entity() else {
                    return false;
                };
                let Some(target) = target.as_living_entity() else {
                    return false;
                };
                let mut conditions =
                    TargetingConditions::for_non_combat().range(targeting::follow_range(living));
                if current_memories
                    .get_internal(&memories::ATTACK_TARGET)
                    .flatten()
                    .is_some_and(|remembered| remembered.uuid == target.uuid())
                {
                    conditions = conditions.ignore_invisibility_testing();
                }
                conditions.test(level.as_ref(), Some(living), target)
            },
        );
        context
            .memories
            .set(&memories::NEAREST_LIVING_ENTITIES, nearby.iter().collect());
        context
            .memories
            .set(&memories::NEAREST_VISIBLE_LIVING_ENTITIES, visible);
    }
}

pub struct HurtBySensor;
impl Sensor for HurtBySensor {
    fn requires(&self) -> &[MemoryModuleTypeRef] {
        super::vanilla_sensors::HURT_BY_MEMORIES
    }
    fn do_tick(&mut self, context: &mut BrainContext<'_>) {
        let Some(body) = context.mob.as_living_entity() else {
            return;
        };
        if let Some(source) = body.last_damage_source() {
            context
                .memories
                .set(&memories::HURT_BY, RememberedDamage::from(&source));
            if let Some(entity) = source
                .causing_entity()
                .filter(|entity| entity.as_living_entity().is_some())
            {
                context
                    .memories
                    .set(&memories::HURT_BY_ENTITY, RememberedEntity::new(entity));
            }
        } else {
            context.memories.erase(memories::HURT_BY.entry());
        }
        if context
            .memories
            .get_internal(&memories::HURT_BY_ENTITY)
            .flatten()
            .is_some_and(|entity| {
                entity.resolve().is_none_or(|entity| {
                    !entity
                        .as_living_entity()
                        .is_some_and(LivingEntity::is_alive)
                        || entity
                            .level()
                            .is_none_or(|level| !Arc::ptr_eq(&level, context.level))
                })
            })
        {
            context.memories.erase(memories::HURT_BY_ENTITY.entry());
        }
    }
}

pub struct IsInWaterSensor;
impl Sensor for IsInWaterSensor {
    fn requires(&self) -> &[MemoryModuleTypeRef] {
        super::vanilla_sensors::IS_IN_WATER_MEMORIES
    }
    fn do_tick(&mut self, context: &mut BrainContext<'_>) {
        context.memories.set_optional(
            &memories::IS_IN_WATER,
            context.mob.is_in_water().then_some(Unit(())),
        );
    }
}
