//! Damage memory without strong entity back-references.
use super::{MemoryValue, RememberedEntity};
use crate::entity::damage::RecentDamageSource;
use glam::DVec3;
use steel_registry::damage_type::DamageType;
use steel_utils::{DowncastType, DowncastTypeKey};

#[derive(Debug, Clone)]
pub struct RememberedDamage {
    pub damage_type: &'static DamageType,
    pub causing_entity: Option<RememberedEntity>,
    pub direct_entity: Option<RememberedEntity>,
    pub source_position: Option<DVec3>,
}
// SAFETY: Unique Steel-owned concrete payload key.
unsafe impl DowncastType for RememberedDamage {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:memory_value/damage_source");
}
impl MemoryValue for RememberedDamage {}
impl From<&RecentDamageSource> for RememberedDamage {
    fn from(source: &RecentDamageSource) -> Self {
        Self {
            damage_type: source.damage_type,
            causing_entity: source.causing_entity().map(RememberedEntity::new),
            direct_entity: source.direct_entity().map(RememberedEntity::new),
            source_position: source.source_position_raw(),
        }
    }
}
