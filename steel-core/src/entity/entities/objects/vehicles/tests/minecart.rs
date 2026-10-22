use std::sync::Arc;

use glam::DVec3;
use steel_registry::vanilla_entities;
use steel_utils::{ChunkPos, types::InteractionHand};

use super::{AbstractMinecart, MinecartEntity, VehicleEntity};
use crate::behavior::InteractionResult;
use crate::bootstrap::init_globals;
use crate::entity::{Entity, EntityEventSource, create_entity_instance, next_entity_id};
use crate::test_support::{
    TestPlayerBuilder, TestWorld, fresh_test_world, insert_ready_full_chunk,
};
use crate::world::World;

fn test_world(key: &'static str) -> TestWorld {
    init_globals();

    let fixture = fresh_test_world(key);
    insert_ready_full_chunk(&fixture.world, ChunkPos::new(0, 0));

    fixture
}

fn test_cart(world: &Arc<World>) -> Arc<MinecartEntity> {
    let cart = Arc::new(MinecartEntity::new(
        &vanilla_entities::MINECART,
        next_entity_id(),
        DVec3::new(8.5, 64.0, 8.5),
        Arc::downgrade(world),
    ));

    world
        .try_add_entity(Arc::clone(&cart).into_shared_entity())
        .expect("test minecart should enter the loaded chunk");

    cart
}

#[test]
fn registered_minecart_factory_supports_vanilla_boarding_results() {
    let fixture = test_world("minecart_factory_boarding");
    let world = &fixture.world;
    let cart = create_entity_instance(
        world,
        &vanilla_entities::MINECART,
        DVec3::new(8.5, 64.0, 8.5),
    )
    .expect("minecart factory should be registered");
    let minecart = cart
        .as_abstract_minecart()
        .expect("minecart factory should expose its capability");
    world
        .try_add_entity(Arc::clone(&cart))
        .expect("cart should enter the loaded chunk");
    let player = TestPlayerBuilder::new(Arc::clone(world), "CartRider", next_entity_id()).build();
    world
        .try_add_entity(Arc::clone(&player).into_shared_entity())
        .expect("player should enter the loaded chunk");
    player.base().set_boarding_cooldown(1);
    assert_eq!(
        cart.interact(&player, InteractionHand::MainHand, DVec3::ZERO),
        InteractionResult::Consume
    );
    assert!(!player.is_passenger());

    player.base().set_boarding_cooldown(0);
    assert_eq!(
        cart.interact(&player, InteractionHand::MainHand, DVec3::ZERO),
        InteractionResult::SuccessServer
    );
    assert!(cart.has_passenger(player.as_ref()));
    assert_eq!(
        cart.interact(&player, InteractionHand::MainHand, DVec3::ZERO),
        InteractionResult::Pass
    );

    minecart.activate_minecart(world, cart.block_position(), true);
    assert!(!player.is_passenger());
    assert!(!cart.is_vehicle());
}

#[test]
fn powered_activation_starts_the_hurt_animation_without_destroying_the_cart() {
    let fixture = test_world("minecart_powered_activation");
    let world = &fixture.world;
    let cart = test_cart(world);

    cart.set_hurt_time(0);
    cart.set_hurt_dir(1);
    cart.set_damage(7.0);
    cart.clear_hurt_mark();

    cart.activate_minecart(world, cart.block_position(), true);

    assert_eq!(
        (cart.hurt_time(), cart.hurt_dir(), cart.damage()),
        (10, -1, 50.0),
    );
    assert!(cart.hurt_marked());
    assert!(!cart.is_removed());
}

#[test]
fn powered_activation_preserves_an_existing_hurt_animation() {
    let fixture = test_world("minecart_existing_hurt_animation");
    let world = &fixture.world;
    let cart = test_cart(world);

    cart.set_hurt_time(3);
    cart.set_hurt_dir(1);
    cart.set_damage(7.0);
    cart.clear_hurt_mark();

    cart.activate_minecart(world, cart.block_position(), true);

    assert_eq!(
        (cart.hurt_time(), cart.hurt_dir(), cart.damage()),
        (3, 1, 7.0),
    );
    assert!(!cart.hurt_marked());
    assert!(!cart.is_removed());
}
