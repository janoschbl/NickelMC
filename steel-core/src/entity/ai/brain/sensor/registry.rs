//! Registry of implemented sensor factories; unsupported species sensors have no factory.
use super::{SensorTypeRef, vanilla_sensors};
use rustc_hash::FxHashMap;
use std::sync::OnceLock;
use steel_utils::Identifier;

pub struct SensorTypeRegistry {
    entries: Vec<Option<SensorTypeRef>>,
    by_key: FxHashMap<Identifier, usize>,
}
impl SensorTypeRegistry {
    pub fn register(&mut self, sensor: SensorTypeRef) -> usize {
        let id = self.entries.len();
        self.register_vanilla(id, sensor);
        id
    }
    pub(super) fn register_vanilla(&mut self, id: usize, sensor: SensorTypeRef) {
        assert!(
            !self.by_key.contains_key(&sensor.key),
            "duplicate sensor type: {}",
            sensor.key
        );
        self.entries.resize(self.entries.len().max(id + 1), None);
        assert!(self.entries[id].is_none(), "duplicate sensor id: {id}");
        self.entries[id] = Some(sensor);
        self.by_key.insert(sensor.key.clone(), id);
    }
    pub fn by_id(&self, id: usize) -> Option<SensorTypeRef> {
        self.entries.get(id).copied().flatten()
    }
    pub fn by_key(&self, key: &Identifier) -> Option<SensorTypeRef> {
        self.by_id(*self.by_key.get(key)?)
    }
}
pub static SENSOR_TYPES: OnceLock<SensorTypeRegistry> = OnceLock::new();
/// Must be called before bootstrap to append plugin-owned sensor types.
pub fn init_sensor_types_with(register: impl FnOnce(&mut SensorTypeRegistry)) -> bool {
    let mut initialized = false;
    SENSOR_TYPES.get_or_init(|| {
        let mut registry = SensorTypeRegistry {
            entries: Vec::new(),
            by_key: FxHashMap::default(),
        };
        vanilla_sensors::register(&mut registry);
        register(&mut registry);
        initialized = true;
        registry
    });
    initialized
}
