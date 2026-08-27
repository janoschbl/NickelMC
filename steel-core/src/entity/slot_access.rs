//! Mutable access to an entity-owned item slot.

use steel_registry::item_stack::ItemStack;

/// A vanilla entity slot, including slots backed by synchronized item data.
pub trait SlotAccess {
    /// Reads the current stack.
    fn get(&self) -> ItemStack;
    /// Attempts to replace the current stack.
    fn set(&self, item: ItemStack) -> bool;
}
