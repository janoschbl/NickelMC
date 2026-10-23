//! Brain AI.

pub mod memory;

/// Initialize the brain registries.
pub fn init_brain_registries() {
    memory::init_memory_module_types();
}

/// Registers extension memories after vanilla, before freezing the global registry.
/// Returns false if another initialization already froze it.
pub fn init_brain_registries_with(
    register: impl FnOnce(&mut memory::MemoryModuleTypeRegistry),
) -> bool {
    memory::init_memory_module_types_with(register)
}
