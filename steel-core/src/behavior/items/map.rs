use crate::behavior::ItemBehavior;
use crate::world::World;
use std::sync::Arc;
use steel_macros::item_behavior;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_items;

/// The non-empty map item
#[item_behavior]
pub struct MapItem;

impl MapItem {
    pub(crate) fn create(
        level: &Arc<World>,
        origin_x: i32,
        origin_z: i32,
        scale: i8,
        track_position: bool,
        unlimited_tracking: bool,
    ) -> ItemStack {
        let mut map = ItemStack::new(&vanilla_items::FILLED_MAP);
    }
}

impl ItemBehavior for MapItem {}
