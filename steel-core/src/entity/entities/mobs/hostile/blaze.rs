use crate::entity::projectile::triangle_random;
use crate::{
    entity::{
        Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
        MobBase, PathfinderMob, SharedEntity,
        ai::{
            goal::{
                BlazeAttackGoal, HurtByTargetGoal, LookAtPlayerGoal, MoveTowardsRestrictionGoal,
                NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
            },
            path::PathType,
        },
        damage::DamageSource,
        monster::Monster,
        sync_dirty_mob_effects,
    },
    physics::MoveResult,
    world::World,
};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use std::sync::Weak;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::{
    entity_type::EntityTypeRef, sound_event::SoundEventRef, sound_events,
    vanilla_entity_data::BlazeEntityData,
};
use steel_utils::{DowncastType, DowncastTypeKey, locks::SyncMutex};

#[entity_behavior(class = "Blaze")]
/// Vanilla blaze entity.
pub struct BlazeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<BlazeEntityData>,
    height_data: SyncMutex<BlazeHeightData>,
}

pub struct BlazeHeightData {
    allowed_offset: f32,
    next_offset_change_tick: i32,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BlazeEntity`.
unsafe impl DowncastType for BlazeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/blaze");
}

impl BlazeEntity {
    /// Creates a blaze at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a blaze from persisted base entity state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let mut entity_data = BlazeEntityData::new();
        let height_data = BlazeHeightData {
            allowed_offset: 0.5,
            next_offset_change_tick: 0,
        };

        {
            let mut malus = mob_base.pathfinding_malus().lock();
            malus.set(PathType::Water, -1.0);
            malus.set(PathType::Lava, 8.0);
            malus.set(PathType::FireInNeighbor, 0.0);
            malus.set(PathType::Fire, 0.0);
        }

        mob_base.set_xp_reward(10);

        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(4, BlazeAttackGoal::new());
            goal_selector.add_goal(5, MoveTowardsRestrictionGoal::new(1.0));
            goal_selector.add_goal(7, WaterAvoidingRandomStrollGoal::with_probability(1.0, 0.0));
            goal_selector.add_goal(8, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());
            let mut target_selector = mob_base.target_selector().lock();
            target_selector.add_goal(1, HurtByTargetGoal::new().set_alert_others([]));
            target_selector.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_, _| true),
            );
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            height_data: SyncMutex::new(height_data),
        }
    }

    /// Returns the charged data flag
    pub fn is_charged(&self) -> bool {
        self.entity_data.lock().flags.get() & 1 != 0
    }

    /// Sets the charged data flag
    pub fn set_charged(&self, value: bool) {
        let mut data = self.entity_data.lock();
        let current_value = *data.flags.get();
        if value {
            data.flags.set(current_value | 1);
        } else {
            data.flags.set(current_value & !1);
        }
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        sync_dirty_mob_effects(&self.living_base, &self.entity_data);
    }
}

impl Monster for BlazeEntity {
    fn update_no_action_time(&self) {
        self.set_no_action_time(self.no_action_time() + 2);
    }
}

impl Entity for BlazeEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn dimensions_for_pose(
        &self,
        _pose: crate::entity::EntityPose,
    ) -> steel_registry::entity_type::EntityDimensions {
        if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type
                .dimensions
                .scale(LivingEntity::get_scale(self))
        }
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(steel_registry::vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(0.6) as f32
    }

    fn fall_sounds(&self) -> (SoundEventRef, SoundEventRef) {
        Monster::fall_sounds_monster(self)
    }

    fn swim_sound(&self) -> SoundEventRef {
        Monster::swim_sound_monster(self)
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn sound_source(&self) -> SoundSource {
        Monster::sound_source_monster(self)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }

    fn is_on_fire(&self) -> bool {
        self.is_charged()
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }
}

impl LivingEntity for BlazeEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
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

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_DEATH)
    }

    fn should_drop_experience(&self) -> bool {
        Monster::should_drop_experience_monster(self)
    }

    fn should_drop_loot(&self, world: &World) -> bool {
        Monster::should_drop_loot_monster(self, world)
    }

    fn server_ai_step(&self, entity: &SharedEntity) {
        Mob::mob_server_ai_step(self, entity);
    }

    fn ai_step(&self, entity: &SharedEntity) -> Option<MoveResult> {
        if !self.on_ground() && self.velocity().y < 0.0 {
            self.set_velocity(self.velocity() * DVec3::new(1.0, 0.6, 1.0));
        }

        Monster::monster_ai_step(self, entity)
    }
}

impl Mob for BlazeEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self, entity: &SharedEntity) {
        PathfinderMob::tick_pathfinder_goal_selectors(self, entity);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_BLAZE_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }

    fn custom_server_ai_step(&self) {
        let mut height_data = self.height_data.lock();
        height_data.next_offset_change_tick -= 1;
        if height_data.next_offset_change_tick <= 0 {
            height_data.next_offset_change_tick = 100;
            height_data.allowed_offset = triangle_random(0.5, 6.891) as f32;
        }

        let target = self.target();

        if let Some(target) = target.as_deref().and_then(Entity::as_living_entity)
            && target.get_eye_y() > self.get_eye_y() + f64::from(height_data.allowed_offset)
            && Mob::can_attack(self, target)
        {
            let vel = self.velocity();
            self.set_velocity(
                vel + DVec3::new(0.0, (f64::from(0.3_f32) - vel.y) * f64::from(0.3_f32), 0.0),
            );
            self.mark_velocity_sync();
        }
    }
}

impl PathfinderMob for BlazeEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{entities::PigEntity, next_entity_id};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
    use std::sync::Arc;
    use steel_registry::{init_vanilla_registry, vanilla_entities};
    use steel_utils::ChunkPos;

    #[test]
    fn blaze_height_pursuit_accelerates_instead_of_setting_vertical_speed() {
        init_vanilla_registry();
        let fixture = fresh_test_world("blaze_height_pursuit");
        let world = &fixture.world;
        insert_ready_full_chunk(world, ChunkPos::new(0, 0));
        let blaze = BlazeEntity::new(
            &vanilla_entities::BLAZE,
            next_entity_id(),
            DVec3::new(8.0, 70.0, 8.0),
            Arc::downgrade(world),
        );
        let target: SharedEntity = Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            next_entity_id(),
            DVec3::new(8.0, 80.0, 8.0),
            Arc::downgrade(world),
        ));
        blaze.set_target(Some(&target));
        blaze.height_data.lock().next_offset_change_tick = 2;
        blaze.set_velocity(DVec3::new(0.2, -0.1, 0.3));
        blaze.custom_server_ai_step();
        let factor = f64::from(0.3_f32);
        assert_eq!(
            blaze.velocity(),
            DVec3::new(0.2, -0.1 + (factor + 0.1) * factor, 0.3)
        );
        assert!(blaze.needs_velocity_sync());
    }
}
