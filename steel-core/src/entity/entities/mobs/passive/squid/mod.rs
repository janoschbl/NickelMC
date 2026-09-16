use std::{f32::consts::PI, ops::Add, sync::Weak};

use glam::{DVec3, Vec3};
use steel_macros::entity_behavior;
use steel_registry::{
    entity_data::{EntityPose, ParticleData},
    entity_type::{EntityAttachments, EntityDimensions, EntityTypeRef},
    sound_events,
    vanilla_entity_data::SquidEntityData,
    vanilla_mob_effects::LEVITATION,
    vanilla_particle_types,
};
use steel_utils::{
    DowncastType, DowncastTypeKey, entity_events,
    locks::SyncMutex,
    random::{Random, legacy_random::LegacyRandom},
};

use crate::entity::{AnimalBase, ai::goal::SquidFleeGoal};
use crate::{
    entity::{
        AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntitySyncedData,
        LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob,
        ai::goal::SquidRandomMovementGoal,
    },
    physics::{MoveResult, MoverType},
    world::World,
};

const SQUID_BABY_WIDTH: f32 = 0.5;
const SQUID_BABY_HEIGHT: f32 = 0.5;
const SQUID_BABY_EYE_HEIGHT: f32 = 0.37;

const SQUID_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    SQUID_BABY_WIDTH,
    SQUID_BABY_HEIGHT,
    SQUID_BABY_EYE_HEIGHT,
    EntityAttachments::fallback(),
);

#[entity_behavior(class = "Squid")]
pub struct SquidEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<SquidEntityData>,

    state: SyncMutex<SquidState>,
}

pub struct SquidState {
    movement_vector: DVec3,
    // I don't know if we want random here, or if its okay to just re-create it in SquidRandomMovementGoal
    random: LegacyRandom,
    tentacle_speed: f32,
    tentacle_movement: f32,
}

const SQUID_AIR_DRAG: f64 = 0.98;

unsafe impl DowncastType for SquidEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/squid");
}

impl SquidEntity {
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    pub fn has_movement_vector(&self) -> bool {
        self.state.lock().movement_vector.length_squared() > 1.0e-5_f64
    }

    pub fn set_movement_vector(&self, new_vec: DVec3) {
        self.state.lock().movement_vector = new_vec;
    }

    pub fn random_next_f32(&self) -> f32 {
        self.state.lock().random.next_f32()
    }

    pub fn random_next_i32_bounded(&self, bound: i32) -> i32 {
        self.state.lock().random.next_i32_bounded(bound)
    }

    pub fn movement_vector(&self) -> DVec3 {
        self.state.lock().movement_vector
    }

    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn rotate_vector(&self, vector: DVec3) -> DVec3 {
        let (yaw, pitch) = self.rotation();

        vector
            .rotate_x(pitch.to_radians() as f64)
            .rotate_y(-yaw.to_radians() as f64)
    }

    fn spawn_ink(&self) {
        let Some(world) = self.level() else {
            return;
        };

        self.make_sound(Some(&sound_events::ENTITY_SQUID_SQUIRT));

        let position = self.position() + DVec3::new(0.0, -1.0, 0.0);
        let particle_position = position + DVec3::new(0.0, 0.5, 0.0);
        let particle = ParticleData::simple(&vanilla_particle_types::SQUID_INK);

        for _ in 0..30 {
            let direction = self.rotate_vector(DVec3::new(
                self.random_next_f32() as f64 * 0.6 - 0.3,
                -1.0 as f64,
                self.random_next_f32() as f64 * 0.6 - 0.3,
            ));
            let position_scale = if AgeableMob::is_baby(self) { 0.1 } else { 0.3 };
            let offset = direction * (position_scale + self.random_next_f32() * 2.0) as f64;
            world.send_particles(particle.clone(), particle_position, 0, offset, 0.1);
        }
    }

    fn tick_squid_movement(&self) {
        let (tentacle_movement, movement_vector, animation_synch) = {
            let mut state = self.state.lock();

            state.tentacle_movement += state.tentacle_speed;

            let animation_synch = state.tentacle_movement > std::f32::consts::TAU;

            if animation_synch {
                state.tentacle_movement -= std::f32::consts::TAU;

                if state.random.next_i32_bounded(10) == 0 {
                    state.tentacle_speed = 1.0 / (state.random.next_f32() + 1.0) * 0.2;
                }
            }

            (
                state.tentacle_movement,
                state.movement_vector,
                animation_synch,
            )
        };

        if animation_synch {
            self.broadcast_entity_event(entity_events::EntityStatus::SquidAnimSynch);
        }

        if self.is_in_water() {
            if tentacle_movement < PI {
                let tentacle_scale = tentacle_movement / PI;

                if tentacle_scale > 0.75 {
                    self.set_velocity(DVec3::new(
                        f64::from(movement_vector.x),
                        f64::from(movement_vector.y),
                        f64::from(movement_vector.z),
                    ));
                }
            } else {
                self.set_velocity(self.velocity() * 0.9);
            }
            return;
        }

        let velocity = self.velocity();
        let y = self
            .mob_effect(LEVITATION)
            .map_or(velocity.y - self.get_gravity(), |effect| {
                0.05 * f64::from(effect.amplifier() + 1)
            });

        self.set_velocity(DVec3::new(0.0, y * SQUID_AIR_DRAG, 0.0));
    }

    fn update_squid_rotation(&self) {
        let movement = self.velocity();
        let horizontal = movement.x.hypot(movement.z);

        if movement.length_squared() <= f64::EPSILON {
            return;
        }

        let target_yaw = (-movement.x.atan2(movement.z)).to_degrees() as f32;

        let (current_yaw, current_pitch) = self.rotation();
        let yaw = current_yaw + (target_yaw - current_yaw) * 0.1;

        self.set_rotation((yaw, current_pitch));
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let ageable_base = AgeableMobBase::new();
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);
        let mut entity_data = SquidEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let mut random = LegacyRandom::from_seed(rand::random());
        let tentical_random = random.next_f32();

        // Add goals here
        {
            let mut goal_selector = mob_base.goal_selector().lock();
            // Fleeing must outrank the always-available random movement goal.
            goal_selector.add_goal(0, SquidFleeGoal::new());
            goal_selector.add_goal(1, SquidRandomMovementGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
            state: SyncMutex::new(SquidState {
                movement_vector: DVec3::ZERO,
                random,
                tentacle_speed: 1.0 / (tentical_random + 1.0) * 0.2,
                tentacle_movement: 0.0,
            }),
        }
    }
}

impl Entity for SquidEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn hurt(
        &self,
        world: &World,
        source: &crate::entity::damage::DamageSource,
        amount: f32,
    ) -> bool {
        let hurt = LivingEntity::hurt_server(self, world, source, amount);

        if hurt && self.last_hurt_by_mob().is_some() {
            self.spawn_ink()
        }

        hurt
    }

    fn entity_type(&self) -> EntityTypeRef {
        &self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn get_default_gravity(&self) -> f64 {
        // TODO: Magic number
        0.08
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            SQUID_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }
}

impl LivingEntity for SquidEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn sound_volume(&self) -> f32 {
        // MAGIC NUMBER!
        0.4
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn travel(&self, input: DVec3) -> Option<MoveResult> {
        if self.is_in_water() {}
        self.move_entity(MoverType::SelfMovement, self.velocity())
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = Mob::mob_ai_step(self);
        self.tick_squid_movement();
        self.update_squid_rotation();

        AgeableMob::tick_ageable_mob(self);
        result
    }

    fn hurt_sound(
        &self,
        _source: &crate::entity::damage::DamageSource,
    ) -> Option<steel_registry::sound_event::SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_HURT)
    }

    fn death_sound(&self) -> Option<steel_registry::sound_event::SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_DEATH)
    }
}

impl AgeableMob for SquidEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl Mob for SquidEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn ambient_sound(&self) -> Option<steel_registry::sound_event::SoundEventRef> {
        Some(&sound_events::ENTITY_SQUID_AMBIENT)
    }

    fn finalize_spawn(
        &self,
        world: &std::sync::Arc<World>,
        spawn_reason: crate::entity::EntitySpawnReason,
        group_data: Option<crate::entity::SpawnGroupData>,
    ) -> Option<crate::entity::SpawnGroupData> {
        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }
}

impl PathfinderMob for SquidEntity {}
