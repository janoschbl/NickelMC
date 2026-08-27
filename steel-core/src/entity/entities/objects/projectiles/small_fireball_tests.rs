use std::{
    io::Cursor,
    sync::{Arc, Weak},
};

use glam::DVec3;
use simdnbt::{borrow::read_compound, owned::NbtCompound};
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::{
    init_vanilla_registry, item_stack::ItemStack, vanilla_blocks, vanilla_entities,
    vanilla_game_rules, vanilla_items,
};
use steel_utils::{BlockPos, ChunkPos, Direction, types::UpdateFlags};

use super::SmallFireballEntity;
use crate::{
    behavior::init_behaviors,
    entity::{
        Entity, Projectile, SharedEntity,
        entities::{BlazeEntity, PigEntity},
        projectile::{AbstractHurtingProjectile, Fireball},
    },
    test_support::{fresh_test_world, insert_ready_full_chunk},
    world::ClipHitResult,
};

#[test]
fn small_fireball_accelerates_before_moving_and_wraps_rotation() {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("hurting_projectile_inertia");
    let world = &fixture.world;
    insert_ready_full_chunk(world, ChunkPos::new(0, 0));
    let position = DVec3::new(8.0, 80.0, 8.0);
    let fireball = Arc::new(SmallFireballEntity::new(
        &vanilla_entities::SMALL_FIREBALL,
        1,
        position,
        Arc::downgrade(world),
    ));
    fireball.assign_directional_movement(DVec3::X);
    fireball.set_rotation((-179.0, 0.0));
    world
        .try_add_entity(Arc::clone(&fireball) as SharedEntity)
        .expect("attach fireball");
    Arc::clone(&fireball).tick();
    let expected = 0.2 * f64::from(0.95_f32);
    assert!((fireball.velocity().x - expected).abs() < 1.0e-12);
    assert!((fireball.position().x - position.x - expected).abs() < 1.0e-12);
    assert_eq!(fireball.position().y, position.y);
    assert_eq!(fireball.rotation(), (-197.2, 0.0));
    assert!(fireball.has_been_shot());
}

#[test]
fn small_fireball_round_trips_deflected_acceleration_owner_and_rendered_item() {
    init_vanilla_registry();
    let fireball = SmallFireballEntity::new(
        &vanilla_entities::SMALL_FIREBALL,
        1,
        DVec3::ZERO,
        Weak::new(),
    );
    let owner = uuid::Uuid::from_u128(42);
    fireball.set_owner_uuid(Some(owner));
    fireball.hurting_on_deflection(false);
    fireball.hurting_on_deflection(false);
    let mut item = ItemStack::new(&vanilla_items::DIAMOND);
    item.set_count(12);
    fireball.set_item(item);
    let mut saved = NbtCompound::new();
    fireball.save_additional(&mut saved);
    let mut bytes = Vec::new();
    saved.write(&mut bytes);
    let borrowed = read_compound(&mut Cursor::new(&bytes)).expect("read projectile NBT");
    let loaded = SmallFireballEntity::new(
        &vanilla_entities::SMALL_FIREBALL,
        2,
        DVec3::ZERO,
        Weak::new(),
    );
    loaded.load_additional((&borrowed).into());
    assert_eq!(loaded.acceleration_power(), 0.025);
    assert_eq!(loaded.owner_uuid(), Some(owner));
    assert_eq!(loaded.get_item().item(), &vanilla_items::DIAMOND);
    assert_eq!(loaded.get_item().count(), 1);
    loaded.hurting_on_deflection(true);
    assert_eq!(loaded.acceleration_power(), 0.1);
    loaded.set_item(ItemStack::empty());
    assert_eq!(loaded.get_item().item(), &vanilla_items::FIRE_CHARGE);
    let slot = loaded.get_slot(0).expect("fireball item slot");
    assert!(slot.set(ItemStack::new(&vanilla_items::DIAMOND)));
    assert_eq!(slot.get().item(), &vanilla_items::DIAMOND);
    assert!(loaded.get_slot(1).is_none());
}

#[test]
fn small_fireball_restores_fire_on_rejected_damage_but_burns_victims() {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("small_fireball_damage");
    let world = &fixture.world;
    insert_ready_full_chunk(world, ChunkPos::new(0, 0));
    let position = DVec3::new(8.0, 80.0, 8.0);
    let fireball = Arc::new(SmallFireballEntity::new(
        &vanilla_entities::SMALL_FIREBALL,
        1,
        position,
        Arc::downgrade(world),
    ));
    let immune: SharedEntity = Arc::new(BlazeEntity::new(
        &vanilla_entities::BLAZE,
        2,
        position,
        Arc::downgrade(world),
    ));
    immune.set_remaining_fire_ticks(7);
    Arc::clone(&fireball).on_hit_entity(&immune, position);
    assert_eq!(immune.remaining_fire_ticks(), 7);
    let victim: SharedEntity = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        3,
        position,
        Arc::downgrade(world),
    ));
    let before = victim
        .as_living_entity()
        .expect("pig is living")
        .get_health();
    Arc::clone(&fireball).on_hit_entity(&victim, position);
    assert_eq!(victim.remaining_fire_ticks(), 100);
    assert_eq!(
        victim
            .as_living_entity()
            .expect("pig is living")
            .get_health(),
        before - 5.0
    );
}

#[test]
fn small_fireball_selects_soul_fire_and_respects_mob_griefing() {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("small_fireball_fire_placement");
    let world = &fixture.world;
    insert_ready_full_chunk(world, ChunkPos::new(0, 0));
    let base = BlockPos::new(8, 79, 8);
    assert!(world.set_block(
        base,
        vanilla_blocks::SOUL_SOIL.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let fireball = Arc::new(SmallFireballEntity::new(
        &vanilla_entities::SMALL_FIREBALL,
        1,
        DVec3::new(8.5, 80.0, 8.5),
        Arc::downgrade(world),
    ));
    let owner: SharedEntity = Arc::new(BlazeEntity::new(
        &vanilla_entities::BLAZE,
        2,
        fireball.position(),
        Arc::downgrade(world),
    ));
    fireball.set_owner_entity(Some(&owner));
    let hit = ClipHitResult {
        location: fireball.position(),
        direction: Direction::Up,
        block_pos: base,
        miss: false,
        inside: false,
        world_border_hit: false,
    };
    world.set_game_rule(&vanilla_game_rules::MOB_GRIEFING, false);
    Arc::clone(&fireball).on_hit_block(&hit);
    assert!(world.get_block_state(base.above()).is_air());
    world.set_game_rule(&vanilla_game_rules::MOB_GRIEFING, true);
    Arc::clone(&fireball).on_hit_block(&hit);
    assert_eq!(
        world.get_block_state(base.above()).get_block(),
        &vanilla_blocks::SOUL_FIRE
    );
}
