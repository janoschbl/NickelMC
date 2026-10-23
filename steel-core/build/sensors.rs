//! Generate factories only for implemented sensors, retaining extracted vanilla IDs.
use quote::{format_ident, quote};
use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Deserialize)]
struct Sensor {
    id: usize,
    key: String,
    field: String,
    scan_rate: i32,
    requires: Vec<String>,
}
#[derive(Deserialize)]
struct Memory {
    key: String,
    field: String,
}
pub fn build() -> String {
    println!("cargo:rerun-if-changed=build_assets/sensor_types.json");
    let sensors: Vec<Sensor> = serde_json::from_str(
        &fs::read_to_string("build_assets/sensor_types.json").expect("sensor extractor output"),
    )
    .expect("sensor metadata");
    let memories: Vec<Memory> = serde_json::from_str(
        &fs::read_to_string("build_assets/memory_module_types.json")
            .expect("memory extractor output"),
    )
    .expect("memory metadata");
    let fields: HashMap<_, _> = memories
        .into_iter()
        .map(|memory| (memory.key, memory.field))
        .collect();
    let mut declarations = Vec::new();
    let mut registrations = Vec::new();
    for sensor in sensors {
        let implementation = match sensor.field.as_str() {
            "DUMMY" => "DummySensor",
            "NEAREST_PLAYERS" => "PlayerSensor",
            "NEAREST_LIVING_ENTITIES" => "NearestLivingEntitySensor",
            "HURT_BY" => "HurtBySensor",
            "IS_IN_WATER" => "IsInWaterSensor",
            _ => continue, // Species-specific implementations are registered when their AI lands.
        };
        let implementation = format_ident!("{implementation}");
        let name = format_ident!("{}", sensor.field);
        let required = format_ident!("{}_MEMORIES", sensor.field);
        let keys = sensor
            .requires
            .iter()
            .map(|key| format_ident!("{}", fields.get(key).expect("required memory declaration")));
        let path = sensor
            .key
            .strip_prefix("minecraft:")
            .expect("vanilla sensor namespace");
        assert_eq!(
            sensor.scan_rate, 20,
            "implement nondefault rate before registering sensor"
        );
        let id = sensor.id;
        declarations.push(quote! {
            pub const #required: &[MemoryModuleTypeRef] = &[#(memories::#keys.entry()),*];
            pub static #name: SensorType = SensorType::new(Identifier::vanilla_static(#path), || Box::new(vanilla::#implementation));
        });
        registrations.push(quote! { registry.register_vanilla(#id, &#name); });
    }
    quote! {
        use super::{vanilla, SensorType, SensorTypeRegistry};
        use crate::entity::ai::brain::memory::{MemoryModuleTypeRef, vanilla_memories as memories};
        use steel_utils::Identifier;
        #(#declarations)*
        pub(super) fn register(registry: &mut SensorTypeRegistry) { #(#registrations)* }
    }
    .to_string()
}
