use steel_registry::{init_vanilla_registry, vanilla_entities};

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::{EntityFluidContact, next_entity_id};
use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};
use steel_registry::vanilla_blocks;
use steel_utils::{
    ChunkPos,
    types::{GameType, UpdateFlags},
};

fn test_squid() -> SquidEntity {
    init_vanilla_registry();
    SquidEntity::new(&vanilla_entities::SQUID, 1, DVec3::ZERO, Weak::new())
}

/// A nose-down squid inks behind itself, so Z comes out positive. Reaching for
/// `DVec3::rotate_x` mirrors it and the ink squirts forwards instead.
#[test]
fn rotate_vector_trails_ink_behind_a_pitched_squid() {
    let squid = test_squid();
    squid.state.lock().x_body_rot_old = 30.0;

    let rotated = squid.rotate_vector(DVec3::new(0.0, -1.0, 0.0));

    assert!(
        (rotated - DVec3::new(0.0, -0.866_025, 0.5)).length() < 1e-4,
        "ink must trail behind the squid, got {rotated:?}"
    );
}

#[test]
fn land_movement_applies_float_air_drag_attribute_before_widening() {
    let squid = test_squid();
    squid.set_velocity(DVec3::new(0.3, 0.7, -0.2));
    squid
        .attributes()
        .lock()
        .set_base_value(vanilla_attributes::AIR_DRAG_MODIFIER, 2.5);

    squid.tick_squid_movement();

    let drag = 1.0_f32 - (1.0_f32 - 0.98_f32) * 2.5_f32;
    assert_eq!(
        squid.velocity(),
        DVec3::new(0.0, (0.7 - 0.08) * f64::from(drag), 0.0)
    );
}

#[test]
fn water_rotation_uses_mth_approximation_and_old_body_angles_for_ink() {
    let squid = test_squid();
    squid
        .base()
        .set_fluid_contact(EntityFluidContact::from_parts(0.5, 0.0, true, false));
    squid.set_y_body_rot(10.0);
    squid.state.lock().x_body_rot = 20.0;
    squid.advance_body_rotation();
    squid.set_velocity(DVec3::new(0.123, 0.456, 0.456));

    squid.update_squid_rotation();

    let expected_yaw = 10.0_f32 + (-f32::from_bits(0x4171_8764) - 10.0) * 0.1;
    assert_eq!(squid.rotation().0.to_bits(), expected_yaw.to_bits());
    assert_eq!(squid.state.lock().x_body_rot_old, 20.0);
}

#[test]
fn squid_spawn_obstruction_allows_water_and_excludes_spectators() {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("squid_water_spawn_obstruction");
    let world = &fixture.world;
    insert_ready_full_chunk(world, ChunkPos::new(0, 0));
    let position = DVec3::new(8.0, 65.0, 8.0);
    let squid = SquidEntity::new(
        &vanilla_entities::SQUID,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    );
    assert!(world.set_block(
        squid.block_position(),
        vanilla_blocks::WATER.default_state(),
        UpdateFlags::UPDATE_ALL
    ));
    assert!(squid.check_spawn_obstruction(world));

    let player =
        TestPlayerBuilder::new(Arc::clone(world), "SpawnBlocker", next_entity_id()).build();
    player.base().set_position_local(position);
    let entity: SharedEntity = Arc::clone(&player) as SharedEntity;
    world
        .try_add_entity(entity)
        .expect("player should join world");
    assert!(!squid.check_spawn_obstruction(world));
    player.restore_game_modes(GameType::Spectator, None);
    assert!(squid.check_spawn_obstruction(world));
}
