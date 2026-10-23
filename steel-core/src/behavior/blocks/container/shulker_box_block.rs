use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::{
    block_entity_type::BlockEntityTypeRef,
    blocks::{
        BlockRef,
        block_state_ext::BlockStateExt,
        properties::{BlockStateProperties, EnumProperty},
        shapes::VoxelShape,
    },
    item_stack::ItemStack,
    items::item::BlockHitResult,
    vanilla_block_entity_types, vanilla_custom_stats,
};
use steel_utils::{BlockLocalAabb, BlockPos, BlockStateId, Direction, Downcast, translations};
use text_components::TextComponent;

use crate::{
    behavior::{
        BlockBehavior, BlockCollisionBoxes, BlockCollisionContext, BlockEntityCreation,
        BlockPlaceContext, InteractionResult, InventoryAccess,
    },
    block_entity::{
        BLOCK_ENTITIES, BlockEntity, BlockEntityTicker,
        entities::{AnimationStatus, ShulkerBoxBlockEntity},
    },
    inventory::{
        container::calculate_redstone_signal_from_containers,
        lock::{ContainerLockGuard, ContainerRef},
        menu::kinds::shulker_box,
    },
    nameable::Nameable as _,
    player::Player,
    world::{LevelReader, World},
};

/// Behavior for shulker box blocks.
#[block_behavior]
pub struct ShulkerBoxBlock {
    block: BlockRef,
}

impl ShulkerBoxBlock {
    /// Face the shulker box's lid opens towards.
    pub const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

    /// Creates a new shulker block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

// Vanilla `ShulkerBoxBlock.SHAPES_OPEN_SUPPORT`, derived there with
// `Shapes.rotateAll(Block.boxZ(16.0, 0.0, 1.0))`. Rotation is not available at
// compile time here, so each rotated result is written out directly.
const OPEN_SUPPORT_DOWN_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.0, 1.0, 0.0625, 1.0)];
const OPEN_SUPPORT_UP_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.9375, 0.0, 1.0, 1.0, 1.0)];
const OPEN_SUPPORT_NORTH_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0625)];
const OPEN_SUPPORT_SOUTH_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.9375, 1.0, 1.0, 1.0)];
const OPEN_SUPPORT_WEST_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.0, 0.0625, 1.0, 1.0)];
const OPEN_SUPPORT_EAST_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.9375, 0.0, 0.0, 1.0, 1.0, 1.0)];

/// The one-pixel slab covering `direction`'s face, all that still supports the
/// block once the lid has started opening.
const fn open_support_shape(direction: Direction) -> VoxelShape {
    VoxelShape::from_boxes(match direction {
        Direction::Down => OPEN_SUPPORT_DOWN_BOXES,
        Direction::Up => OPEN_SUPPORT_UP_BOXES,
        Direction::North => OPEN_SUPPORT_NORTH_BOXES,
        Direction::South => OPEN_SUPPORT_SOUTH_BOXES,
        Direction::West => OPEN_SUPPORT_WEST_BOXES,
        Direction::East => OPEN_SUPPORT_EAST_BOXES,
    })
}

impl BlockBehavior for ShulkerBoxBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.clicked_face();
        Some(
            self.block
                .default_state()
                .set_value(&BlockStateProperties::FACING, facing),
        )
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if let Some(block_entity) = world.get_block_entity(pos)
            && let Some(block_entity) = block_entity.downcast_ref::<ShulkerBoxBlockEntity>()
            && let Some(container_ref) = block_entity.container_ref()
            && block_entity.can_open(state, world, pos)
        {
            if !block_entity.can_player_open(player) {
                if !crate::entity::Entity::is_spectator(player) {
                    player.send_overlay_message(
                        &translations::CONTAINER_IS_LOCKED
                            .message([block_entity.name()])
                            .component(),
                    );
                }
                return InteractionResult::Success;
            }
            if let Some(loot) = block_entity.as_randomizable_container()
                && let Err(error) = loot.unpack_loot_table(Some(player))
            {
                log::error!("Could not unpack shulker box loot at {pos:?}: {error}");
                return InteractionResult::Success;
            }
            let inventory = Arc::clone(&player.inventory);
            player.open_menu(block_entity.name(), move |context| {
                shulker_box(inventory, context.container_id, container_ref)
            });

            player.award_custom_stat(&vanilla_custom_stats::OPEN_SHULKER_BOX);
            // TODO: Anger nearby piglins (PiglinAi.angerNearbyPiglins)
        }
        InteractionResult::Success
    }

    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return state;
        };

        let Some(shulker_box_block_entity) = block_entity.downcast_ref::<ShulkerBoxBlockEntity>()
        else {
            return state;
        };

        if player.prevents_block_drops() && !shulker_box_block_entity.is_empty() {
            match shulker_box_block_entity.shulker_box_as_item(state) {
                Ok(item) => world.pop_resource(pos, item),
                Err(error) => log::error!("Could not collect shulker box at {pos:?}: {error}"),
            }
        } else if let Some(loot) = shulker_box_block_entity.as_randomizable_container()
            && let Err(error) = loot.unpack_loot_table(Some(player))
        {
            log::error!("Could not unpack shulker box loot at {pos:?}: {error}");
        }

        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::SHULKER_BOX,
            level,
            pos,
            state,
        ))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::SHULKER_BOX,
        )
    }

    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        event: i32,
        data: i32,
    ) -> bool {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return false;
        };
        block_entity.trigger_event(event, data)
    }

    /// Vanilla `ShulkerBoxBlock.getShape`, which the collision shape inherits.
    ///
    /// The lid box is computed from live animation progress, so the boxes are
    /// produced directly instead of going through a static [`VoxelShape`].
    fn get_collision_boxes(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _context: BlockCollisionContext,
    ) -> BlockCollisionBoxes {
        if let Some(block_entity) = world.get_block_entity(pos)
            && let Some(shulker_box_block_entity) =
                block_entity.downcast_ref::<ShulkerBoxBlockEntity>()
        {
            return BlockCollisionBoxes::from_slice(&[
                shulker_box_block_entity.get_bounding_box(state)
            ]);
        }

        BlockCollisionBoxes::from_slice(VoxelShape::FULL_BLOCK.boxes())
    }

    fn get_block_support_boxes(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
    ) -> BlockCollisionBoxes {
        let shape = if let Some(block_entity) = world.get_block_entity(pos)
            && let Some(shulker_box_block_entity) =
                block_entity.downcast_ref::<ShulkerBoxBlockEntity>()
            && !matches!(
                shulker_box_block_entity.animation_status(),
                AnimationStatus::Closed
            ) {
            open_support_shape(state.get_value(Self::FACING).opposite())
        } else {
            VoxelShape::FULL_BLOCK
        };

        BlockCollisionBoxes::from_slice(shape.boxes())
    }

    fn affect_neighbors_after_removal(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        world.update_neighbor_for_output_signal(pos, self.block);
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let Ok(guard) = ContainerLockGuard::lock_all(&[&container_ref]) else {
            // TODO: Handle preparation failures without reporting an empty
            // inventory if lazy access is added to shulker-box storage.
            return 0;
        };
        calculate_redstone_signal_from_containers(guard.get(container_ref.container_id()))
    }

    fn fits_inside_container_items(&self) -> bool {
        false
    }
}
