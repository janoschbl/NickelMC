//! Concrete payloads shared by vanilla memory declarations.

use glam::DVec3;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_utils::nbt::{NbtNumeric, nbt_collection_values};
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey, GlobalPos, UuidExt};
use uuid::Uuid;

use super::{MemoryValue, PersistentMemoryValue, RememberedEntity};
use crate::entity::ai::path::Path;

macro_rules! payload {
    ($name:ident, $value:ty, $key:literal) => {
        #[derive(Debug, Clone)]
        pub struct $name(pub $value);
        // SAFETY: Each literal is a unique Steel-owned concrete payload key.
        unsafe impl DowncastType for $name {
            const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new($key);
        }
        impl MemoryValue for $name {}
    };
}

payload!(Boolean, bool, "steel:memory_value/bool");
payload!(Integer, i32, "steel:memory_value/int");
payload!(Long, i64, "steel:memory_value/long");
payload!(Unit, (), "steel:memory_value/unit");
payload!(BlockPosition, BlockPos, "steel:memory_value/block_pos");
payload!(GlobalPosition, GlobalPos, "steel:memory_value/global_pos");
payload!(EntityUuid, Uuid, "steel:memory_value/uuid");
payload!(Vector, DVec3, "steel:memory_value/vec3");
payload!(NavigationPath, Path, "steel:memory_value/path");

impl PersistentMemoryValue for Boolean {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::Byte(i8::from(self.0))
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        tag.codec_bool().map(Self)
    }
}
impl PersistentMemoryValue for Integer {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::Int(self.0)
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        tag.codec_i32().map(Self)
    }
}
impl PersistentMemoryValue for Long {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::Long(self.0)
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        let value = match tag {
            NbtTag::Long(value) => *value,
            NbtTag::Float(value) => *value as i64,
            NbtTag::Double(value) => *value as i64,
            _ => i64::from(tag.codec_i32()?),
        };
        Some(Self(value))
    }
}
impl PersistentMemoryValue for Unit {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::Compound(NbtCompound::new())
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        tag.compound().map(|_| Self(()))
    }
}
impl PersistentMemoryValue for BlockPosition {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::IntArray(vec![self.0.x(), self.0.y(), self.0.z()])
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        let values = nbt_collection_values(tag)?;
        let [x, y, z, ..] = values.as_slice() else {
            return None;
        };
        Some(Self(BlockPos::new(
            x.codec_i32()?,
            y.codec_i32()?,
            z.codec_i32()?,
        )))
    }
}
impl PersistentMemoryValue for GlobalPosition {
    fn to_nbt(&self) -> NbtTag {
        let mut nbt = NbtCompound::new();
        nbt.insert("dimension", self.0.dimension.to_string());
        nbt.insert("pos", BlockPosition(self.0.pos).to_nbt());
        NbtTag::Compound(nbt)
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        let nbt = tag.compound()?;
        Some(Self(GlobalPos::new(
            nbt.string("dimension")?.to_str().parse().ok()?,
            BlockPosition::from_nbt(nbt.get("pos")?)?.0,
        )))
    }
}
impl PersistentMemoryValue for EntityUuid {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::IntArray(self.0.to_int_array().to_vec())
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        let values = nbt_collection_values(tag)?
            .iter()
            .map(NbtNumeric::codec_i32)
            .collect::<Option<Vec<_>>>()?;
        Uuid::from_int_array(&values).map(Self)
    }
}

/// Ordered global positions; empty collections erase their memory slot.
#[derive(Debug, Clone, Default)]
pub struct GlobalPositions(pub Vec<GlobalPos>);
// SAFETY: Unique Steel-owned payload key.
unsafe impl DowncastType for GlobalPositions {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/global_positions");
}
impl MemoryValue for GlobalPositions {
    fn is_empty_collection(&self) -> bool {
        self.0.is_empty()
    }
}
impl PersistentMemoryValue for GlobalPositions {
    fn to_nbt(&self) -> NbtTag {
        NbtTag::List(NbtList::Compound(
            self.0
                .iter()
                .map(|pos| {
                    let NbtTag::Compound(nbt) = GlobalPosition(pos.clone()).to_nbt() else {
                        unreachable!()
                    };
                    nbt
                })
                .collect(),
        ))
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        Some(Self(
            nbt_collection_values(tag)?
                .iter()
                .map(|tag| GlobalPosition::from_nbt(tag).map(|pos| pos.0))
                .collect::<Option<Vec<_>>>()?,
        ))
    }
}

/// A set encoded by vanilla as a list, with duplicates removed on decode/write.
#[derive(Debug, Clone, Default)]
pub struct GlobalPositionSet(pub Vec<GlobalPos>);
// SAFETY: Unique Steel-owned payload key, distinct from the ordered-list payload.
unsafe impl DowncastType for GlobalPositionSet {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("steel:memory_value/global_position_set");
}
impl MemoryValue for GlobalPositionSet {
    fn is_empty_collection(&self) -> bool {
        self.0.is_empty()
    }
}
impl PersistentMemoryValue for GlobalPositionSet {
    fn to_nbt(&self) -> NbtTag {
        GlobalPositions(self.0.clone()).to_nbt()
    }
    fn from_nbt(tag: &NbtTag) -> Option<Self> {
        let mut positions = Vec::new();
        for pos in GlobalPositions::from_nbt(tag)?.0 {
            if !positions.contains(&pos) {
                positions.push(pos);
            }
        }
        Some(Self(positions))
    }
}

/// UUID collection used by frog tongue target exclusion.
#[derive(Debug, Clone, Default)]
pub struct EntityUuids(pub Vec<Uuid>);
// SAFETY: Unique Steel-owned payload key.
unsafe impl DowncastType for EntityUuids {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/uuids");
}
impl MemoryValue for EntityUuids {
    fn is_empty_collection(&self) -> bool {
        self.0.is_empty()
    }
}

/// Position-tracking capability; plugin implementations use their own concrete keys.
pub trait PositionTracker: steel_utils::ErasedType + std::fmt::Debug + Send + Sync {
    fn current_position(&self) -> Option<DVec3>;
    fn current_block_position(&self) -> Option<BlockPos>;
    fn is_visible_by(&self, visible: &super::NearestVisibleLivingEntities) -> bool;
}

/// A fixed position, preserving the exact vector for look targets.
#[derive(Debug, Clone)]
pub struct BlockPosTracker(pub DVec3);
// SAFETY: Unique Steel-owned concrete tracker key.
unsafe impl DowncastType for BlockPosTracker {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:position_tracker/block");
}
impl PositionTracker for BlockPosTracker {
    fn current_position(&self) -> Option<DVec3> {
        Some(self.0)
    }
    fn current_block_position(&self) -> Option<BlockPos> {
        Some(BlockPos::from(self.0))
    }
    fn is_visible_by(&self, _visible: &super::NearestVisibleLivingEntities) -> bool {
        true
    }
}

/// A weak, identity-preserving entity position tracker.
#[derive(Debug, Clone)]
pub struct EntityTracker {
    pub entity: RememberedEntity,
    pub track_eye_height: bool,
    pub target_eye_height: bool,
}
// SAFETY: Unique Steel-owned concrete tracker key.
unsafe impl DowncastType for EntityTracker {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:position_tracker/entity");
}
impl PositionTracker for EntityTracker {
    fn current_position(&self) -> Option<DVec3> {
        let entity = self.entity.resolve()?;
        Some(if self.track_eye_height {
            entity.position() + DVec3::Y * entity.get_eye_height()
        } else {
            entity.position()
        })
    }
    fn current_block_position(&self) -> Option<BlockPos> {
        let entity = self.entity.resolve()?;
        Some(BlockPos::from(if self.target_eye_height {
            entity.position() + DVec3::Y * entity.get_eye_height()
        } else {
            entity.position()
        }))
    }
    fn is_visible_by(&self, visible: &super::NearestVisibleLivingEntities) -> bool {
        self.entity.resolve().is_some_and(|entity| {
            entity
                .as_living_entity()
                .is_none_or(|living| living.is_alive() && visible.contains(&entity))
        })
    }
}

#[derive(Debug)]
pub struct LookTarget(pub Box<dyn PositionTracker>);
// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for LookTarget {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/look_target");
}
impl MemoryValue for LookTarget {}

#[derive(Debug)]
pub struct WalkTarget {
    pub target: Box<dyn PositionTracker>,
    pub speed_modifier: f32,
    pub close_enough_dist: i32,
}
// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for WalkTarget {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/walk_target");
}
impl MemoryValue for WalkTarget {}

/// Uninhabited vanilla dummy payload.
#[derive(Debug)]
pub enum Dummy {}
// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for Dummy {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/dummy");
}
impl MemoryValue for Dummy {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpearStatus {
    Approach,
    Charging,
    Retreat,
}
// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for SpearStatus {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/spear_status");
}
impl MemoryValue for SpearStatus {}
