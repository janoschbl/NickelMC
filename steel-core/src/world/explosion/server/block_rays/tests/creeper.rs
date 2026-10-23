//! Creeper-sized blasts over mixed-resistance blocks, compared with `BlastOracle.java`.

use std::fmt::Write as _;

use std::sync::Arc;

use steel_registry::blocks::BlockRef;

use super::*;

/// Resistances come from the registry, so this also pins the vanilla values used by the oracle.
fn populate_fixture(world: &Arc<World>) {
    for x in -1..=1 {
        for z in -1..=1 {
            insert_ready_full_chunk(world, ChunkPos::new(x, z));
        }
    }
    let place = |pos: BlockPos, block: BlockRef| {
        assert!(world.set_block(pos, block.default_state(), UpdateFlags::UPDATE_NONE));
    };
    for x in -12..=12 {
        for z in -12..=12 {
            for y in 50..=62 {
                place(BlockPos::new(x, y, z), &vanilla_blocks::STONE);
            }
            place(BlockPos::new(x, 63, z), &vanilla_blocks::DIRT);
        }
    }
    for z in -2..=2 {
        for y in 64..=66 {
            place(BlockPos::new(-2, y, z), &vanilla_blocks::GLASS);
        }
    }
    for y in 64..=65 {
        place(BlockPos::new(3, y, 1), &vanilla_blocks::OBSIDIAN);
        for x in 1..=2 {
            for z in -1..=0 {
                place(BlockPos::new(x, y, z), &vanilla_blocks::WATER);
            }
        }
    }
}

fn format_positions(positions: &[BlockPos]) -> String {
    let mut text = String::new();
    for pos in positions {
        writeln!(text, "{} {} {}", pos.x(), pos.y(), pos.z()).expect("string write");
    }
    text
}

fn check_case(name: &str, radius: f32, seed: u64, immutable: bool) {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("explosion_creeper_oracle");
    let world = &fixture.world;
    populate_fixture(world);
    let explosion = ServerExplosion::new(
        world,
        None,
        None,
        None,
        immutable
            .then_some(&DefaultExplosionDamageCalculator as &dyn ImmutableExplosionBlockCalculator),
        DVec3::new(0.5, 64.0, 0.5),
        radius,
        false,
        BlockInteraction::DestroyWithDecay,
    );
    if immutable {
        let bounds = explosion
            .immutable_ray_region_bounds(0)
            .expect("fixture ray region");
        world
            .try_with_block_region(bounds, |region| assert!(region.has_complete_data()))
            .expect("fixture has a resident ray region");
    }

    let original = (-12..=12)
        .flat_map(|x| (-12..=12).flat_map(move |z| (50..=75).map(move |y| BlockPos::new(x, y, z))))
        .map(|pos| (pos, world.get_block_state(pos)))
        .filter(|(_, state)| !state.is_air())
        .collect::<Vec<_>>();
    let mut random = LegacyRandom::from_seed(seed);
    let mut affected = explosion.calculate_exploded_positions(|| random.next_f32());
    let iteration = format_positions(&affected);
    explosion.interact_with_blocks(&mut affected, &mut random);
    let shuffled = format_positions(&affected);

    let oracle = include_str!("creeper-blast-order.txt");
    let case = oracle
        .split("case ")
        .find(|case| case.starts_with(name))
        .expect("oracle case");
    let (header, rest) = case.split_once("iteration\n").expect("iteration section");
    let (expected_iteration, expected_shuffled) =
        rest.split_once("shuffled\n").expect("shuffled section");
    assert_eq!(
        header.trim(),
        format!("{name} count {}", affected.len()),
        "{name} affected count"
    );
    assert_eq!(
        iteration, expected_iteration,
        "{name} HashSet iteration order"
    );
    assert_eq!(
        shuffled, expected_shuffled,
        "{name} shuffled callback order"
    );
    let mut destroyed = 0;
    for (pos, state) in original {
        let after = world.get_block_state(pos);
        if affected.contains(&pos) {
            assert!(after.is_air(), "{name} must destroy {pos:?}");
            destroyed += 1;
        } else {
            assert_eq!(after, state, "{name} must preserve {pos:?}");
        }
    }
    assert!(destroyed > 0, "fixture must exercise actual destruction");
}

#[test]
fn creeper_blast_matches_vanilla_set_and_order() {
    for immutable in [false, true] {
        check_case("creeper-radius-3", 3.0, 20260, immutable);
    }
}

#[test]
fn charged_creeper_blast_matches_vanilla_set_and_order() {
    for immutable in [false, true] {
        check_case("charged-creeper-radius-6", 6.0, 20261, immutable);
    }
}

#[test]
fn tnt_blast_matches_vanilla_set_order_and_destruction() {
    for immutable in [false, true] {
        check_case("tnt-radius-4", 4.0, 20262, immutable);
    }
}
