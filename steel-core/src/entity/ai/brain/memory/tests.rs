use std::ptr;

use steel_utils::DowncastType as _;

use super::{
    Memories, MemoryModuleType, MemoryModuleTypeRegistry, MemoryStatus, PackedMemories,
    RememberedEntities, RememberedEntity,
};
use crate::entity::ai::brain::test_support::{
    Ticks, spawn_pig, test_memory, test_persistent_memory,
};

static SAVED: MemoryModuleType<Ticks> = test_persistent_memory("saved");
static SAVED_PERMANENT: MemoryModuleType<Ticks> = test_persistent_memory("saved_permanent");
static COOLDOWN: MemoryModuleType<Ticks> = test_memory("cooldown");
static DUPLICATE_COOLDOWN: MemoryModuleType<Ticks> = test_memory("cooldown");
static OTHER_COOLDOWN: MemoryModuleType<Ticks> = test_memory("other_cooldown");
static NEARBY: MemoryModuleType<RememberedEntities> = test_memory("nearby");
static TARGET: MemoryModuleType<RememberedEntity> = test_memory("target");

#[test]
fn registered_memory_is_recoverable_by_key() {
    let mut registry = MemoryModuleTypeRegistry::new();
    registry.register(&COOLDOWN);
    registry.register(&OTHER_COOLDOWN);
    registry.freeze();

    let found = registry
        .by_key(COOLDOWN.key())
        .expect("registered memory should resolve by key");
    assert!(ptr::eq(found, COOLDOWN.entry()));
    assert_eq!(found.value_type_key(), Ticks::TYPE_KEY);
    assert!(registry.by_key(NEARBY.key()).is_none());
}

#[test]
#[should_panic(expected = "duplicate memory module type key")]
fn duplicate_memory_keys_are_rejected() {
    let mut registry = MemoryModuleTypeRegistry::new();
    registry.register(&COOLDOWN);
    registry.register(&DUPLICATE_COOLDOWN);
}

#[test]
#[should_panic(expected = "after the registry has been frozen")]
fn frozen_memory_registry_rejects_registration() {
    let mut registry = MemoryModuleTypeRegistry::new();
    registry.freeze();
    registry.register(&COOLDOWN);
}

#[test]
fn unregistered_memory_is_inert() {
    let mut memories = Memories::new();
    memories.set(&COOLDOWN, Ticks(5));

    assert!(!memories.is_registered(COOLDOWN.entry()));
    assert!(!memories.has_value(COOLDOWN.entry()));
    for status in [
        MemoryStatus::ValuePresent,
        MemoryStatus::ValueAbsent,
        MemoryStatus::Registered,
    ] {
        assert!(
            !memories.check(COOLDOWN.entry(), status),
            "unregistered memory must not satisfy {status:?}"
        );
    }
}

#[test]
fn registering_an_existing_memory_keeps_its_value() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    memories.set(&COOLDOWN, Ticks(5));

    memories.register(COOLDOWN.entry());

    assert_eq!(memories.get(&COOLDOWN), Some(&Ticks(5)));
}

#[test]
fn expiring_memory_survives_its_full_time_to_live() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    memories.set_with_expiry(&COOLDOWN, Ticks(5), 2);

    for tick in 1..=2 {
        memories.forget_outdated();
        assert_eq!(
            memories.get(&COOLDOWN),
            Some(&Ticks(5)),
            "memory should still be readable on tick {tick} of its 2 tick life"
        );
    }

    memories.forget_outdated();
    assert_eq!(memories.get(&COOLDOWN), None);
}

#[test]
fn permanent_memory_never_expires() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    memories.set(&COOLDOWN, Ticks(5));

    for _ in 0..64 {
        memories.forget_outdated();
    }

    assert_eq!(memories.get(&COOLDOWN), Some(&Ticks(5)));
}

#[test]
fn expired_memory_is_writable_again() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    memories.set_with_expiry(&COOLDOWN, Ticks(5), 0);
    memories.forget_outdated();
    assert!(!memories.has_value(COOLDOWN.entry()));

    memories.set(&COOLDOWN, Ticks(7));
    for _ in 0..8 {
        memories.forget_outdated();
    }

    assert_eq!(memories.get(&COOLDOWN), Some(&Ticks(7)));
}

#[test]
fn writing_an_empty_collection_clears_the_slot() {
    let entity = spawn_pig();
    let mut memories = Memories::new();
    memories.register(NEARBY.entry());

    memories.set(&NEARBY, [&entity].into_iter().collect());
    assert!(memories.has_value(NEARBY.entry()));

    memories.set(&NEARBY, RememberedEntities::default());
    assert!(!memories.has_value(NEARBY.entry()));
    assert!(memories.check(NEARBY.entry(), MemoryStatus::ValueAbsent));
}

#[test]
fn setting_none_clears_the_slot() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    memories.set(&COOLDOWN, Ticks(5));

    memories.set_optional(&COOLDOWN, None);

    assert!(!memories.has_value(COOLDOWN.entry()));
}

#[test]
fn remembered_entity_does_not_keep_the_entity_alive() {
    let entity = spawn_pig();
    let mut memories = Memories::new();
    memories.register(TARGET.entry());
    memories.set(&TARGET, RememberedEntity::new(&entity));

    assert!(
        memories
            .get(&TARGET)
            .and_then(RememberedEntity::resolve)
            .is_some()
    );

    drop(entity);

    assert!(
        memories
            .get(&TARGET)
            .and_then(RememberedEntity::resolve)
            .is_none(),
        "a memory must not keep a removed entity alive"
    );
}

#[test]
fn remembered_entities_skip_entities_that_left_the_world() {
    let kept = spawn_pig();
    let removed = spawn_pig();
    let mut memories = Memories::new();
    memories.register(NEARBY.entry());
    memories.set(&NEARBY, [&kept, &removed].into_iter().collect());

    drop(removed);

    let remembered = memories.get(&NEARBY).expect("memory should hold a value");
    assert_eq!(remembered.len(), 2);
    let resolved: Vec<_> = remembered.resolve().map(|entity| entity.id()).collect();
    assert_eq!(resolved, vec![kept.id()]);
}

#[test]
#[should_panic(expected = "Unregistered memory fetched")]
fn reading_an_unregistered_memory_panics() {
    let memories = Memories::new();
    let _ = memories.get(&COOLDOWN);
}

#[test]
fn get_internal_tells_unregistered_from_empty() {
    let mut memories = Memories::new();
    assert_eq!(memories.get_internal(&COOLDOWN), None);

    memories.register(COOLDOWN.entry());
    assert_eq!(memories.get_internal(&COOLDOWN), Some(None));

    memories.set(&COOLDOWN, Ticks(5));
    assert_eq!(memories.get_internal(&COOLDOWN), Some(Some(&Ticks(5))));
}

#[test]
fn time_until_expiry_counts_down() {
    let mut memories = Memories::new();
    memories.register(COOLDOWN.entry());
    assert_eq!(memories.time_until_expiry(COOLDOWN.entry()), i64::MAX);

    memories.set_with_expiry(&COOLDOWN, Ticks(5), 10);
    memories.forget_outdated();
    memories.forget_outdated();

    assert_eq!(memories.time_until_expiry(COOLDOWN.entry()), 8);
}

#[test]
fn persistent_memories_round_trip_with_their_remaining_lifetime() {
    let mut registry = MemoryModuleTypeRegistry::new();
    registry.register(&SAVED);
    registry.register(&SAVED_PERMANENT);
    registry.register(&COOLDOWN);

    let mut memories = Memories::new();
    for memory in registry.iter() {
        memories.register(memory);
    }
    memories.set_with_expiry(&SAVED, Ticks(7), 40);
    memories.set(&SAVED_PERMANENT, Ticks(9));
    memories.set(&COOLDOWN, Ticks(1));
    memories.forget_outdated();

    let nbt = memories.pack().to_nbt();
    assert_eq!(nbt.len(), 2, "the non-persistent memory must not be saved");

    let mut restored = Memories::new();
    for memory in registry.iter() {
        restored.register(memory);
    }
    restored.restore(&PackedMemories::from_nbt(&nbt, &registry));

    assert_eq!(restored.get(&SAVED), Some(&Ticks(7)));
    assert_eq!(restored.time_until_expiry(SAVED.entry()), 39);
    assert_eq!(restored.get(&SAVED_PERMANENT), Some(&Ticks(9)));
    assert_eq!(
        restored.time_until_expiry(SAVED_PERMANENT.entry()),
        i64::MAX
    );
    assert_eq!(restored.get(&COOLDOWN), None);
}
