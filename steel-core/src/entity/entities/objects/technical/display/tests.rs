use super::*;
use block_display::BlockDisplayEntity;
use glam::{DVec3, Quat, Vec3};
use item_display::{ItemDisplayContext, ItemDisplayEntity};
use simdnbt::borrow::read_compound;
use std::io::Cursor;
use std::sync::Weak;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, Direction};
use steel_registry::entity_data::EntityData;
use steel_registry::item_stack::ItemStack;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
use text_components::TextComponent;
use text_display::{Alignment, TextDisplayEntity};

fn reload(entity: &impl Entity, encoded: &NbtCompound) {
    let mut bytes = Vec::new();
    encoded.write(&mut bytes);
    let compound = read_compound(&mut Cursor::new(bytes.as_slice())).unwrap();
    entity.load_additional(BorrowedNbtCompoundView::from(&compound));
}

#[test]
fn display_item_slot_zero_updates_metadata_and_rejects_other_slots() {
    init_vanilla_registry();
    let entity =
        ItemDisplayEntity::new(&vanilla_entities::ITEM_DISPLAY, 1, DVec3::ZERO, Weak::new());
    let slot = entity.get_slot(0).unwrap();
    assert!(slot.get().is_empty());
    assert!(slot.set(ItemStack::new(&vanilla_items::DIAMOND)));
    assert_eq!(slot.get().item(), &*vanilla_items::DIAMOND);
    entity.with_view(|view| assert_eq!(view.item_stack_ref().item(), &*vanilla_items::DIAMOND));
    assert!(entity.get_slot(1).is_none());
    assert!(entity.get_slot(-1).is_none());
    assert!(slot.set(ItemStack::empty()));
    assert!(slot.get().is_empty());
}

#[test]
fn display_block_round_trip_preserves_transform_brightness_and_state() {
    init_vanilla_registry();
    let entity = BlockDisplayEntity::new(
        &vanilla_entities::BLOCK_DISPLAY,
        1,
        DVec3::ZERO,
        Weak::new(),
    );
    let transform = Transformation {
        translation: Vec3::new(1.0, 2.0, 3.0),
        left_rotation: Quat::IDENTITY,
        scale: Vec3::new(2.0, 3.0, 4.0),
        right_rotation: Quat::IDENTITY,
    };
    let state = vanilla_blocks::PISTON
        .default_state()
        .set_value(&BlockStateProperties::FACING, Direction::East);
    entity.with_view(|mut view| {
        view.set_block_state(state);
        view.set_transformation(transform);
        view.set_brightness_override(Some(Brightness { block: 7, sky: 13 }));
        view.set_billboard_constraints(BillboardConstraints::Center);
        view.set_transformation_interpolation_delay(12);
    });
    let mut saved = NbtCompound::new();
    entity.save_additional(&mut saved);
    assert!(saved.get("start_interpolation").is_none());
    let restored = BlockDisplayEntity::new(
        &vanilla_entities::BLOCK_DISPLAY,
        2,
        DVec3::ZERO,
        Weak::new(),
    );
    reload(&restored, &saved);
    restored.with_view(|view| {
        assert_eq!(view.block_state(), state);
        assert_eq!(view.transformation(), transform);
        assert_eq!(
            view.brightness_override().unwrap().pack(),
            Brightness { block: 7, sky: 13 }.pack()
        );
        assert!(matches!(
            view.billboard_constraints(),
            BillboardConstraints::Center
        ));
        assert_eq!(view.transformation_interpolation_delay(), 0);
    });
    let metadata = restored.pack_dirty_entity_data().unwrap();
    assert!(
        metadata.iter().any(
            |value| matches!(value.value, EntityData::Vector3(v) if v == transform.translation)
        )
    );
    assert!(
        metadata
            .iter()
            .any(|value| matches!(value.value, EntityData::BlockState(v) if v == state))
    );
    assert!(restored.pack_dirty_entity_data().is_none());
}

#[test]
fn display_start_interpolation_retransmits_even_when_unchanged() {
    init_vanilla_registry();
    let entity = BlockDisplayEntity::new(
        &vanilla_entities::BLOCK_DISPLAY,
        1,
        DVec3::ZERO,
        Weak::new(),
    );
    for _ in 0..2 {
        entity.with_view(|mut view| view.set_transformation_interpolation_delay(0));
        let metadata = entity.pack_dirty_entity_data().unwrap();
        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].index, 8);
        assert!(matches!(metadata[0].value, EntityData::Int(0)));
    }
}

#[test]
fn display_primitive_nbt_getters_floor_floats_and_narrow_bytes() {
    init_vanilla_registry();
    let entity =
        TextDisplayEntity::new(&vanilla_entities::TEXT_DISPLAY, 1, DVec3::ZERO, Weak::new());
    let mut saved = NbtCompound::new();
    saved.insert("line_width", -0.5_f64);
    saved.insert("shadow", 256_i32);
    saved.insert("see_through", -0.5_f32);
    saved.insert("text_opacity", 256_i64);
    reload(&entity, &saved);
    entity.with_view(|view| {
        assert_eq!(view.line_width(), -1);
        assert!(!view.shadow());
        assert!(view.see_through());
        assert_eq!(view.text_opacity(), 0);
    });
}

#[test]
fn display_load_coerces_numeric_tags_and_defaults_invalid_block_properties() {
    init_vanilla_registry();
    let entity = BlockDisplayEntity::new(
        &vanilla_entities::BLOCK_DISPLAY,
        1,
        DVec3::ZERO,
        Weak::new(),
    );
    let mut saved = NbtCompound::new();
    saved.insert("teleport_duration", 100_i64);
    saved.insert("view_range", 3_i32);
    let mut properties = NbtCompound::new();
    properties.insert("facing", "east");
    properties.insert("extended", "not_a_boolean");
    properties.insert("unknown", "ignored");
    let mut block = NbtCompound::new();
    block.insert("id", "minecraft:piston");
    block.insert("properties", properties);
    saved.insert("block_state", block);
    reload(&entity, &saved);
    entity.with_view(|view| {
        assert_eq!(view.pos_rot_interpolation_duration(), 59);
        assert_eq!(view.view_range(), 3.0);
        assert_eq!(
            view.block_state(),
            vanilla_blocks::PISTON
                .default_state()
                .set_value(&BlockStateProperties::FACING, Direction::East)
        );
    });
}

#[test]
fn display_item_round_trip_preserves_stack_and_context() {
    init_vanilla_registry();
    let entity =
        ItemDisplayEntity::new(&vanilla_entities::ITEM_DISPLAY, 1, DVec3::ZERO, Weak::new());
    entity.with_view(|mut view| {
        view.set_item_stack(ItemStack::new(&vanilla_items::DIAMOND));
        view.set_item_transform(ItemDisplayContext::OnShelf);
    });
    let mut saved = NbtCompound::new();
    entity.save_additional(&mut saved);
    let restored =
        ItemDisplayEntity::new(&vanilla_entities::ITEM_DISPLAY, 2, DVec3::ZERO, Weak::new());
    reload(&restored, &saved);
    restored.with_view(|view| {
        assert_eq!(view.item_stack_ref().item, &*vanilla_items::DIAMOND);
        assert_eq!(view.item_transform(), ItemDisplayContext::OnShelf);
    });
    assert!(restored.pack_dirty_entity_data().unwrap().iter().any(|value| matches!(&value.value, EntityData::ItemStack(item) if item.item == &*vanilla_items::DIAMOND)));
}

#[test]
fn display_text_round_trip_preserves_independent_fields_and_byte_opacity() {
    init_vanilla_registry();
    let entity =
        TextDisplayEntity::new(&vanilla_entities::TEXT_DISPLAY, 1, DVec3::ZERO, Weak::new());
    entity.with_view(|mut view| {
        view.set_text(TextComponent::plain("display text"));
        view.set_line_width(123);
        view.set_background_color(0x12345678);
        view.set_text_opacity(200);
        view.set_shadow(true);
        view.set_see_through(true);
        view.set_alignment(Alignment::Right);
    });
    let mut saved = NbtCompound::new();
    entity.save_additional(&mut saved);
    assert!(matches!(saved.get("text_opacity"), Some(NbtTag::Byte(-56))));
    let restored =
        TextDisplayEntity::new(&vanilla_entities::TEXT_DISPLAY, 2, DVec3::ZERO, Weak::new());
    reload(&restored, &saved);
    restored.with_view(|view| {
        assert_eq!(view.text_ref(), &TextComponent::plain("display text"));
        assert_eq!(view.line_width(), 123);
        assert_eq!(view.background_color(), 0x12345678);
        assert_eq!(view.text_opacity(), 200);
        assert!(view.shadow());
        assert!(view.see_through());
        assert_eq!(view.alignment(), Alignment::Right);
    });
    assert!(restored.pack_dirty_entity_data().unwrap().iter().any(|value| matches!(&value.value, EntityData::Component(text) if **text == TextComponent::plain("display text"))));
}
