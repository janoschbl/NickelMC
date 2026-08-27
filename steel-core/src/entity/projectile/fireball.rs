//! Item-bearing hurting projectiles.

use simdnbt::{ToNbtTag, borrow::NbtCompound as BorrowedNbtCompoundView, owned::NbtCompound};
use steel_registry::{item_stack::ItemStack, vanilla_items};

use super::AbstractHurtingProjectile;

struct FireballSlot<'a, T: ?Sized>(&'a T);

impl<T: Fireball + ?Sized> crate::entity::SlotAccess for FireballSlot<'_, T> {
    fn get(&self) -> ItemStack {
        self.0.get_item()
    }
    fn set(&self, item: ItemStack) -> bool {
        self.0.set_item(item);
        true
    }
}

/// Shared rendered-item and persistence behavior for vanilla fireballs.
pub trait Fireball: AbstractHurtingProjectile {
    /// Exposes the rendered item through vanilla's entity slot zero.
    fn fireball_slot(&self, slot: i32) -> Option<Box<dyn crate::entity::SlotAccess + '_>> {
        (slot == 0).then(|| Box::new(FireballSlot(self)) as Box<dyn crate::entity::SlotAccess>)
    }

    /// Returns the rendered stack.
    fn get_item(&self) -> ItemStack;

    /// Stores one item, falling back to a fire charge for an empty stack.
    fn set_item(&self, item: ItemStack);

    /// Saves the rendered item and inherited projectile state.
    fn save_fireball(&self, nbt: &mut NbtCompound) {
        self.save_hurting_projectile(nbt);
        nbt.insert("Item", self.get_item().to_nbt_tag());
    }

    /// Restores the rendered item, retaining vanilla's fire-charge fallback.
    fn load_fireball(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_hurting_projectile(nbt);
        self.set_item(
            nbt.compound("Item")
                .and_then(|item| ItemStack::from_borrowed_compound(&item))
                .unwrap_or_else(|| ItemStack::new(&vanilla_items::FIRE_CHARGE)),
        );
    }
}
