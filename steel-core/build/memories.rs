//! Typed vanilla memory declarations from SteelExtractor's Java payload metadata.
use quote::{format_ident, quote};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Memory {
    id: usize,
    key: String,
    field: String,
    java_value_type: String,
    can_serialize: bool,
}

pub fn build() -> String {
    const ASSET: &str = "build_assets/memory_module_types.json";
    println!("cargo:rerun-if-changed={ASSET}");
    let mut entries: Vec<Memory> =
        serde_json::from_str(&fs::read_to_string(ASSET).expect("memory extractor output"))
            .expect("memory declarations");
    entries.sort_by_key(|entry| entry.id);
    let mut declarations = Vec::new();
    let mut registrations = Vec::new();
    for (id, entry) in entries.into_iter().enumerate() {
        assert_eq!(id, entry.id, "memory ids must be contiguous");
        let rust_type = match entry.java_value_type.as_str() {
            "java.lang.Void" => "Dummy",
            "java.lang.Boolean" => "Boolean",
            "java.lang.Integer" => "Integer",
            "java.lang.Long" => "Long",
            "net.minecraft.util.Unit" => "Unit",
            "java.util.UUID" => "EntityUuid",
            "java.util.List<java.util.UUID>" => "EntityUuids",
            "net.minecraft.core.BlockPos" => "BlockPosition",
            "net.minecraft.core.GlobalPos" => "GlobalPosition",
            "java.util.List<net.minecraft.core.GlobalPos>" => "GlobalPositions",
            "java.util.Set<net.minecraft.core.GlobalPos>" => "GlobalPositionSet",
            "net.minecraft.world.phys.Vec3" => "Vector",
            "net.minecraft.world.level.pathfinder.Path" => "NavigationPath",
            "net.minecraft.world.damagesource.DamageSource" => "RememberedDamage",
            "net.minecraft.world.entity.ai.memory.WalkTarget" => "WalkTarget",
            "net.minecraft.world.entity.ai.behavior.PositionTracker" => "LookTarget",
            "net.minecraft.world.entity.ai.memory.NearestVisibleLivingEntities" => {
                "NearestVisibleLivingEntities"
            }
            "net.minecraft.world.entity.ai.behavior.SpearAttack$SpearStatus" => "SpearStatus",
            value if value.starts_with("java.util.List<net.minecraft.world.entity.") => {
                "RememberedEntities"
            }
            value if value.starts_with("net.minecraft.world.entity.") => "RememberedEntity",
            value => panic!("Unsupported extracted memory payload: {value}"),
        };
        let ty = format_ident!("{rust_type}");
        let field = format_ident!("{}", entry.field);
        let key = entry
            .key
            .strip_prefix("minecraft:")
            .expect("vanilla memory namespace");
        let constructor = if entry.can_serialize {
            format_ident!("new_persistent")
        } else {
            format_ident!("new")
        };
        declarations.push(quote! { pub static #field: MemoryModuleType<#ty> = MemoryModuleType::#constructor(Identifier::vanilla_static(#key)); });
        registrations.push(quote! { assert_eq!(registry.register(&#field), #id); });
    }
    quote! {
        use crate::entity::ai::brain::memory::*;
        use steel_utils::Identifier;
        #(#declarations)*
        pub(super) fn register(registry: &mut MemoryModuleTypeRegistry) { #(#registrations)* }
    }
    .to_string()
}
