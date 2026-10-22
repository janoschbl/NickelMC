//! Per-mob memory slots.

use std::fmt::{self, Debug, Formatter};
use std::ptr;

use simdnbt::owned::{NbtCompound, NbtTag};
use smallvec::SmallVec;
use steel_utils::{Downcast as _, DowncastType};

use super::{
    MemoryModuleType, MemoryModuleTypeRef, MemoryModuleTypeRegistry, MemoryStatus, MemoryValue,
};

/// Slots a mob keeps inline before its memories spill to the heap.
const INLINE_SLOTS: usize = 8;

/// `time_to_live` of a memory that never expires.
const NEVER_EXPIRE: i64 = i64::MAX;

/// One memory slot and its remaining lifetime.
struct MemorySlot {
    memory: MemoryModuleTypeRef,
    value: Option<Box<dyn MemoryValue>>,
    time_to_live: i64,
}

impl MemorySlot {
    const fn empty(memory: MemoryModuleTypeRef) -> Self {
        Self {
            memory,
            value: None,
            time_to_live: NEVER_EXPIRE,
        }
    }

    fn set(&mut self, value: Box<dyn MemoryValue>, time_to_live: i64) {
        self.value = Some(value);
        self.time_to_live = time_to_live;
    }

    fn clear(&mut self) {
        self.value = None;
        self.time_to_live = NEVER_EXPIRE;
    }

    const fn has_value(&self) -> bool {
        self.value.is_some()
    }

    const fn time_to_live(&self) -> i64 {
        self.time_to_live
    }

    const fn can_expire(&self) -> bool {
        self.time_to_live != NEVER_EXPIRE
    }

    const fn has_expired(&self) -> bool {
        self.time_to_live <= 0
    }

    /// Ages the slot by one tick.
    fn tick(&mut self) {
        if !self.has_value() || !self.can_expire() {
            return;
        }

        if self.has_expired() {
            self.clear();
        } else {
            self.time_to_live -= 1;
        }
    }
}

/// The memories one mob's brain holds.
///
/// Slots are found by the address of their registry entry. A brain holds few
/// enough that a linear scan beats hashing, and small brains never allocate.
#[derive(Default)]
pub struct Memories {
    slots: SmallVec<[MemorySlot; INLINE_SLOTS]>,
}

/// One saved memory.
#[derive(Debug, Clone)]
pub struct PackedMemory {
    /// The memory the value belongs to.
    pub memory: MemoryModuleTypeRef,
    /// The encoded value.
    pub value: NbtTag,
    /// Remaining lifetime in ticks, or `None` if the memory never expires.
    pub time_to_live: Option<i64>,
}

/// The saved form of a mob's persistent memories.
///
/// Encoded as a compound keyed by memory id, each holding `value` and an
/// optional `ttl`.
#[derive(Debug, Clone, Default)]
pub struct PackedMemories(Vec<PackedMemory>);

impl PackedMemories {
    /// The saved memories.
    #[must_use]
    pub fn memories(&self) -> &[PackedMemory] {
        &self.0
    }

    /// Encodes the saved memories.
    #[must_use]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut compound = NbtCompound::new();
        for packed in &self.0 {
            let mut entry = NbtCompound::new();
            entry.insert("value", packed.value.clone());
            if let Some(time_to_live) = packed.time_to_live {
                entry.insert("ttl", time_to_live);
            }
            compound.insert(packed.memory.key.to_string(), entry);
        }
        compound
    }

    /// Decodes saved memories, skipping entries whose memory is unknown or not
    /// persistent, or whose value is malformed.
    #[must_use]
    pub fn from_nbt(compound: &NbtCompound, registry: &MemoryModuleTypeRegistry) -> Self {
        let memories = compound
            .iter()
            .filter_map(|(key, tag)| {
                let memory = registry.by_key(&key.to_str().parse().ok()?)?;
                let entry = tag.compound()?;
                memory.can_serialize().then_some(PackedMemory {
                    memory,
                    value: entry.get("value")?.clone(),
                    time_to_live: entry.long("ttl"),
                })
            })
            .collect();
        Self(memories)
    }
}

impl Memories {
    /// Creates a brain with no memories registered.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn slot(&self, memory: MemoryModuleTypeRef) -> Option<&MemorySlot> {
        self.slots.iter().find(|slot| ptr::eq(slot.memory, memory))
    }

    fn slot_mut(&mut self, memory: MemoryModuleTypeRef) -> Option<&mut MemorySlot> {
        self.slots
            .iter_mut()
            .find(|slot| ptr::eq(slot.memory, memory))
    }

    /// Registers `memory`, leaving an already-registered slot untouched.
    pub fn register(&mut self, memory: MemoryModuleTypeRef) {
        if self.slot(memory).is_none() {
            self.slots.push(MemorySlot::empty(memory));
        }
    }

    /// Returns whether `memory` is registered on this mob.
    #[must_use]
    pub fn is_registered(&self, memory: MemoryModuleTypeRef) -> bool {
        self.slot(memory).is_some()
    }

    /// Returns whether `memory` satisfies `status`.
    #[must_use]
    pub fn check(&self, memory: MemoryModuleTypeRef, status: MemoryStatus) -> bool {
        let Some(slot) = self.slot(memory) else {
            return false;
        };

        match status {
            MemoryStatus::Registered => true,
            MemoryStatus::ValuePresent => slot.has_value(),
            MemoryStatus::ValueAbsent => !slot.has_value(),
        }
    }

    /// Returns whether `memory` currently holds a value.
    #[must_use]
    pub fn has_value(&self, memory: MemoryModuleTypeRef) -> bool {
        self.check(memory, MemoryStatus::ValuePresent)
    }

    /// Returns the value held by `memory`.
    ///
    /// # Panics
    ///
    /// Panics if `memory` is not registered on this mob. Use
    /// [`get_internal`](Self::get_internal) when it may not be.
    #[must_use]
    pub fn get<V: MemoryValue + DowncastType>(
        &self,
        memory: &'static MemoryModuleType<V>,
    ) -> Option<&V> {
        let Some(value) = self.get_internal(memory) else {
            panic!("Unregistered memory fetched: {}", memory.key());
        };
        value
    }

    /// Returns the value held by `memory`, or `None` if it is not registered.
    #[must_use]
    #[expect(
        clippy::option_option,
        reason = "the outer option tells an unregistered memory from an empty one, like vanilla getMemoryInternal"
    )]
    pub fn get_internal<V: MemoryValue + DowncastType>(
        &self,
        memory: &'static MemoryModuleType<V>,
    ) -> Option<Option<&V>> {
        let slot = self.slot(memory.entry())?;
        Some(
            slot.value
                .as_deref()
                .and_then(|value| value.downcast_ref::<V>()),
        )
    }

    /// Returns the ticks until `memory` expires, `i64::MAX` if it never does.
    ///
    /// # Panics
    ///
    /// Panics if `memory` is not registered on this mob.
    #[must_use]
    pub fn time_until_expiry(&self, memory: MemoryModuleTypeRef) -> i64 {
        let Some(slot) = self.slot(memory) else {
            panic!("Unregistered memory fetched: {}", memory.key);
        };
        slot.time_to_live()
    }

    /// Stores `value` in `memory` permanently.
    pub fn set<V: MemoryValue + DowncastType>(
        &mut self,
        memory: &'static MemoryModuleType<V>,
        value: V,
    ) {
        self.set_internal(memory.entry(), Some(Box::new(value)), NEVER_EXPIRE);
    }

    /// Stores `value` in `memory` for `time_to_live` further ticks.
    pub fn set_with_expiry<V: MemoryValue + DowncastType>(
        &mut self,
        memory: &'static MemoryModuleType<V>,
        value: V,
        time_to_live: i64,
    ) {
        self.set_internal(memory.entry(), Some(Box::new(value)), time_to_live);
    }

    /// Stores `value` in `memory`, clearing the slot when it is `None`.
    pub fn set_optional<V: MemoryValue + DowncastType>(
        &mut self,
        memory: &'static MemoryModuleType<V>,
        value: Option<V>,
    ) {
        let value = value.map(|value| Box::new(value) as Box<dyn MemoryValue>);
        self.set_internal(memory.entry(), value, NEVER_EXPIRE);
    }

    /// Clears `memory`.
    pub fn erase(&mut self, memory: MemoryModuleTypeRef) {
        if let Some(slot) = self.slot_mut(memory) {
            slot.clear();
        }
    }

    /// Clears every memory.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.clear();
        }
    }

    /// Ages every memory by one tick, clearing those that have expired.
    pub fn forget_outdated(&mut self) {
        for slot in &mut self.slots {
            slot.tick();
        }
    }

    /// Collects the persistent memories that hold a value.
    #[must_use]
    pub fn pack(&self) -> PackedMemories {
        let memories = self
            .slots
            .iter()
            .filter_map(|slot| {
                let value = slot.memory.encode(slot.value.as_deref()?)?;
                Some(PackedMemory {
                    memory: slot.memory,
                    value,
                    time_to_live: slot.can_expire().then_some(slot.time_to_live()),
                })
            })
            .collect();
        PackedMemories(memories)
    }

    /// Restores saved memories into the slots registered on this mob.
    pub fn restore(&mut self, packed: &PackedMemories) {
        for saved in &packed.0 {
            let Some(value) = saved.memory.decode(&saved.value) else {
                continue;
            };
            let time_to_live = saved.time_to_live.unwrap_or(NEVER_EXPIRE);
            self.set_internal(saved.memory, Some(value), time_to_live);
        }
    }

    /// Writes a value into a registered slot, applying vanilla's two write rules.
    fn set_internal(
        &mut self,
        memory: MemoryModuleTypeRef,
        value: Option<Box<dyn MemoryValue>>,
        time_to_live: i64,
    ) {
        let Some(slot) = self.slot_mut(memory) else {
            return;
        };

        match value.filter(|value| !value.is_empty_collection()) {
            Some(value) => slot.set(value, time_to_live),
            None => slot.clear(),
        }
    }
}

impl Debug for Memories {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let mut map = formatter.debug_map();
        for slot in &self.slots {
            map.entry(&slot.memory.key, &slot.value);
        }
        map.finish()
    }
}
