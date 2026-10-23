use super::{vanilla::fixtures, *};
use crate::behavior::init_behaviors;
use crate::block_entity::{BlockEntity as _, entities::BeehiveBlockEntity};
use std::sync::Weak;
use steel_registry::{
    blocks::{block_state_ext::BlockStateExt, properties::BlockStateProperties},
    data_components::vanilla_components::{
        BEES, BLOCK_STATE, CUSTOM_NAME, SUSPICIOUS_STEW_EFFECTS,
    },
    init_vanilla_registry,
    item_stack::ItemStack,
    vanilla_blocks, vanilla_enchantments, vanilla_items, vanilla_mob_effects,
};
use steel_utils::random::{Random, RandomSource, legacy_random::LegacyRandom};
use text_components::TextComponent;

fn random(seed: u64) -> RandomSource {
    RandomSource::Legacy(LegacyRandom::from_seed(seed))
}

fn tables() -> LootTableBehaviorRegistry {
    init_vanilla_registry();
    init_behaviors();
    let mut tables = LootTableBehaviorRegistry::new();
    fixtures::register(&mut tables);
    tables
}

#[test]
fn level_enchanting_matches_java_single_option_vectors_and_discards_book_patch() {
    tables();
    // Standalone Java transcription of 26.3 EnchantmentHelper.selectEnchantment,
    // using java.util.Random and this checkout's extracted book/unbreaking data.
    // This is not a full vanilla-runtime differential test.
    for (seed, level, next) in [
        (0, 3, -8_292_973_307_042_192_125),
        (1, 2, 6_137_546_356_583_794_141),
        (12_345, 3, 6_022_414_958_441_676_900),
        (2_147_483_647, 3, 1_248_961_208_387_847_436),
    ] {
        let mut rng = random(seed);
        let mut ctx = LootContext::new(&mut rng);
        let mut item = ItemStack::with_count(&vanilla_items::BOOK, 7);
        item.set(CUSTOM_NAME, TextComponent::plain("discarded"));
        modifiers::enchanting::enchant_with_levels(
            &mut ctx,
            &mut item,
            20,
            vec![&vanilla_enchantments::UNBREAKING],
            false,
        );
        assert!(item.is(&vanilla_items::ENCHANTED_BOOK));
        assert_eq!(item.count(), 1);
        assert!(!item.has(CUSTOM_NAME));
        assert_eq!(
            item.get_enchantments_for_crafting()
                .expect("stored enchantments")
                .get_level(&vanilla_enchantments::UNBREAKING.key) as i32,
            level
        );
        assert_eq!(rng.next_i64(), next);
    }
}

#[test]
fn random_enchantment_consumes_choice_level_and_optional_trade_cost_only() {
    tables();
    for allowed in [false, true] {
        let mut expected = random(12_345);
        expected.next_i32_bounded(1);
        let level = expected.next_i32_between(1, vanilla_enchantments::UNBREAKING.max_level as i32);
        let cost = allowed.then(|| 2 + expected.next_i32_bounded(5 + level * 10) + 3 * level);
        let mut rng = random(12_345);
        let mut ctx = LootContext::new(&mut rng);
        ctx.additional_cost_component_allowed = allowed;
        let mut item = ItemStack::new(&vanilla_items::BOOK);
        modifiers::enchanting::enchant_randomly(
            &mut ctx,
            &mut item,
            vec![&vanilla_enchantments::UNBREAKING],
            true,
            true,
        );
        assert_eq!(
            item.get_enchantments_for_crafting()
                .expect("stored enchantments")
                .get_level(&vanilla_enchantments::UNBREAKING.key) as i32,
            level
        );
        assert_eq!(
            item.get(steel_registry::data_components::vanilla_components::ADDITIONAL_TRADE_COST)
                .copied(),
            cost
        );
        assert_eq!(rng.next_i64(), expected.next_i64());
    }
}

#[test]
fn compiled_damage_uses_remaining_durability_and_skips_rng_for_non_damageable_items() {
    let tables = tables();
    let mut rng = random(123);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    let items = fixtures::DAMAGE_FRACTION
        .get_random_items(&mut ctx)
        .expect("damage loot");
    let max = items[0].get_max_damage();
    let first = (0.75 * max as f32).floor() as i32;
    let remaining = 1.0 - first as f32 / max as f32;
    assert_eq!(
        items[0].get_damage_value(),
        ((1.0 - (0.1 + remaining)) * max as f32).floor() as i32
    );
    let mut expected = random(123);
    let mut rng = random(123);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    assert_eq!(
        fixtures::DAMAGE_NON_DAMAGEABLE
            .get_random_items(&mut ctx)
            .expect("unchanged loot")[0],
        ItemStack::new(&vanilla_items::DIAMOND)
    );
    assert_eq!(rng.next_i64(), expected.next_i64());
}

#[test]
fn copy_state_requires_context_and_copies_the_requested_property() {
    let tables = tables();
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    assert_eq!(
        fixtures::COPY_STATE
            .get_random_items(&mut ctx)
            .expect("no state")[0],
        ItemStack::new(&vanilla_items::BEE_NEST)
    );
    ctx.block_state = Some(
        vanilla_blocks::BEE_NEST
            .default_state()
            .set_value(&BlockStateProperties::LEVEL_HONEY, 5),
    );
    let items = fixtures::COPY_STATE
        .get_random_items(&mut ctx)
        .expect("copied state");
    assert_eq!(
        items[0]
            .get(BLOCK_STATE)
            .expect("state component")
            .properties()
            .get("honey_level")
            .map(String::as_str),
        Some("5")
    );
}

#[test]
fn stew_appends_effects_and_scales_only_noninstant_durations() {
    let tables = tables();
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    let items = fixtures::STEW_DURATION
        .get_random_items(&mut ctx)
        .expect("stew loot");
    let effects = items[0]
        .get(SUSPICIOUS_STEW_EFFECTS)
        .expect("effects")
        .effects();
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].effect(), vanilla_mob_effects::POISON);
    assert_eq!(effects[0].duration(), 60);
    assert_eq!(effects[1].effect(), vanilla_mob_effects::INSTANT_HEALTH);
    assert_eq!(effects[1].duration(), 2);
}

#[test]
fn absent_capabilities_do_not_match_but_empty_hook_subpredicate_is_unconstrained() {
    let tables = tables();
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    ctx.this_entity = Some(EntityRef::default());
    for table in [
        &fixtures::MISSING_CUBE_MOB,
        &fixtures::MISSING_RAIDER,
        &fixtures::MISSING_FROG,
    ] {
        assert!(
            table
                .get_random_items(&mut ctx)
                .expect("closed predicate")
                .is_empty()
        );
    }
    assert_eq!(
        fixtures::FISHING_HOOK_ANY
            .get_random_items(&mut ctx)
            .expect("unconstrained predicate")
            .len(),
        1
    );
}

#[test]
fn unsupported_exploration_map_preserves_the_original_stack() {
    let tables = tables();
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    let items = fixtures::EXPLORATION_MAP
        .get_random_items(&mut ctx)
        .expect("logged map gap");
    assert_eq!(items.len(), 1);
    assert!(items[0].is(&vanilla_items::MAP));
    assert_eq!(items[0].count(), 3);
    assert_eq!(
        items[0].get(CUSTOM_NAME),
        Some(&TextComponent::plain("Unchanged"))
    );
}

#[test]
fn copy_components_preserves_stored_bees_through_item_placement() {
    let tables = tables();
    let source = BeehiveBlockEntity::new(
        Weak::new(),
        steel_utils::BlockPos::ZERO,
        vanilla_blocks::BEE_NEST.default_state(),
    );
    source.store_worldgen_bee(123);
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng).with_block_entity(&source);
    ctx.tables = Some(&tables);
    let items = fixtures::COPY_BEES
        .get_random_items(&mut ctx)
        .expect("copied bee components");
    let occupants = items[0].get(BEES).expect("bee component").bees();
    assert_eq!(occupants.len(), 1);
    assert_eq!(occupants[0].ticks_in_hive(), 123);
    let placed = BeehiveBlockEntity::new(
        Weak::new(),
        steel_utils::BlockPos::ZERO,
        vanilla_blocks::BEE_NEST.default_state(),
    );
    placed.apply_components_from_item_stack(&items[0]);
    assert_eq!(placed.save_custom_only(), source.save_custom_only());
}

#[test]
fn dynamic_drops_require_the_real_owner_and_propagate_errors() {
    let tables = tables();
    let mut rng = random(0);
    let mut ctx = LootContext::new(&mut rng);
    ctx.tables = Some(&tables);
    assert!(matches!(
        fixtures::DYNAMIC_OWNER.get_random_items(&mut ctx),
        Err(LootError::Unsupported { .. })
    ));
    let provider = |key: &steel_utils::Identifier| {
        assert_eq!(key, &steel_utils::Identifier::vanilla_static("sherds"));
        Ok(vec![ItemStack::with_count(&vanilla_items::BRICK, 4)])
    };
    ctx.dynamic_drops = Some(&provider);
    let items = fixtures::DYNAMIC_OWNER
        .get_random_items(&mut ctx)
        .expect("owner drops");
    assert_eq!(items, [ItemStack::with_count(&vanilla_items::BRICK, 4)]);
}
