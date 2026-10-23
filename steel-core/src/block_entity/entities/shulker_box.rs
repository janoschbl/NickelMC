use std::sync::{
    Arc, Weak,
    atomic::{AtomicI32, Ordering},
};

use glam::DVec3;
use simdnbt::{
    ToNbtTag,
    borrow::{BaseNbtCompound, NbtCompound as NbtCompoundView},
    owned::{NbtCompound, NbtList, NbtTag},
};
use steel_registry::{
    ItemStackTemplate, REGISTRY,
    blocks::{
        behavior::PushReaction, block_state_ext::BlockStateExt, properties::BlockStateProperties,
    },
    data_components::{
        DataComponentMap, DataComponentPatch, ItemContainerContents, vanilla_components::CONTAINER,
    },
    item_stack::ItemStack,
    vanilla_block_entity_types,
};
use steel_utils::{
    BlockLocalAabb, BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, WorldAabb,
    geometry::{Aabb, Space},
    locks::SyncMutex,
    types::UpdateFlags,
};

use crate::{
    behavior::blocks::ShulkerBoxBlock,
    block_entity::{
        BlockEntity, BlockEntityBase, BlockEntityLifecycleExt, ImplicitComponentGetter,
        RandomizableContainer, container_properties::ContainerProperties,
        randomizable_container::RandomizableContainerStorage,
    },
    inventory::{container::Container, lock::ContainerRef},
    physics::{CollisionWorld as _, MoverType, WorldCollisionProvider},
    world::World,
};

/// Number of slots in a shulker box (3 rows of 9).
pub const SHULKER_BOX_SLOTS: usize = 27;
const ANIMATION_STEPS: u8 = 10;

/// The current animation state of a shulker box.
#[derive(Debug, Clone, Copy)]
pub enum AnimationStatus {
    /// Fully closed.
    Closed,
    /// Opening.
    Opening,
    /// Fully open.
    Opened,
    /// Closing.
    Closing,
}

pub struct ShulkerBoxAnimation {
    animation_status: AnimationStatus,
    progress: u8,
    old_progress: u8,
}

impl ShulkerBoxAnimation {
    fn progress(&self) -> f32 {
        f32::from(self.progress) / f32::from(ANIMATION_STEPS)
    }

    fn old_progress(&self) -> f32 {
        f32::from(self.old_progress) / f32::from(ANIMATION_STEPS)
    }
}

/// Block entity backing every shulker box color, including the undyed one.
pub struct ShulkerBoxBlockEntity {
    storage: RandomizableContainerStorage,
    properties: ContainerProperties,
    animation: SyncMutex<ShulkerBoxAnimation>,
    open_count: AtomicI32,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ShulkerBoxEntity`.
unsafe impl DowncastType for ShulkerBoxBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/shulker_box");
}

fn do_neighbor_updates(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
    world.update_neighbour_shapes(state, pos, UpdateFlags::UPDATE_ALL, 512);
    world.update_neighbors_at(pos, state.get_block());
}

impl ShulkerBoxBlockEntity {
    /// Creates a new shulker box block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            storage: RandomizableContainerStorage::new(
                &vanilla_block_entity_types::SHULKER_BOX,
                SHULKER_BOX_SLOTS,
                level,
                pos,
                state,
            ),
            properties: ContainerProperties::default(),
            animation: SyncMutex::new(ShulkerBoxAnimation {
                animation_status: AnimationStatus::Closed,
                progress: 0,
                old_progress: 0,
            }),
            open_count: AtomicI32::new(0),
        }
    }

    fn update_animation(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        let mut animation = self.animation.lock();
        animation.old_progress = animation.progress;
        match animation.animation_status {
            AnimationStatus::Closed => animation.progress = 0,
            AnimationStatus::Opening => {
                animation.progress += 1;
                if animation.old_progress == 0 {
                    do_neighbor_updates(world, pos, state);
                }

                if animation.progress >= ANIMATION_STEPS {
                    animation.animation_status = AnimationStatus::Opened;
                    animation.progress = ANIMATION_STEPS;
                    do_neighbor_updates(world, pos, state);
                }

                drop(animation);
                self.move_collided_entities(world, pos, state);
            }
            AnimationStatus::Opened => animation.progress = ANIMATION_STEPS,
            AnimationStatus::Closing => {
                animation.progress = animation.progress.saturating_sub(1);
                if animation.old_progress == ANIMATION_STEPS {
                    do_neighbor_updates(world, pos, state);
                }

                if animation.progress == 0 {
                    animation.animation_status = AnimationStatus::Closed;
                    animation.progress = 0;
                    do_neighbor_updates(world, pos, state);
                }
            }
        }
    }

    fn move_collided_entities(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        let direction = state.get_value(&BlockStateProperties::FACING);

        let aabb: WorldAabb = {
            let animation = self.animation.lock();
            Self::get_progress_delta_aabb(
                1.0,
                direction,
                animation.old_progress(),
                animation.progress(),
                DVec3::from(pos.get_bottom_center()),
            )
        };

        let entities = world.get_entities_in_aabb(&aabb);
        for entity in entities {
            if entity.piston_push_reaction() == PushReaction::IgnoreEntity {
                continue;
            }
            let (offset_x, offset_y, offset_z) = direction.offset();
            entity.move_entity(
                MoverType::ShulkerBox,
                DVec3::new(
                    (aabb.width() + 0.01) * f64::from(offset_x),
                    (aabb.height() + 0.01) * f64::from(offset_y),
                    (aabb.depth() + 0.01) * f64::from(offset_z),
                ),
            );
        }
    }

    /// Builds a shulker box item containing the stored inventory.
    pub fn shulker_box_as_item(&self, state: BlockStateId) -> std::io::Result<ItemStack> {
        let block_item = REGISTRY.items.by_block(state.get_block());
        let mut patch = DataComponentPatch::new();
        for (key, value) in self.collect_components()?.iter() {
            patch.set_raw(key.clone(), value.clone());
        }
        Ok(ItemStack::with_count_and_patch(block_item, 1, patch))
    }

    /// Calculates the bounding box of a shulker during its opening animation.
    ///
    /// The bounds are expanded in the opening direction based on the animation progress.
    #[must_use]
    pub fn get_progress_delta_aabb<I: Space>(
        size: f32,
        direction: Direction,
        progress_from: f32,
        progress_to: f32,
        position: DVec3,
    ) -> Aabb<DVec3, I> {
        let size = f64::from(size);
        let bounds =
            Aabb::<DVec3, I>::new(-size * 0.5, 0.0, -size * 0.5, size * 0.5, size, size * 0.5);

        let max_movement = f64::from(progress_from.max(progress_to));
        let min_movement = f64::from(progress_from.min(progress_to));

        let dir = DVec3::from(direction.offset_vec());

        bounds
            .expand_towards(dir * max_movement * size)
            .contract(-dir * (1.0 + min_movement) * size)
            .translate(position)
    }

    /// Block-local bounds of the box including the lid at its current progress.
    #[must_use]
    pub fn get_bounding_box(&self, state: BlockStateId) -> BlockLocalAabb {
        let bottom_center = DVec3::new(0.5, 0.0, 0.5);
        Self::get_progress_delta_aabb(
            1.0,
            state.get_value(ShulkerBoxBlock::FACING),
            -1.0,
            0.5 * self.progress(1.0),
            bottom_center,
        )
    }

    /// Returns whether the shulker box can open
    #[must_use]
    pub fn can_open(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) -> bool {
        if !matches!(self.animation_status(), AnimationStatus::Closed) {
            return true;
        }

        let direction = state.get_value(ShulkerBoxBlock::FACING);

        let lid_open_bounding_box: WorldAabb = Self::get_progress_delta_aabb(
            1.0,
            direction,
            0.0,
            0.5,
            DVec3::from(pos.get_bottom_center()),
        )
        .deflate(1.0E-6);

        let collision = WorldCollisionProvider::new(world);
        !collision.has_block_collision(&lid_open_bounding_box)
    }

    /// Checks the lock and the spectator restriction on pending loot.
    pub fn can_player_open(&self, player: &crate::player::Player) -> bool {
        use crate::entity::Entity as _;
        !(player.is_spectator() && self.storage.loot_table().is_some())
            && self.properties.can_open(player)
    }

    /// Checks if the block entity's container has any items
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage.container.lock().is_empty()
    }

    /// Interpolated progress for smooth rendering between ticks.
    #[must_use]
    pub fn progress(&self, partial_tick: f32) -> f32 {
        let animation = self.animation.lock();
        let new = animation.progress();
        let old = animation.old_progress();
        old + partial_tick * (new - old)
    }

    /// Get the current animation state
    #[must_use]
    pub fn animation_status(&self) -> AnimationStatus {
        self.animation.lock().animation_status
    }
}

impl crate::nameable::Nameable for ShulkerBoxBlockEntity {
    fn custom_name(&self) -> Option<text_components::TextComponent> {
        self.properties.custom_name()
    }

    fn name(&self) -> text_components::TextComponent {
        self.custom_name().unwrap_or_else(|| {
            text_components::TextComponent::translated(
                steel_utils::translations::CONTAINER_SHULKER_BOX.msg(),
            )
        })
    }
}

impl BlockEntity for ShulkerBoxBlockEntity {
    fn collect_implicit_components(
        &self,
        components: &mut steel_registry::data_components::DataComponentMap,
    ) -> std::io::Result<()> {
        self.properties.collect_components(components);
        self.storage.collect_components(components)
    }

    fn base(&self) -> &BlockEntityBase {
        &self.storage.base
    }

    fn tick(&self, world: &Arc<World>) {
        self.update_animation(
            world,
            self.storage.base.pos,
            self.storage.base.block_state(),
        );
    }

    fn trigger_event(&self, kind: i32, data: i32) -> bool {
        if kind == 1 {
            self.open_count.store(data, Ordering::Relaxed);
            if data == 0 {
                self.animation.lock().animation_status = AnimationStatus::Closing;
            }

            if data == 1 {
                self.animation.lock().animation_status = AnimationStatus::Opening;
            }

            true
        } else {
            false
        }
    }

    fn load_additional(&self, nbt: &BaseNbtCompound<'_>) {
        self.storage.load_additional(nbt);
        self.properties.load(nbt);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.storage.save_additional(nbt);
        self.properties.save(nbt);
    }

    fn apply_implicit_components(&self, components: &mut ImplicitComponentGetter<'_>) {
        self.storage.apply_components(components);
        self.properties.apply_components(components);
    }

    fn as_randomizable_container(&self) -> Option<&dyn RandomizableContainer> {
        Some(&self.storage)
    }

    fn as_nameable(&self) -> Option<&dyn crate::nameable::Nameable> {
        Some(self)
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.storage.container_ref.clone())
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_items};

    use super::*;
    use crate::test_support::fresh_test_world;

    #[test]
    fn applying_item_components_fills_the_slots_and_collects_them_back() {
        init_vanilla_registry();
        let world_fixture = fresh_test_world("shulker_box_apply_components");
        let world = &world_fixture.world;
        let shulker = ShulkerBoxBlockEntity::new(
            Arc::downgrade(world),
            BlockPos::new(8, 64, 8),
            vanilla_blocks::SHULKER_BOX.default_state(),
        );
        let diamonds =
            ItemStack::with_count_and_patch(&vanilla_items::DIAMOND, 5, DataComponentPatch::new());
        let mut slots = vec![None; SHULKER_BOX_SLOTS];
        slots[3] = Some(ItemStackTemplate::from_stack(&diamonds).expect("a plain stack converts"));
        let contents = ItemContainerContents::new(slots).expect("27 slots fit the container limit");
        let mut stack = ItemStack::new(&vanilla_items::SHULKER_BOX);
        stack.set(CONTAINER, contents.clone());

        shulker.apply_components_from_item_stack(&stack);

        {
            let container = shulker.storage.container.lock();
            assert!(container.items().get(3).is(&vanilla_items::DIAMOND));
            assert_eq!(container.items().get(3).count(), 5);
            assert!(
                container
                    .items()
                    .iter()
                    .enumerate()
                    .all(|(slot, item)| slot == 3 || item.is_empty())
            );
        }
        assert!(
            !shulker.base().stored_components().has(CONTAINER),
            "an implicit component must not also be stored"
        );
        assert_eq!(
            shulker
                .collect_components()
                .expect("valid components")
                .get(CONTAINER),
            Some(contents)
        );
    }
}
