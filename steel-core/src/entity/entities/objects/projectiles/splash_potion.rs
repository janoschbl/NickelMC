//! Splash potion projectile with distance scaled effects and water interactions

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::{PotionContents, vanilla_components};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_entity_data::SplashPotionEntityData;
use steel_registry::{MobEffectInstance as RegistryMobEffectInstance, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::behavior::MOB_EFFECT_BEHAVIORS;
use crate::entity::damage::DamageSource;
use crate::entity::potion_contents::to_runtime_instance_icon_from_visibility;
use crate::entity::{
    AbstractThrownPotion, Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity,
    Projectile, ProjectileBase, ProjectileHit, SPLASH_RANGE_SQ, SharedEntity,
    ThrowableItemProjectile, ThrowableProjectile, compute_margin,
};
use crate::world::{ClipHitResult, World};

/// Scaled effects lasting 20 ticks or less are skipped
const MIN_REMAINING_TICKS: i32 = 20;

/// A thrown splash potion.
#[entity_behavior(class = "ThrownSplashPotion")]
pub struct SplashPotionEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced data carrying the rendered item stack.
    entity_data: SyncMutex<SplashPotionEntityData>,
    /// Shared `Projectile` state (owner / left-owner / has-been-shot).
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SplashPotionEntity`.
unsafe impl DowncastType for SplashPotionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/splash_potion");
}

impl SplashPotionEntity {
    /// Creates a new thrown splash potion with no owner and the default rendered item.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown splash potion from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Applies one effect with distance falloff and duration scaling
    fn apply_effect(
        self: &Arc<Self>,
        world: &World,
        living: &dyn LivingEntity,
        effect: &RegistryMobEffectInstance,
        scale: f64,
        duration_scale: f32,
        owner: Option<&SharedEntity>,
    ) {
        let behavior = MOB_EFFECT_BEHAVIORS.get_behavior(effect.effect());
        if let Some(instantaneous) = behavior.as_instantaneous() {
            let source: SharedEntity = Arc::<Self>::clone(self);
            instantaneous.apply_instantaneous(
                world,
                living,
                effect.amplifier(),
                Some(&source),
                owner,
                scale,
            );
            return;
        }

        let duration = effect.map_duration(|base_duration| {
            (scale * f64::from(base_duration) * f64::from(duration_scale) + 0.5) as i32
        });
        let scaled_effect = to_runtime_instance_icon_from_visibility(effect, duration);
        if scaled_effect.ends_within(MIN_REMAINING_TICKS) {
            return;
        }
        living.add_mob_effect_with_source(
            scaled_effect,
            Some(owner.map_or(self.as_ref() as &dyn Entity, AsRef::as_ref)),
        );
    }
}

impl Entity for SplashPotionEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(self: Arc<Self>) {
        self.throwable_projectile_tick();
    }

    fn get_default_gravity(&self) -> f64 {
        self.thrown_potion_default_gravity()
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn spawn_data(&self) -> i32 {
        self.get_owner().map_or(0, |owner| owner.id())
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn projectile_owner_uuid(&self) -> Option<uuid::Uuid> {
        self.owner_uuid()
    }

    fn projectile_owner(&self) -> Option<SharedEntity> {
        self.get_owner()
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        self.save_throwable_item(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_throwable_item(nbt);
    }
}

impl Projectile for SplashPotionEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn calculate_horizontal_hurt_knockback_direction(
        &self,
        hurt_entity: &dyn LivingEntity,
        _damage_source: &DamageSource,
    ) -> (f64, f64) {
        self.thrown_potion_knockback_direction(hurt_entity)
    }

    fn on_hit(self: Arc<Self>, hit: &ProjectileHit) {
        self.thrown_potion_on_hit(hit);
    }

    fn on_hit_block(self: Arc<Self>, hit: &ClipHitResult) {
        self.thrown_potion_on_hit_block(hit);
    }
}

impl ThrowableProjectile for SplashPotionEntity {}

impl ThrowableItemProjectile for SplashPotionEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::SPLASH_POTION
    }

    fn set_item(&self, item: ItemStack) {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .set(item);
    }

    fn get_item(&self) -> ItemStack {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .get()
            .clone()
    }
}

impl AbstractThrownPotion for SplashPotionEntity {
    fn on_hit_as_potion(
        self: &Arc<Self>,
        world: &Arc<World>,
        potion_item: &ItemStack,
        hit: &ProjectileHit,
    ) {
        let contents = potion_item
            .get_or_default(vanilla_components::POTION_CONTENTS, PotionContents::empty());
        let duration_scale =
            potion_item.get_or_default(vanilla_components::POTION_DURATION_SCALE, 1.0);
        let mob_effects = contents.all_effects();

        let potion_aabb = self
            .bounding_box()
            .translate(hit.location() - self.position());
        let effect_aabb = potion_aabb.inflate_xyz(4.0, 2.0, 4.0);
        let entities =
            world.get_entities_in_aabb_matching(&effect_aabb, |entity| !entity.is_spectator());
        if entities.is_empty() {
            return;
        }

        let margin = compute_margin(self.tick_count());
        let owner = self.get_owner();

        for entity in entities {
            let Some(living) = entity.as_living_entity() else {
                continue;
            };
            if !living.is_affected_by_potions() {
                continue;
            }

            let dist = potion_aabb.distance_to_sqr_aabb(entity.bounding_box().inflate(margin));
            if dist >= SPLASH_RANGE_SQ {
                continue;
            }
            let scale = 1.0 - dist.sqrt() / 4.0;

            for effect in &mob_effects {
                self.apply_effect(world, living, effect, scale, duration_scale, owner.as_ref());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::data_components::{PotionContents, vanilla_components};
    use steel_registry::item_stack::ItemStack;
    use steel_registry::{
        MobEffectInstance, init_vanilla_registry, vanilla_entities, vanilla_items,
        vanilla_mob_effects, vanilla_potions,
    };
    use steel_utils::types::GameType;
    use steel_utils::{BlockPos, ChunkPos, Direction};

    use crate::behavior::init_behaviors;
    use crate::entity::{
        AbstractThrownPotion, Entity, LivingEntity, Projectile, ProjectileHit, SharedEntity,
        ThrowableItemProjectile,
    };
    use crate::test_support::{
        TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk, test_world,
    };
    use crate::world::ClipHitResult;

    use super::SplashPotionEntity;

    fn block_hit(position: DVec3) -> ProjectileHit {
        ProjectileHit::Block {
            location: position,
            hit: ClipHitResult {
                location: position,
                direction: Direction::Up,
                block_pos: BlockPos::containing(position.x, position.y, position.z),
                miss: false,
                inside: false,
                world_border_hit: false,
            },
        }
    }

    #[test]
    fn splash_range_uses_hit_box_distance_and_excludes_spectators() {
        init_vanilla_registry();
        init_behaviors();
        let fixture = fresh_test_world("splash_range");
        let world = &fixture.world;
        insert_ready_full_chunk(world, ChunkPos::new(0, 0));
        let origin = DVec3::new(8.0, 65.0, 8.0);
        let potion = Arc::new(SplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            1,
            origin,
            Arc::downgrade(world),
        ));
        let mut item = ItemStack::new(&vanilla_items::SPLASH_POTION);
        item.set(
            vanilla_components::POTION_CONTENTS,
            PotionContents::new(
                None,
                None,
                vec![MobEffectInstance::simple(vanilla_mob_effects::LUCK, 200, 0)],
                None,
            ),
        );

        for (id, offset, spectator, expected) in [
            (2, 0.0, false, Some(200)),
            (3, 2.0, false, Some(121)),
            (4, 4.5, false, None),
            (5, 0.0, true, None),
        ] {
            let player = TestPlayerBuilder::new(Arc::clone(world), "SplashTest", id).build();
            player
                .try_set_position(origin + DVec3::X * offset)
                .expect("test position");
            if spectator {
                player.restore_game_modes(GameType::Spectator, None);
            }
            world
                .try_add_entity(Arc::clone(&player) as SharedEntity)
                .expect("attach player");
            potion.on_hit_as_potion(world, &item, &block_hit(origin));
            assert_eq!(
                player
                    .mob_effect(vanilla_mob_effects::LUCK)
                    .map(|effect| effect.duration()),
                expected,
                "offset={offset}, spectator={spectator}"
            );
        }
    }

    #[test]
    fn splash_duration_cutoff_preserves_infinite_and_rounds_after_both_scales() {
        init_vanilla_registry();
        init_behaviors();
        let fixture = fresh_test_world("splash_duration");
        let world = &fixture.world;
        let potion = Arc::new(SplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            1,
            DVec3::ZERO,
            Arc::downgrade(world),
        ));
        let player = TestPlayerBuilder::new(Arc::clone(world), "SplashTest", 2).build();
        for (duration, scale, duration_scale, expected) in [
            (40, 0.5, 1.0, None),
            (41, 0.5, 1.0, Some(21)),
            (101, 0.5, 0.5, Some(25)),
            (-1, 0.001, 0.001, Some(-1)),
            (0, 1.0, 1.0, None),
        ] {
            player.remove_mob_effect(vanilla_mob_effects::LUCK);
            potion.apply_effect(
                world,
                player.as_ref(),
                &MobEffectInstance::simple(vanilla_mob_effects::LUCK, duration, 0),
                scale,
                duration_scale,
                None,
            );
            assert_eq!(
                player
                    .mob_effect(vanilla_mob_effects::LUCK)
                    .map(|effect| effect.duration()),
                expected,
                "duration={duration}"
            );
        }
    }

    #[test]
    fn splash_instant_healing_rounds_distance_scale_and_ignores_duration_scale() {
        init_vanilla_registry();
        init_behaviors();
        let fixture = fresh_test_world("splash_instant");
        let world = &fixture.world;
        let potion = Arc::new(SplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            1,
            DVec3::ZERO,
            Arc::downgrade(world),
        ));
        let player = TestPlayerBuilder::new(Arc::clone(world), "SplashTest", 2).build();
        for (scale, expected) in [(1.0, 14.0), (0.5, 12.0), (0.124, 10.0), (0.125, 11.0)] {
            player.set_health(10.0);
            potion.apply_effect(
                world,
                player.as_ref(),
                &MobEffectInstance::simple(vanilla_mob_effects::INSTANT_HEALTH, 1, 0),
                scale,
                0.0,
                None,
            );
            assert_eq!(player.get_health(), expected);
            assert!(
                player
                    .mob_effect(vanilla_mob_effects::INSTANT_HEALTH)
                    .is_none()
            );
        }
    }

    #[test]
    fn water_with_custom_effects_does_not_extinguish_entities() {
        init_vanilla_registry();
        init_behaviors();
        let fixture = fresh_test_world("splash_water");
        let world = &fixture.world;
        insert_ready_full_chunk(world, ChunkPos::new(0, 0));
        let origin = DVec3::new(8.0, 65.0, 8.0);
        let player = TestPlayerBuilder::new(Arc::clone(world), "SplashTest", 2).build();
        player.try_set_position(origin).expect("test position");
        world
            .try_add_entity(Arc::clone(&player) as SharedEntity)
            .expect("attach player");
        for custom in [true, false] {
            player.ignite_for_ticks(100);
            let potion = Arc::new(SplashPotionEntity::new(
                &vanilla_entities::SPLASH_POTION,
                1,
                origin,
                Arc::downgrade(world),
            ));
            let mut contents = PotionContents::of(&vanilla_potions::WATER);
            if custom {
                contents = PotionContents::new(
                    contents.potion(),
                    None,
                    vec![MobEffectInstance::simple(vanilla_mob_effects::LUCK, 200, 0)],
                    None,
                );
            }
            let mut item = ItemStack::new(&vanilla_items::SPLASH_POTION);
            item.set(vanilla_components::POTION_CONTENTS, contents);
            potion.set_item(item);
            potion.on_hit(&block_hit(origin));
            assert_eq!(player.is_on_fire(), custom);
        }
    }

    #[test]
    fn on_hit_discards_the_splash_potion() {
        init_vanilla_registry();
        init_behaviors();

        let world = test_world();
        let potion = Arc::new(SplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            1,
            DVec3::ZERO,
            Arc::downgrade(world),
        ));
        let hit = ProjectileHit::Block {
            location: DVec3::ZERO,
            hit: ClipHitResult {
                location: DVec3::ZERO,
                direction: Direction::Up,
                block_pos: BlockPos::new(0, 0, 0),
                miss: false,
                inside: false,
                world_border_hit: false,
            },
        };

        Arc::clone(&potion).on_hit(&hit);
        assert!(potion.is_removed());
    }
}
