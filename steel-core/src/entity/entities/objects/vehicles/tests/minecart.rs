use std::sync::Arc;

use glam::DVec3;
use steel_registry::vanilla_entities;
use steel_utils::{ChunkPos, types::InteractionHand};

use super::{AbstractMinecart, MinecartEntity, VehicleEntity};
use crate::behavior::InteractionResult;
use crate::bootstrap::init_globals;
use crate::entity::{Entity, EntityEventSource, create_entity_instance, next_entity_id};
use crate::inventory::container::Container;
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
fn dismount_search_prefers_side_then_crouches_under_a_low_ceiling() {
    use steel_registry::{entity_data::EntityPose, vanilla_blocks};
    use steel_utils::{BlockPos, types::UpdateFlags};
    for low_ceiling in [false, true] {
        let fixture = test_world("minecart_dismount_side");
        let world = &fixture.world;
        let flags = UpdateFlags::UPDATE_NONE | UpdateFlags::UPDATE_SKIP_ON_PLACE;
        for x in 6..=10 {
            for z in 6..=10 {
                assert!(world.set_block(
                    BlockPos::new(x, 63, z),
                    vanilla_blocks::STONE.default_state(),
                    flags
                ));
                if low_ceiling {
                    assert!(world.set_block(
                        BlockPos::new(x, 65, z),
                        vanilla_blocks::STONE.default_state(),
                        flags
                    ));
                    // A bottom slab floor gives 1.5 blocks of headroom.
                    assert!(world.set_block(
                        BlockPos::new(x, 63, z),
                        vanilla_blocks::STONE_SLAB.default_state(),
                        flags
                    ));
                }
            }
        }
        let cart = test_cart(world);
        let player =
            TestPlayerBuilder::new(Arc::clone(world), "Dismounter", next_entity_id()).build();
        world
            .try_add_entity(Arc::clone(&player).into_shared_entity())
            .expect("player insertion");
        assert!(player.start_riding(&Arc::clone(&cart).into_shared_entity()));
        player.stop_riding();
        let (dx, dz) =
            crate::entity::dismount_helper::offsets_for_direction(cart.motion_direction())[0];
        let expected = DVec3::new(
            8.5 + f64::from(dx),
            if low_ceiling { 63.5 } else { 64.0 },
            8.5 + f64::from(dz),
        );
        assert_eq!(player.position(), expected);
        assert_eq!(
            player.pose(),
            if low_ceiling {
                EntityPose::Sneaking
            } else {
                EntityPose::Standing
            }
        );
    }
}

#[test]
fn dismount_without_a_floor_uses_cart_top() {
    let fixture = test_world("minecart_dismount_top");
    let cart = test_cart(&fixture.world);
    let player =
        TestPlayerBuilder::new(Arc::clone(&fixture.world), "Dismounter", next_entity_id()).build();
    assert_eq!(
        cart.dismount_location_for_passenger(player.as_ref()),
        DVec3::new(8.5, cart.bounding_box().max_y(), 8.5)
    );
}

#[test]
fn moving_non_minecart_entities_refreshes_water_contact_but_cart_keeps_tick_state() {
    use crate::physics::MoverType;
    use steel_registry::vanilla_blocks;
    use steel_utils::{BlockPos, types::UpdateFlags};
    let fixture = test_world("minecart_move_water_contact");
    let world = &fixture.world;
    assert!(world.set_block(
        BlockPos::new(9, 64, 8),
        vanilla_blocks::WATER.default_state(),
        UpdateFlags::UPDATE_NONE | UpdateFlags::UPDATE_SKIP_ON_PLACE
    ));
    for entity_type in [
        &vanilla_entities::PIG,
        &vanilla_entities::ITEM,
        &vanilla_entities::MINECART,
    ] {
        for no_physics in [false, true] {
            let entity = create_entity_instance(world, entity_type, DVec3::new(8.5, 64.0, 8.5))
                .expect("entity creation");
            world
                .try_add_entity(Arc::clone(&entity))
                .expect("entity insertion");
            entity.set_no_physics(no_physics);
            assert!(!entity.is_in_water());
            assert!(
                Arc::clone(&entity)
                    .move_entity(MoverType::SelfMovement, DVec3::X)
                    .is_some()
            );
            assert_eq!(
                entity.is_in_water(),
                entity_type != &vanilla_entities::MINECART
            );
        }
    }
}

#[test]
fn picking_a_cart_selects_existing_items_or_creates_only_in_creative_and_in_reach() {
    use steel_protocol::packets::game::SPickItemFromEntity;
    use steel_registry::{item_stack::ItemStack, vanilla_items};
    let fixture = test_world("minecart_pick");
    let world = &fixture.world;
    let cart = test_cart(world);
    let player = TestPlayerBuilder::new(Arc::clone(world), "Picker", next_entity_id()).build();
    player.base().set_position_local(cart.position());
    let packet = || SPickItemFromEntity {
        id: cart.id(),
        include_data: false,
    };
    player.handle_pick_item_from_entity(packet());
    assert!(player.inventory.lock().get_selected_item().is_empty());
    player.restore_game_modes(steel_utils::types::GameType::Creative, None);
    player.handle_pick_item_from_entity(packet());
    assert!(
        player
            .inventory
            .lock()
            .get_selected_item()
            .is(&vanilla_items::MINECART)
    );
    player.restore_game_modes(steel_utils::types::GameType::Survival, None);
    {
        let mut inventory = player.inventory.lock();
        inventory.set_selected_slot(1);
        inventory.set_item(2, ItemStack::new(&vanilla_items::MINECART));
    }
    player
        .base()
        .set_position_local(DVec3::new(100.0, 64.0, 100.0));
    player.handle_pick_item_from_entity(packet());
    assert_eq!(player.inventory.lock().get_selected_slot(), 1);
    player.base().set_position_local(cart.position());
    player.handle_pick_item_from_entity(packet());
    assert_eq!(player.inventory.lock().get_selected_slot(), 0);

    {
        let mut inventory = player.inventory.lock();
        for slot in 0..9 {
            inventory.set_item(
                slot,
                ItemStack::with_count(&vanilla_items::STONE, slot as i32 + 1),
            );
        }
        inventory.set_selected_slot(4);
    }
    player.restore_game_modes(steel_utils::types::GameType::Creative, None);
    player.handle_pick_item_from_entity(packet());
    {
        let inventory = player.inventory.lock();
        assert_eq!(inventory.get_selected_slot(), 4);
        assert!(inventory.get_selected_item().is(&vanilla_items::MINECART));
        assert!(inventory.get_item(9).is(&vanilla_items::STONE));
        assert_eq!(
            inventory.get_item(9).count(),
            5,
            "displaced hotbar stack must survive"
        );
    }
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
