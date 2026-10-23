//! Vanilla block-state NBT codec shape used by block entities.

use std::str::FromStr;

use simdnbt::borrow::NbtTag as BorrowedNbtTag;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::{BlockStateId, Identifier};

#[must_use]
pub(crate) fn save(state: BlockStateId) -> NbtTag {
    if state == state.get_block().default_state() {
        return NbtTag::String(state.get_block().key.to_string().into());
    }
    let mut encoded = NbtCompound::new();
    encoded.insert("id", state.get_block().key.to_string());

    let state_properties = REGISTRY.blocks.properties(state);
    if state_properties.len() != 0 {
        let mut properties = NbtCompound::new();
        for (name, value) in state_properties {
            properties.insert(name, value);
        }
        encoded.insert("properties", properties);
    }

    NbtTag::Compound(encoded)
}

#[must_use]
pub(crate) fn load(encoded: BorrowedNbtTag<'_, '_>) -> Option<BlockStateId> {
    let name = encoded
        .string()
        .or_else(|| encoded.compound()?.string("id"))?;
    let identifier = Identifier::from_str(name.to_str().as_ref()).ok()?;
    let block = REGISTRY.blocks.by_key(&identifier)?;

    let mut state = block.default_state();
    if let Some(encoded_properties) = encoded
        .compound()
        .and_then(|value| value.compound("properties"))
    {
        for (name, value) in encoded_properties.iter() {
            let Some(value) = value.string() else {
                continue;
            };
            if let Some(updated) = REGISTRY.blocks.try_set_property_by_name(
                state,
                name.to_str().as_ref(),
                value.to_str().as_ref(),
            ) {
                state = updated;
            }
        }
    }

    Some(state)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use simdnbt::owned::NbtTag;
    use steel_registry::blocks::properties::{BlockStateProperties, Direction, PistonType};
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_blocks;

    use super::*;

    #[test]
    fn propertyful_block_state_round_trips_vanilla_nbt_shape() {
        init_vanilla_registry();
        let state = vanilla_blocks::PISTON_HEAD
            .default_state()
            .set_value(&BlockStateProperties::FACING, Direction::East)
            .set_value(&BlockStateProperties::PISTON_TYPE, PistonType::Sticky)
            .set_value(&BlockStateProperties::SHORT, true);

        let mut root = NbtCompound::new();
        root.insert("state", save(state));
        let encoded = root
            .compound("state")
            .expect("nondefault state is a compound");
        assert_eq!(
            encoded.string("id").map(|name| name.to_str()),
            Some("minecraft:piston_head".into())
        );
        assert!(matches!(
            encoded.get("properties"),
            Some(NbtTag::Compound(_))
        ));

        let mut bytes = Vec::new();
        root.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test NBT should reborrow");
        let view = simdnbt::borrow::NbtCompound::from(&borrowed);
        assert_eq!(load(view.get("state").unwrap()), Some(state));
    }

    #[test]
    fn default_states_use_string_form_including_propertyful_blocks() {
        init_vanilla_registry();
        for block in [&vanilla_blocks::STONE, &vanilla_blocks::PISTON] {
            let mut root = NbtCompound::new();
            root.insert("state", save(block.default_state()));
            assert_eq!(
                root.string("state").unwrap().to_str(),
                block.key.to_string()
            );
            let mut bytes = Vec::new();
            root.write(&mut bytes);
            let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).unwrap();
            let view = simdnbt::borrow::NbtCompound::from(&borrowed);
            assert_eq!(
                load(view.get("state").unwrap()),
                Some(block.default_state())
            );
        }
    }
}
