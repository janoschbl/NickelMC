use crate::behavior::blocks::{SkullData, SkullType};
use crate::behavior::skull_types;
use crate::block_entity::{BlockEntity, BlockEntityBase, ImplicitComponentGetter};
use crate::world::World;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag, ToNbtTag};
use std::sync::Weak;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::data_components::vanilla_components::{CUSTOM_NAME, NOTE_BLOCK_SOUND, PROFILE};
use steel_registry::data_components::{DataComponentMap, DataComponentPatch};
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, ResolvableProfile, vanilla_block_entity_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};
use text_components::TextComponent;

const PROFILE_NBT_KEY: &str = "profile";
const NOTE_BLOCK_SOUND_NBT_KEY: &str = "note_block_sound";
const CUSTOM_NAME_NBT_KEY: &str = "custom_name";

/// Skull block entity.
///
/// Stores player profile, note block sound and custom name
pub struct SkullBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<SkullData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkullBlockEntity`.
unsafe impl DowncastType for SkullBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/skull");
}

impl SkullBlockEntity {
    /// Creates a Skull block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::SKULL, level, pos, state),
            state: SyncMutex::new(SkullData::default()),
        }
    }

    /// Used to get the sound identifier by noteblocks.
    pub fn get_note_block_sound(&self) -> Option<Identifier> {
        self.state.lock().note_block_sound.clone()
    }

    /// Returns a clone of the stored skull data
    #[must_use]
    pub fn data(&self) -> SkullData {
        self.state.lock().clone()
    }

    /// Replaces the stored skull data and sends it to clients
    pub fn set_data(&self, data: SkullData) {
        {
            let mut state = self.state.lock();
            if *state == data {
                return;
            }
            *state = data;
        }
        self.set_changed();
        if let Some(world) = self.get_level() {
            world.send_block_updated(self.base.pos);
        }
    }

    /// Builds the picked item form of a placed skull with all stored components.
    pub fn skull_as_picked_item(&self, state: BlockStateId) -> ItemStack {
        let patch = self.state.lock().component_patch();
        Self::skull_as_item(state, patch)
    }

    /// Builds the dropped item form of a placed skull. Only player heads keep their profile and
    /// note block sound.
    pub fn skull_as_dropped_item(&self, state: BlockStateId) -> ItemStack {
        let is_player_head = SkullType::from_block(state.get_block()) == Some(&skull_types::PLAYER);
        let data = self.state.lock();
        let patch = if is_player_head {
            data.component_patch()
        } else {
            data.custom_name_patch()
        };
        drop(data);
        Self::skull_as_item(state, patch)
    }

    fn skull_as_item(state: BlockStateId, patch: DataComponentPatch) -> ItemStack {
        let block_item = REGISTRY.items.by_block(state.get_block());
        ItemStack::with_count_and_patch(block_item, 1, patch)
    }
}

impl BlockEntity for SkullBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();

        state.owner =
            ResolvableProfile::from_optional_nbt_tag(nbt.get(PROFILE_NBT_KEY)).unwrap_or_default();
        state.note_block_sound =
            Identifier::from_optional_nbt_tag(nbt.get(NOTE_BLOCK_SOUND_NBT_KEY))
                .unwrap_or_default();
        state.custom_name =
            TextComponent::from_optional_nbt_tag(nbt.get(CUSTOM_NAME_NBT_KEY)).unwrap_or_default();
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();

        if let Some(profile) = state.owner.clone().to_optional_nbt_tag() {
            nbt.insert(PROFILE_NBT_KEY, profile);
        }

        if let Some(sound) = state.note_block_sound.clone().to_optional_nbt_tag() {
            nbt.insert(NOTE_BLOCK_SOUND_NBT_KEY, sound);
        }

        if let Some(name) = state.custom_name.clone().to_optional_nbt_tag() {
            nbt.insert(CUSTOM_NAME_NBT_KEY, name);
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        Some(self.save_custom_only())
    }

    fn apply_implicit_components(&self, components: &mut ImplicitComponentGetter<'_>) {
        let mut state = self.state.lock();
        state.owner = components.get(PROFILE);
        state.note_block_sound = components.get(NOTE_BLOCK_SOUND);
        state.custom_name = components.get(CUSTOM_NAME);
    }

    fn collect_implicit_components(&self, components: &mut DataComponentMap) {
        let state = self.state.lock();
        components.set(PROFILE, state.owner.clone());
        components.set(NOTE_BLOCK_SOUND, state.note_block_sound.clone());
        components.set(CUSTOM_NAME, state.custom_name.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::data_components::vanilla_components::REPAIR_COST;
    use steel_registry::{init_vanilla_registry, sound_events, vanilla_blocks, vanilla_items};

    #[test]
    fn implicit_skull_components_are_collected_not_duplicated_and_clear_on_replacement() {
        init_vanilla_registry();
        let skull = SkullBlockEntity::new(
            Weak::new(),
            BlockPos::new(0, 64, 0),
            vanilla_blocks::PLAYER_HEAD.default_state(),
        );
        let mut stack = ItemStack::new(&vanilla_items::PLAYER_HEAD);
        let name = TextComponent::plain("Named Head");
        let sound = sound_events::BLOCK_NOTE_BLOCK_BANJO.key.clone();
        stack.set(CUSTOM_NAME, name.clone());
        stack.set(NOTE_BLOCK_SOUND, sound.clone());
        stack.set(REPAIR_COST, 7);

        skull.apply_components_from_item_stack(&stack);

        let stored = skull.base().stored_components();
        assert_eq!(stored.get(REPAIR_COST), Some(7));
        assert!(!stored.has(CUSTOM_NAME));
        assert!(!stored.has(NOTE_BLOCK_SOUND));
        let collected = skull.collect_components();
        assert_eq!(collected.get(CUSTOM_NAME), Some(name));
        assert_eq!(collected.get(NOTE_BLOCK_SOUND), Some(sound));

        skull.apply_components_from_item_stack(&ItemStack::new(&vanilla_items::PLAYER_HEAD));
        assert_eq!(skull.data().custom_name, None);
        assert_eq!(skull.get_note_block_sound(), None);
        assert!(skull.base().stored_components().is_empty());
    }
}
