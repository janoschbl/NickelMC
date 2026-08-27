//! Blaze and dispenser fireballs, including impact damage and ignition.

use glam::DVec3;
use simdnbt::{borrow::NbtCompound as BorrowedNbtCompoundView, owned::NbtCompound};
use std::sync::{Arc, Weak};
use steel_macros::entity_behavior;
use steel_registry::{
    entity_type::EntityTypeRef, item_stack::ItemStack, vanilla_damage_types,
    vanilla_entity_data::SmallFireballEntityData, vanilla_game_rules, vanilla_items,
};
use steel_utils::{DowncastType, DowncastTypeKey, locks::SyncMutex, types::UpdateFlags};

use crate::{
    behavior::blocks::portal::FireBlock,
    entity::{
        Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
        ProjectileHit, RemovalReason, SharedEntity,
        damage::DamageSource,
        projectile::{AbstractHurtingProjectile, Fireball, HurtingProjectileBase},
    },
    world::{ClipHitResult, World},
};
use steel_registry::blocks::block_state_ext::BlockStateExt;

#[cfg(test)]
#[path = "small_fireball_tests.rs"]
mod tests;

/// A small accelerating fireball.
#[entity_behavior(class = "SmallFireball")]
pub struct SmallFireballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<SmallFireballEntityData>,
    projectile_base: ProjectileBase,
    hurting_base: HurtingProjectileBase,
}

// SAFETY: This owner-scoped key uniquely identifies SmallFireballEntity.
unsafe impl DowncastType for SmallFireballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/small_fireball");
}

impl SmallFireballEntity {
    /// Creates an unowned small fireball.
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Restores a saved small fireball.
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let entity = Self {
            base,
            entity_type,
            entity_data: SyncMutex::new(SmallFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
            hurting_base: HurtingProjectileBase::default(),
        };
        entity.set_item(ItemStack::new(&vanilla_items::FIRE_CHARGE));
        entity
    }
}

impl Entity for SmallFireballEntity {
    fn get_slot(&self, slot: i32) -> Option<Box<dyn crate::entity::SlotAccess + '_>> {
        self.fireball_slot(slot)
    }
    fn base(&self) -> &EntityBase {
        &self.base
    }
    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }
    fn tick(self: Arc<Self>) {
        self.hurting_projectile_tick();
    }
    fn attackable(&self) -> bool {
        false
    }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
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
    fn play_entity_on_fire_extinguished_sound(&self) {}
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_fireball(nbt);
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_fireball(nbt);
    }
}

impl AbstractHurtingProjectile for SmallFireballEntity {
    fn hurting_projectile_base(&self) -> &HurtingProjectileBase {
        &self.hurting_base
    }
}

impl Fireball for SmallFireballEntity {
    fn get_item(&self) -> ItemStack {
        self.entity_data.lock().fireball.item_stack.get().clone()
    }
    fn set_item(&self, item: ItemStack) {
        let item = if item.is_empty() {
            ItemStack::new(&vanilla_items::FIRE_CHARGE)
        } else {
            item.copy_with_count(1)
        };
        self.entity_data.lock().fireball.item_stack.set(item);
    }
}

impl Projectile for SmallFireballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }
    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }
    fn can_hit_entity(&self, entity: &dyn Entity) -> bool {
        self.base_can_hit_entity(entity) && !entity.no_physics()
    }
    fn on_deflection(&self, by_attack: bool) {
        self.hurting_on_deflection(by_attack);
    }
    fn on_hit_entity(self: Arc<Self>, entity: &SharedEntity, _location: DVec3) {
        let Some(world) = self.level() else { return };
        let remaining_fire = entity.remaining_fire_ticks();
        entity.ignite_for_ticks(100);
        let mut source = DamageSource::environment(&vanilla_damage_types::UNATTRIBUTED_FIREBALL)
            .with_direct_entity(Arc::clone(&self) as SharedEntity);
        if let Some(owner) = self.get_owner() {
            source = DamageSource::environment(&vanilla_damage_types::FIREBALL)
                .with_direct_entity(Arc::clone(&self) as SharedEntity)
                .with_causing_entity(owner);
        }
        if !entity.hurt(&world, &source, 5.0) {
            entity.set_remaining_fire_ticks(remaining_fire);
        } else {
            crate::enchantment_helper::do_post_attack_effects(&world, entity, &source);
        }
    }
    fn on_hit_block(self: Arc<Self>, hit: &ClipHitResult) {
        self.projectile_on_hit_block(hit);
        let Some(world) = self.level() else { return };
        if self
            .get_owner()
            .is_some_and(|owner| owner.as_mob().is_some())
            && !world.get_game_rule(&vanilla_game_rules::MOB_GRIEFING)
        {
            return;
        }
        let pos = hit.block_pos.relative(hit.direction);
        if world.get_block_state(pos).is_air() {
            world.set_block(
                pos,
                FireBlock::get_state(world.as_ref(), pos),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
    fn on_hit(self: Arc<Self>, hit: &ProjectileHit) {
        Arc::clone(&self).projectile_on_hit(hit);
        self.set_removed(RemovalReason::Discarded);
    }
}
