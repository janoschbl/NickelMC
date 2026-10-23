use super::*;
use crate::bootstrap::init_globals;
use crate::entity::{LivingEntity, create_entity_instance};
use crate::test_support::{TestPlayerBuilder, fresh_test_world};
use glam::DVec3;
use std::sync::Arc;
use steel_registry::{vanilla_advancements, vanilla_entities, vanilla_items};

fn done(player: &Player, advancement: AdvancementRef) -> bool {
    player
        .advancements
        .lock()
        .progress
        .get_progress(advancement)
        .is_some_and(|progress| progress.is_done())
}

#[test]
fn enchanted_item_dispatch_checks_arguments_and_does_not_reaward_completed_criteria() {
    init_globals();
    let fixture = fresh_test_world("enchanted_item_trigger");
    fixture.world.set_game_rule(
        &steel_registry::vanilla_game_rules::SHOW_ADVANCEMENT_MESSAGES,
        false,
    );
    let player = TestPlayerBuilder::new(Arc::clone(&fixture.world), "Enchanter", 6001).build();
    let advancement = &*vanilla_advancements::STORY_ENCHANT_ITEM;
    let listeners = [CriterionListener {
        advancement,
        criterion: "enchanted_item",
        matches: |player, event| {
            // Predicates run without the advancement lock held.
            let _progress = player.advancements.lock();
            matches!(event, CriterionEvent::EnchantedItem { item, levels: 3 } if item.is(&vanilla_items::ENCHANTED_BOOK))
        },
    }];
    let item = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
    player.trigger_criterion_listeners(
        &CriterionEvent::EnchantedItem {
            item: &item,
            levels: 2,
        },
        &listeners,
    );
    assert!(!done(&player, advancement));
    player.trigger_criterion_listeners(
        &CriterionEvent::EnchantedItem {
            item: &item,
            levels: 3,
        },
        &listeners,
    );
    assert!(done(&player, advancement));
    let before = player
        .advancements
        .lock()
        .progress
        .get_progress(advancement)
        .expect("progress")
        .criteria["enchanted_item"]
        .clone();
    player.trigger_criterion(CriterionEvent::EnchantedItem {
        item: &item,
        levels: 3,
    });
    let after = player
        .advancements
        .lock()
        .progress
        .get_progress(advancement)
        .expect("progress")
        .criteria["enchanted_item"]
        .clone();
    assert_eq!(before.0, after.0);
}

#[test]
fn effects_changed_requires_every_effect_and_source_predicates_reject_missing_sources() {
    init_globals();
    let fixture = fresh_test_world("effects_changed_trigger");
    fixture.world.set_game_rule(
        &steel_registry::vanilla_game_rules::SHOW_ADVANCEMENT_MESSAGES,
        false,
    );
    let player = TestPlayerBuilder::new(Arc::clone(&fixture.world), "Alchemist", 6002).build();
    let advancement = &*vanilla_advancements::NETHER_ALL_POTIONS;
    player.trigger_criterion(CriterionEvent::EffectsChanged { source: None });
    assert!(!done(&player, advancement));
    for (_, effect) in steel_registry::REGISTRY.mob_effects.iter() {
        player.set_mob_effect(effect, 0);
    }
    player.trigger_criterion(CriterionEvent::EffectsChanged { source: None });
    assert!(done(&player, advancement));
    let axolotl_advancement = &*vanilla_advancements::HUSBANDRY_KILL_AXOLOTL_TARGET;
    assert!(!done(&player, axolotl_advancement));
    let pig = create_entity_instance(&fixture.world, &vanilla_entities::PIG, DVec3::ZERO)
        .expect("pig factory");
    player.trigger_criterion(CriterionEvent::EffectsChanged {
        source: Some(pig.as_ref()),
    });
    assert!(!done(&player, axolotl_advancement));
}
