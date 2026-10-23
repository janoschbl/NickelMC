use std::convert::Infallible;

use crate::loot::{LootContext, vanilla_loot_tables};
use rand::{Rng as _, TryRng};
use steel_registry::{vanilla_attributes, vanilla_damage_types};
use steel_utils::random::RandomSource;
use steel_utils::random::legacy_random::LegacyRandom;

use super::*;
use crate::entity::LivingEntity as _;
use crate::entity::entities::EndCrystalEntity;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::scoreboard::ScoreHolder;

#[test]
fn team_protection_skips_blast_damage_but_not_knockback() {
    for allow_friendly_fire in [false, true] {
        let fixture = ready_world("explosion_source_team");
        let world = &fixture.world;
        let source = add_entity(
            world,
            PrimedTntEntity::new(
                &vanilla_entities::TNT,
                next_entity_id(),
                TEST_BLOCK_BOTTOM_CENTER,
                Arc::downgrade(world),
            ),
        );
        let target = add_entity(
            world,
            PigEntity::new(
                &vanilla_entities::PIG,
                next_entity_id(),
                TEST_BLOCK_BOTTOM_CENTER + DVec3::X * 3.0,
                Arc::downgrade(world),
            ),
        );
        let scoreboard = world.scoreboard();
        let team = scoreboard.add_team("red").expect("team");
        scoreboard
            .set_allow_friendly_fire(&team, allow_friendly_fire)
            .expect("policy");
        for entity in [source.as_ref() as &dyn Entity, target.as_ref()] {
            scoreboard
                .add_holder_to_team(&ScoreHolder::new(entity.scoreboard_name()), &team)
                .expect("membership");
        }
        let before = target.get_health();
        let mut explosion = ServerExplosion::new(
            world,
            Some(source),
            None,
            None,
            None,
            TEST_BLOCK_BOTTOM_CENTER,
            4.0,
            false,
            BlockInteraction::Keep,
        );
        explosion.hurt_entities();
        assert_eq!(target.get_health() < before, allow_friendly_fire);
        assert!(target.velocity().x > 0.0);
    }
}

#[test]
fn player_owned_tnt_respects_player_team_and_pvp_damage_gates() {
    for (same_team, pvp) in [(true, true), (false, false), (false, true)] {
        let fixture = ready_world("explosion_owner_team");
        let world = &fixture.world;
        world.set_game_rule(&steel_registry::vanilla_game_rules::PVP, pvp);
        let player = |name, offset| {
            let player = TestPlayerBuilder::new(Arc::clone(world), name, next_entity_id()).build();
            player
                .base()
                .set_position_local(TEST_BLOCK_BOTTOM_CENTER + DVec3::X * offset);
            player.restore_game_modes(GameType::Survival, None);
            player
                .abilities
                .lock()
                .update_for_game_mode(GameType::Survival);
            player.set_client_loaded(true);
            assert!(world.add_player(Arc::clone(&player), ResetReason::InitialJoin));
            player
        };
        let owner = player("Owner", 6.0);
        let target = player("Target", 3.0);
        if same_team {
            let scoreboard = world.scoreboard();
            let team = scoreboard.add_team("red").expect("team");
            scoreboard
                .set_allow_friendly_fire(&team, false)
                .expect("policy");
            for player in [&owner, &target] {
                scoreboard
                    .add_holder_to_team(&ScoreHolder::new(player.scoreboard_name()), &team)
                    .expect("membership");
            }
        }
        let source = add_entity(
            world,
            PrimedTntEntity::primed(
                &vanilla_entities::TNT,
                next_entity_id(),
                TEST_BLOCK_BOTTOM_CENTER,
                world,
                Some(owner.as_ref()),
            ),
        );
        let before = target.get_health();
        let mut explosion = ServerExplosion::new(
            world,
            Some(source),
            None,
            None,
            None,
            TEST_BLOCK_BOTTOM_CENTER,
            4.0,
            false,
            BlockInteraction::Keep,
        );
        explosion.hurt_entities();
        assert_eq!(target.get_health() < before, !same_team && pvp);
        assert!(target.velocity().x > 0.0);
        assert!(explosion.hit_players.contains_key(&target.id()));
    }
}

#[test]
fn tnt_and_creeper_blasts_apply_vanilla_damage_and_knockback() {
    for (radius, offset, expected_damage, resistance) in [
        (3.0, 4.5, 7.5625, 0.0),
        (3.0, 4.5, 7.5625, 0.5),
        (4.0, 6.0, 9.75, 0.0),
        (4.0, 6.0, 9.75, 0.5),
    ] {
        let fixture = ready_world("explosion_damage_parity");
        let world = &fixture.world;
        let center = DVec3::new(0.5, 64.0, 0.5);
        let pig = add_entity(
            world,
            PigEntity::new(
                &vanilla_entities::PIG,
                next_entity_id(),
                center + DVec3::new(offset, 0.0, 0.0),
                Arc::downgrade(world),
            ),
        );
        pig.attributes().lock().set_base_value(
            vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE,
            resistance,
        );
        let health_before = pig.get_health();
        let mut explosion = ServerExplosion::new(
            world,
            None,
            None,
            None,
            None,
            center,
            radius,
            false,
            BlockInteraction::Destroy,
        );

        explosion.hurt_entities();

        // Normalized distance is 0.75, exposure is 1, and impact is 0.25.
        assert_eq!(health_before - pig.get_health(), expected_damage);
        let direction = (pig.explosion_damage_origin() - center).normalize();
        assert!((pig.velocity() - direction * (0.25 * (1.0 - resistance))).length() < 1.0e-12);
        assert!(pig.velocity().x > 0.0 && pig.velocity().y > 0.0);
    }
}

#[test]
fn explosion_decay_and_survival_match_java_seed_one() {
    init_vanilla_registry();
    // BlastOracle.java --loot: 64 nextFloat() <= 1/radius draws, seed 1.
    for (radius, expected_count) in [(3.0, 24), (4.0, 20)] {
        let mut random = RandomSource::Legacy(LegacyRandom::from_seed(1));
        let tool = ItemStack::empty();
        let mut context = LootContext::new(&mut random)
            .with_tool(&tool)
            .with_block_state(vanilla_blocks::STONE.default_state())
            .with_explosion(radius);
        let dropped: i32 = (0..64)
            .flat_map(|_| {
                vanilla_loot_tables::BLOCKS_STONE
                    .get_random_items(&mut context)
                    .expect("stone loot")
            })
            .map(|item| {
                assert!(item.is(&vanilla_items::COBBLESTONE));
                item.count()
            })
            .sum();
        assert_eq!(dropped, expected_count);
    }
}

#[test]
fn destructive_callbacks_preserve_loot_without_decay_and_at_radius_one() {
    for interaction in [
        BlockInteraction::Destroy,
        BlockInteraction::DestroyWithDecay,
    ] {
        let fixture = ready_world("explosion_loot_context");
        let world = &fixture.world;
        let mut affected = (0..12).map(|x| BlockPos::new(x, 64, 0)).collect::<Vec<_>>();
        for &pos in &affected {
            assert!(world.set_block(
                pos,
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE
            ));
        }
        let radius = if interaction == BlockInteraction::Destroy {
            4.0
        } else {
            1.0
        };
        let explosion = ServerExplosion::new(
            world,
            None,
            None,
            None,
            None,
            TEST_BLOCK_CENTER,
            radius,
            false,
            interaction,
        );
        explosion.interact_with_blocks(&mut affected, &mut LegacyRandom::from_seed(1));
        assert!(
            affected
                .iter()
                .all(|&pos| world.get_block_state(pos).is_air())
        );
        let drops = world.get_entities_in_aabb_matching(
            &WorldAabb::new(-1.0, 63.0, -1.0, 13.0, 66.0, 2.0),
            |entity| entity.entity_type() == &vanilla_entities::ITEM,
        );
        let count: i32 = drops
            .iter()
            .filter_map(|entity| entity.downcast_ref::<ItemEntity>())
            .map(|item| {
                let stack = item.get_item();
                assert!(stack.is(&vanilla_items::COBBLESTONE));
                stack.count()
            })
            .sum();
        assert_eq!(count, 12);
    }
}

#[test]
fn explosion_removal_spills_barrel_contents_once() {
    let fixture = ready_world("explosion_barrel_contents");
    init_block_entities();
    let world = &fixture.world;
    let pos = BlockPos::new(4, 64, 4);
    assert!(world.set_block(
        pos,
        vanilla_blocks::BARREL.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let block_entity = world.get_block_entity(pos).expect("barrel block entity");
    let container_ref = ContainerRef::from_block_entity(block_entity).expect("barrel container");
    {
        let mut guard = ContainerLockGuard::lock_all(&[&container_ref]).expect("lock barrel");
        let container = guard
            .get_mut(container_ref.container_id())
            .expect("locked barrel");
        container.set_item(0, ItemStack::with_count(&vanilla_items::DIAMOND, 23));
    }
    let explosion = ServerExplosion::new(
        world,
        None,
        None,
        None,
        None,
        TEST_BLOCK_CENTER,
        4.0,
        false,
        BlockInteraction::Destroy,
    );
    explosion.interact_with_blocks(&mut [pos], &mut LegacyRandom::from_seed(1));
    assert!(world.get_block_state(pos).is_air());
    assert!(world.get_block_entity(pos).is_none());
    let drops = world.get_entities_in_aabb(&WorldAabb::new(3.0, 63.0, 3.0, 6.0, 67.0, 6.0));
    let diamonds: i32 = drops
        .iter()
        .filter_map(|entity| entity.downcast_ref::<ItemEntity>())
        .map(ItemEntity::get_item)
        .filter(|stack| stack.is(&vanilla_items::DIAMOND))
        .map(|stack| stack.count())
        .sum();
    assert_eq!(diamonds, 23);
}

#[test]
fn crystal_destruction_explodes_unless_the_damage_was_an_explosion() {
    for hit_by_explosion in [false, true] {
        let fixture = ready_world("explosion_crystal_source");
        let world = &fixture.world;
        let glass = BlockPos::new(5, 64, 4);
        assert!(world.set_block(
            glass,
            vanilla_blocks::GLASS.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
        let crystal = add_entity(
            world,
            EndCrystalEntity::new(
                &vanilla_entities::END_CRYSTAL,
                next_entity_id(),
                DVec3::new(4.5, 64.0, 4.5),
                Arc::downgrade(world),
            ),
        );
        let source = if hit_by_explosion {
            DamageSource::explosion(None, None)
        } else {
            DamageSource::environment(&vanilla_damage_types::GENERIC)
        };
        assert!(crystal.hurt(world, &source, 1.0));
        assert!(crystal.is_removed());
        assert_eq!(world.get_block_state(glass).is_air(), !hit_by_explosion);
    }
}
