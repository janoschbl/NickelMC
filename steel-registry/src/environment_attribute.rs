//! The activity-valued subset of environment attributes used by brain schedules.

use steel_utils::Identifier;

use crate::biome::BiomeRef;
use crate::dimension_type::DimensionTypeRef;

/// A typed environment attribute and its extracted constant dimension/biome layers.
///
/// Activity attributes are discrete and only support the override modifier.
/// Other attribute types retain their existing readers until their layers are needed.
#[derive(Debug)]
pub struct EnvironmentAttribute<V: 'static> {
    pub key: Identifier,
    pub default_value: V,
    pub full_resolution_biomes: bool,
    pub dimensions: &'static [(DimensionTypeRef, V)],
    pub biomes: &'static [(BiomeRef, V)],
}

pub type EnvironmentAttributeRef<V> = &'static EnvironmentAttribute<V>;
