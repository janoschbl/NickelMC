//! Item slots exposed by entities to commands.

use steel_registry::item_stack::ItemStack;

/// A readable slot whose setter reports whether the replacement was accepted.
pub trait SlotAccess {
    /// Returns the current stack, including an empty stack for an empty slot.
    fn get(&self) -> ItemStack;

    /// Replaces the stack when permitted by this slot.
    fn set(&self, stack: ItemStack) -> bool;
}
