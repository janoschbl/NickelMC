//! Brain memory

mod damage;
mod map;
mod module_type;
mod value;
mod vanilla_values;
mod visible;

pub mod vanilla_memories {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/entity/generated/memories.rs"
    ));
}

#[cfg(test)]
mod tests;

pub use damage::RememberedDamage;
pub use map::{Memories, PackedMemories, PackedMemory};
pub use module_type::{
    MEMORY_MODULE_TYPES, MemoryModuleType, MemoryModuleTypeEntry, MemoryModuleTypeRef,
    MemoryModuleTypeRegistry, MemoryModuleTypeRegistryLock,
};
pub use value::{MemoryValue, PersistentMemoryValue, RememberedEntities, RememberedEntity};
pub use vanilla_values::*;
pub use visible::NearestVisibleLivingEntities;

pub(super) use module_type::init_memory_module_types;
pub(super) use module_type::init_memory_module_types_with;

/// The state an entry condition requires a memory slot to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    /// The slot must hold a value.
    ValuePresent,
    /// The slot must be registered and empty.
    ValueAbsent,
    /// The slot must be registered; its contents do not matter.
    Registered,
}
