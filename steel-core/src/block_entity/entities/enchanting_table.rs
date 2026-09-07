//! Enchanting table block entity.
//!
//! Server-side it only carries the optional custom name used as the menu
//! title. Vanilla's book animation (`bookAnimationTick`) is client-only, so
//! there is no ticker.

use std::sync::{Arc, Weak};

use simdnbt::borrow::{
    BaseNbtCompound as BorrowedNbtCompound, NbtCompound as BorrowedNbtCompoundView,
};
use simdnbt::owned::NbtCompound;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{
    BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex, translations,
};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Enchanting table block entity: holds the optional custom name.
pub struct EnchantingTableBlockEntity {
    base: Arc<BlockEntityBase>,
    name: SyncMutex<Option<TextComponent>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EnchantingTableBlockEntity`.
unsafe impl DowncastType for EnchantingTableBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/enchanting_table");
}

impl EnchantingTableBlockEntity {
    /// Creates a new enchanting table block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: Arc::new(BlockEntityBase::new(
                &vanilla_block_entity_types::ENCHANTING_TABLE,
                level,
                pos,
                state,
            )),
            name: SyncMutex::new(None),
        }
    }

    /// Vanilla `getName`: the custom name, or `container.enchant`.
    #[must_use]
    pub fn display_name(&self) -> TextComponent {
        self.name
            .lock()
            .clone()
            .unwrap_or_else(|| TextComponent::translated(translations::CONTAINER_ENCHANT.msg()))
    }

    /// Vanilla `getCustomName`.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.name.lock().clone()
    }

    /// Vanilla `setCustomName`. Callers persist the change with `set_changed`.
    pub fn set_custom_name(&self, name: Option<TextComponent>) {
        *self.name.lock() = name;
    }
}

impl BlockEntity for EnchantingTableBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: BorrowedNbtCompoundView<'_, '_> = nbt.into();
        // Vanilla `parseCustomNameSafe` drops unreadable names instead of failing the load.
        *self.name.lock() = nbt_view
            .get("CustomName")
            .and_then(|tag| TextComponent::from_nbt(&tag.to_owned()));
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(name) = self.name.lock().as_ref() {
            nbt.insert("CustomName", name.to_codec_nbt());
        }
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::translations;

    use super::*;
    use crate::test_support::fresh_test_world;

    fn table(world: &Arc<World>) -> EnchantingTableBlockEntity {
        EnchantingTableBlockEntity::new(
            Arc::downgrade(world),
            BlockPos::new(8, 64, 8),
            vanilla_blocks::ENCHANTING_TABLE.default_state(),
        )
    }

    #[test]
    fn display_name_falls_back_to_the_container_translation() {
        init_vanilla_registry();
        let world = fresh_test_world("enchanting_table_default_name");
        let table = table(&world);
        assert_eq!(table.custom_name(), None);
        assert_eq!(
            table.display_name(),
            TextComponent::translated(translations::CONTAINER_ENCHANT.msg())
        );
    }

    #[test]
    fn custom_name_round_trips_through_nbt() {
        init_vanilla_registry();
        let world = fresh_test_world("enchanting_table_name_round_trip");
        let named = table(&world);
        let name = TextComponent::from("Arcane Desk".to_string());
        named.set_custom_name(Some(name.clone()));
        let saved = named.save_custom_only();

        let restored = table(&world);
        restored
            .load_with_owned_components(&saved)
            .expect("saved table NBT should load");
        assert_eq!(restored.custom_name(), Some(name.clone()));
        assert_eq!(restored.display_name(), name);

        let unnamed = table(&world);
        unnamed
            .load_with_owned_components(&table(&world).save_custom_only())
            .expect("empty table NBT should load");
        assert_eq!(unnamed.custom_name(), None);
    }
}
