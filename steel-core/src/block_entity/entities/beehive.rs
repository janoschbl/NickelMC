//! Beehive block entity implementation.

use std::io;
use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList};
use steel_registry::data_components::{
    CustomData, DataComponentMap,
    components::{BeehiveOccupant, Bees, EntityData},
    vanilla_components::BEES,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, RegistryExt, vanilla_block_entity_types, vanilla_entities};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Maximum number of occupants in a vanilla beehive.
pub const BEEHIVE_MAX_OCCUPANTS: usize = 3;
/// Minimum occupation time for bees without nectar.
pub const BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS: i32 = 600;

struct BeeOccupant {
    entity_data: NbtCompound,
    ticks_in_hive: i32,
    min_ticks_in_hive: i32,
}

impl BeeOccupant {
    fn worldgen(ticks_in_hive: i32) -> Self {
        Self {
            entity_data: default_bee_entity_data(),
            ticks_in_hive,
            min_ticks_in_hive: BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS,
        }
    }

    fn load(nbt: NbtCompoundView<'_, '_>) -> Self {
        let entity_data = nbt
            .compound("entity_data")
            .map_or_else(default_bee_entity_data, |entity_data| {
                entity_data.to_owned()
            });
        let ticks_in_hive = nbt.int("ticks_in_hive").unwrap_or(0);
        let min_ticks_in_hive = nbt
            .int("min_ticks_in_hive")
            .unwrap_or(BEEHIVE_MIN_OCCUPATION_TICKS_NECTARLESS);

        Self {
            entity_data,
            ticks_in_hive,
            min_ticks_in_hive,
        }
    }

    fn save(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.insert("entity_data", self.entity_data.clone());
        nbt.insert("ticks_in_hive", self.ticks_in_hive);
        nbt.insert("min_ticks_in_hive", self.min_ticks_in_hive);
        nbt
    }
}

fn default_bee_entity_data() -> NbtCompound {
    let mut entity_data = NbtCompound::new();
    entity_data.insert("id", vanilla_entities::BEE.key.to_string());
    entity_data
}

struct BeehiveState {
    stored: Vec<BeeOccupant>,
}

impl BeehiveState {
    fn push_occupant(&mut self, occupant: BeeOccupant) -> bool {
        if self.stored.len() >= BEEHIVE_MAX_OCCUPANTS {
            return false;
        }

        self.stored.push(occupant);
        true
    }
}

/// Beehive and bee nest block entity.
///
/// Currently stores and persists occupants for worldgen bee nests. Full vanilla
/// occupant ticking/release is blocked on bee entity support.
pub struct BeehiveBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BeehiveState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BeehiveBlockEntity`.
unsafe impl DowncastType for BeehiveBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/beehive");
}

impl BeehiveBlockEntity {
    /// Creates a new beehive block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BEEHIVE, level, pos, state),
            state: SyncMutex::new(BeehiveState { stored: Vec::new() }),
        }
    }

    /// Stores a vanilla worldgen bee occupant.
    ///
    /// Mirrors `BeehiveBlockEntity.Occupant.create(ticksInHive)`.
    pub fn store_worldgen_bee(&self, ticks_in_hive: i32) {
        let stored = {
            self.state
                .lock()
                .push_occupant(BeeOccupant::worldgen(ticks_in_hive))
        };
        if stored {
            BlockEntity::set_changed(self);
        }
    }

    /// Returns the number of stored occupants.
    #[must_use]
    pub fn occupant_count(&self) -> usize {
        self.state.lock().stored.len()
    }

    /// Returns whether the hive currently stores no occupants.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.lock().stored.is_empty()
    }
}

impl BlockEntity for BeehiveBlockEntity {
    fn collect_implicit_components(&self, components: &mut DataComponentMap) -> io::Result<()> {
        let bees = self
            .state
            .lock()
            .stored
            .iter()
            .map(|bee| {
                let key = bee
                    .entity_data
                    .get("id")
                    .and_then(|tag| tag.string())
                    .ok_or_else(|| io::Error::other("bee occupant has no entity id"))?
                    .to_string()
                    .parse()
                    .map_err(io::Error::other)?;
                let kind = REGISTRY
                    .entity_types
                    .by_key(&key)
                    .ok_or_else(|| io::Error::other("unknown bee occupant entity id"))?;
                let data = CustomData::try_from_compound(bee.entity_data.clone())
                    .ok_or_else(|| io::Error::other("invalid bee occupant data"))?;
                Ok(BeehiveOccupant::new(
                    EntityData::new(kind, data),
                    bee.ticks_in_hive,
                    bee.min_ticks_in_hive,
                ))
            })
            .collect::<io::Result<Vec<_>>>()?;
        components.set(BEES, Some(Bees::new(bees)));
        Ok(())
    }

    fn apply_components_from_item(&self, item: &ItemStack) {
        use simdnbt::ToNbtTag as _;
        self.state.lock().stored = item
            .get(BEES)
            .into_iter()
            .flat_map(|bees| bees.bees())
            .map(|bee| {
                let simdnbt::owned::NbtTag::Compound(entity_data) =
                    bee.entity_data().clone().to_nbt_tag()
                else {
                    unreachable!("typed entity data encodes as a compound");
                };
                BeeOccupant {
                    entity_data,
                    ticks_in_hive: bee.ticks_in_hive(),
                    min_ticks_in_hive: bee.min_ticks_in_hive(),
                }
            })
            .collect();
    }

    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut stored = Vec::new();

        if let Some(bees) = nbt.list("bees")
            && let Some(compounds) = bees.compounds()
        {
            for compound in compounds {
                if stored.len() >= BEEHIVE_MAX_OCCUPANTS {
                    break;
                }
                stored.push(BeeOccupant::load(compound));
            }
        }

        self.state.lock().stored = stored;
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let bees = self
            .state
            .lock()
            .stored
            .iter()
            .map(BeeOccupant::save)
            .collect::<Vec<_>>();
        nbt.insert("bees", NbtList::Compound(bees));
    }
}
