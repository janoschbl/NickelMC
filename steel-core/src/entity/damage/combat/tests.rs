use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_registry::{
    data_components::vanilla_components::CUSTOM_NAME, init_vanilla_registry, item_stack::ItemStack,
    vanilla_damage_types, vanilla_entities, vanilla_items,
};
use text_components::{
    TextComponent, content::Content, interactivity::HoverEvent, translation::TranslatedMessage,
};

use super::*;
use crate::behavior::init_behaviors;
use crate::entity::{Entity, RemovalReason, SharedEntity, entities::PigEntity};
use crate::inventory::equipment::EquipmentSlot;
use crate::test_support::{TestEntity, TestPlayerBuilder, fresh_test_world};

fn entry(source: DamageSource) -> CombatEntry {
    CombatEntry {
        source,
        damage: 2.0,
        fall_location: None,
        fall_distance: 0.0,
    }
}

fn translation(message: &TextComponent) -> &TranslatedMessage {
    let Content::Translate(translation) = &message.content else {
        panic!("expected a translated death message");
    };
    translation
}

#[test]
fn environmental_damage_expires_strictly_after_one_hundred_entity_ticks() {
    let mut tracker = CombatTracker::default();
    let changes = tracker.record_damage(
        entry(DamageSource::environment(&vanilla_damage_types::GENERIC)),
        7,
        true,
    );
    assert!(!changes.entered);
    assert!(tracker.recheck_status(107, true).released.is_empty());
    assert_eq!(tracker.entries.len(), 1);
    let expired = tracker.recheck_status(108, true);
    assert_eq!(expired.released.len(), 1);
    assert!(expired.left.is_none());
    assert!(tracker.entries.is_empty());
}

#[test]
fn living_damage_enters_combat_and_expires_strictly_after_three_hundred_ticks() {
    init_vanilla_registry();
    let attacker: SharedEntity = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        1,
        DVec3::ZERO,
        Weak::new(),
    ));
    let mut tracker = CombatTracker::default();
    let source = DamageSource::direct(&vanilla_damage_types::MOB_ATTACK, attacker);
    assert!(
        tracker
            .record_damage(entry(source.clone()), 7, true)
            .entered
    );
    assert!(!tracker.record_damage(entry(source), 8, true).entered);
    assert!(tracker.recheck_status(308, true).left.is_none());
    assert_eq!(tracker.duration(308), 301);
    let expired = tracker.recheck_status(309, true);
    assert_eq!(expired.left, Some(302));
    assert_eq!(expired.released.len(), 2);
    assert_eq!(tracker.duration(500), 302);
    assert!(tracker.entries.is_empty());
}

#[test]
fn removed_named_attacker_is_retained_and_assisted_fall_uses_its_weapon() {
    init_vanilla_registry();
    init_behaviors();
    let world_fixture = fresh_test_world("combat_assisted_fall");
    let victim = TestPlayerBuilder::new(Arc::clone(&world_fixture.world), "Victim", 1).build();
    let attacker: SharedEntity =
        TestPlayerBuilder::new(Arc::clone(&world_fixture.world), "Original", 2).build();
    let weak = Arc::downgrade(&attacker);
    let mut weapon = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
    weapon.set(CUSTOM_NAME, TextComponent::const_plain("Named blade"));
    attacker
        .as_living_entity()
        .expect("player")
        .with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |item| *item = weapon.clone());
    let source = DamageSource::direct(&vanilla_damage_types::PLAYER_ATTACK, attacker);
    let mut tracker = CombatTracker::default();
    tracker.record_damage(entry(source), 0, true);
    weak.upgrade()
        .expect("retained attacker")
        .set_removed(RemovalReason::Discarded);
    let generic = tracker
        .death_message(victim.as_ref())
        .expect("named weapon death message");
    let args = translation(&generic).args.as_ref().expect("message args");
    assert_eq!(translation(&generic).key, "death.attack.player.item");
    assert_eq!(args.len(), 3);
    assert!(matches!(
        args[2].interactions.hover,
        Some(HoverEvent::ShowItem { .. })
    ));

    let mut fall = entry(DamageSource::environment(&vanilla_damage_types::FALL));
    fall.fall_distance = 6.0;
    tracker.record_damage(fall, 1, true);
    let assisted = tracker
        .death_message(victim.as_ref())
        .expect("assisted fall message");
    assert_eq!(translation(&assisted).key, "death.fell.assist.item");
    assert_eq!(
        translation(&assisted).args.as_ref().expect("args")[1],
        weak.upgrade().expect("exact original").display_name()
    );
    let changes = tracker.recheck_status(1, false);
    assert!(tracker.entries.is_empty());
    assert_eq!(changes.left, Some(1));
    drop(changes);
    assert!(weak.upgrade().is_none());
}

#[test]
fn indirect_damage_names_the_causing_entity_instead_of_the_projectile() {
    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("combat_projectile_owner");
    let victim = TestPlayerBuilder::new(Arc::clone(&fixture.world), "Victim", 1).build();
    let attacker = TestPlayerBuilder::new(Arc::clone(&fixture.world), "Shooter", 2).build();
    let mut weapon = ItemStack::new(&vanilla_items::BOW);
    weapon.set(CUSTOM_NAME, TextComponent::const_plain("Named bow"));
    attacker.with_equipment_slot_mut(EquipmentSlot::MainHand, &mut |item| *item = weapon.clone());
    let projectile = TestEntity::shared(3, DVec3::ZERO, Weak::new(), &vanilla_entities::ARROW);
    let source = DamageSource::environment(&vanilla_damage_types::ARROW)
        .with_causing_entity(Arc::clone(&attacker) as SharedEntity)
        .with_direct_entity(projectile);
    let mut tracker = CombatTracker::default();
    tracker.record_damage(entry(source), 0, true);
    let message = tracker.death_message(victim.as_ref()).expect("message");
    assert_eq!(translation(&message).key, "death.attack.arrow.item");
    assert_eq!(
        translation(&message).args.as_ref().expect("args")[1],
        attacker.display_name()
    );
}

#[test]
fn combat_records_only_damage_remaining_after_armor_and_absorption() {
    use steel_registry::vanilla_attributes;

    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("combat_damage_reductions");
    let player = TestPlayerBuilder::new(Arc::clone(&fixture.world), "Player", 1).build();
    player.set_client_loaded(true);
    let player: SharedEntity = player;
    let pig: SharedEntity = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        2,
        DVec3::ZERO,
        Arc::downgrade(&fixture.world),
    ));
    let source = DamageSource::environment(&vanilla_damage_types::FIREWORKS);
    for entity in [player, pig] {
        let victim = entity.as_living_entity().expect("living victim");
        let initial_health = victim.get_health();
        let _owner = entity.base().damage_history().retain_owner();
        {
            let mut attributes = victim.attributes().lock();
            attributes.set_base_value(vanilla_attributes::ARMOR, 20.0);
            attributes.set_base_value(vanilla_attributes::MAX_ABSORPTION, 3.0);
        }
        victim.set_absorption_amount(3.0);
        victim.actually_hurt(&fixture.world, &source, 1.0);
        assert!(
            entity
                .base()
                .damage_history()
                .combat_tracker()
                .entries
                .is_empty()
        );
        victim.set_absorption_amount(3.0);
        victim.actually_hurt(&fixture.world, &source, 10.0);
        let tracker = entity.base().damage_history().combat_tracker();
        assert_eq!(tracker.entries.len(), 1);
        assert!((tracker.entries[0].damage - 1.0).abs() < 1.0e-5);
        assert!((victim.get_health() - (initial_health - 1.0)).abs() < 1.0e-5);
    }
}

#[test]
fn living_tick_rechecks_combat_every_twenty_entity_ticks_not_world_ticks() {
    use crate::entity::Mob;
    use crate::test_support::{advance_test_game_time_to, insert_ready_full_chunk};
    use steel_utils::ChunkPos;

    init_vanilla_registry();
    init_behaviors();
    let fixture = fresh_test_world("combat_tick_cadence");
    let world = &fixture.world;
    insert_ready_full_chunk(world, ChunkPos::new(0, 0));
    let victim = Arc::new(PigEntity::new(
        &vanilla_entities::PIG,
        1,
        DVec3::ZERO,
        Arc::downgrade(world),
    ));
    victim.set_no_ai(true);
    victim.set_no_gravity(true);
    let attacker = TestPlayerBuilder::new(Arc::clone(world), "Attacker", 2).build();
    let shared: SharedEntity = Arc::clone(&victim) as SharedEntity;
    let _owner = victim.base().damage_history().retain_owner();
    victim.record_combat_damage(
        &DamageSource::direct(&vanilla_damage_types::PLAYER_ATTACK, attacker),
        1.0,
    );
    advance_test_game_time_to(world, 1000);
    world.damage_history.upgrade().expect("history").expire();
    for _ in 0..319 {
        victim.advance_tick_count();
        victim.tick_living_entity(&shared);
    }
    assert_eq!(victim.tick_count(), 319);
    assert_eq!(
        victim
            .base()
            .damage_history()
            .combat_tracker()
            .entries
            .len(),
        1
    );
    victim.advance_tick_count();
    victim.tick_living_entity(&shared);
    assert!(
        victim
            .base()
            .damage_history()
            .combat_tracker()
            .entries
            .is_empty()
    );
    assert_eq!(victim.combat_duration(), 320);
}

#[test]
fn fall_selection_uses_strict_distance_threshold_and_largest_climbable_damage() {
    init_vanilla_registry();
    let world_fixture = fresh_test_world("combat_fall_threshold");
    let victim = TestPlayerBuilder::new(Arc::clone(&world_fixture.world), "Victim", 1).build();
    let mut tracker = CombatTracker::default();
    let mut first = entry(DamageSource::environment(&vanilla_damage_types::FALL));
    first.fall_distance = 5.0;
    tracker.record_damage(first, 0, true);
    assert_eq!(
        translation(&tracker.death_message(victim.as_ref()).expect("message")).key,
        "death.attack.fall"
    );
    tracker.entries[0].fall_location = Some("ladder");
    tracker.entries[0].damage = 6.0;
    assert_eq!(
        translation(&tracker.death_message(victim.as_ref()).expect("message")).key,
        "death.fell.accident.ladder"
    );
    let mut second = entry(DamageSource::environment(&vanilla_damage_types::FALL));
    second.fall_location = Some("water");
    second.damage = 7.0;
    tracker.record_damage(second, 1, true);
    assert_eq!(
        translation(&tracker.death_message(victim.as_ref()).expect("message")).key,
        "death.fell.accident.water"
    );
}

#[test]
fn intentional_game_design_message_carries_the_vanilla_link_and_hover() {
    init_vanilla_registry();
    let world_fixture = fresh_test_world("combat_intentional_design");
    let victim = TestPlayerBuilder::new(Arc::clone(&world_fixture.world), "Victim", 1).build();
    let mut tracker = CombatTracker::default();
    tracker.record_damage(
        entry(DamageSource::environment(
            &vanilla_damage_types::BAD_RESPAWN_POINT,
        )),
        0,
        true,
    );
    let message = tracker.death_message(victim.as_ref()).expect("message");
    assert_eq!(
        translation(&message).key,
        "death.attack.badRespawnPoint.message"
    );
    let args = translation(&message).args.as_ref().expect("args");
    assert!(args[1].interactions.click.is_some());
    assert!(args[1].interactions.hover.is_some());
}
