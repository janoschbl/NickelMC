//! Compile the supported vanilla trigger predicates into core, never runtime JSON.

use heck::ToShoutySnakeCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde_json::Value;
use std::{fs, path::Path};

pub(crate) fn build() -> String {
    let root = Path::new("../steel-utils/build_assets/builtin_datapacks/minecraft/advancement");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut paths = Vec::new();
    visit(root, &mut paths);
    paths.sort();
    let mut listeners = Vec::new();
    for path in paths {
        let data: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read advancement"))
                .expect("parse advancement");
        let key = path
            .strip_prefix(root)
            .expect("advancement path")
            .with_extension("");
        let advancement = format_ident!(
            "{}",
            key.to_string_lossy()
                .replace('/', "_")
                .to_shouty_snake_case()
        );
        for (name, criterion) in data["criteria"].as_object().expect("criteria object") {
            let trigger = criterion["trigger"].as_str().expect("criterion trigger");
            let conditions = criterion
                .get("conditions")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let predicate = match trigger {
                "minecraft:enchanted_item" => {
                    // No conditions are present on the extracted 26.3 enchanted-item criterion.
                    // Fail closed if that changes rather than silently ignoring predicates.
                    assert!(
                        conditions.as_object().is_some_and(|map| map.is_empty()),
                        "uncompiled enchanted-item conditions: {conditions}"
                    );
                    quote! { matches!(event, CriterionEvent::EnchantedItem { .. }) }
                }
                "minecraft:effects_changed" => effects_changed(&conditions),
                _ => continue,
            };
            listeners.push(quote! {
                CriterionListener {
                    advancement: &steel_registry::vanilla_advancements::#advancement,
                    criterion: #name,
                    matches: |player, event| { let _ = player; #predicate },
                }
            });
        }
    }
    quote! {
        fn vanilla_listeners() -> &'static [CriterionListener] {
            static LISTENERS: std::sync::LazyLock<Vec<CriterionListener>> = std::sync::LazyLock::new(|| vec![#(#listeners),*]);
            &LISTENERS
        }
    }.to_string()
}

fn visit(root: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(root).expect("advancement directory") {
        let path = entry.expect("advancement entry").path();
        if path.is_dir() {
            visit(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
}

fn effects_changed(conditions: &Value) -> TokenStream {
    let object = conditions.as_object().expect("effects-changed conditions");
    assert!(
        object.keys().all(|key| key == "effects" || key == "source"),
        "uncompiled effects-changed conditions: {conditions}"
    );
    let mut checks = Vec::new();
    if let Some(effects) = object.get("effects") {
        for (key, predicate) in effects.as_object().expect("effects map") {
            assert!(
                predicate.as_object().is_some_and(|map| map.is_empty()),
                "uncompiled effect predicate: {predicate}"
            );
            let (namespace, path) = key.split_once(':').expect("effect identifier");
            assert_eq!(namespace, "minecraft");
            let effect = format_ident!("{}", path.to_shouty_snake_case());
            checks.push(quote! { crate::entity::LivingEntity::has_mob_effect(player, steel_registry::vanilla_mob_effects::#effect) });
        }
    }
    if let Some(source) = object.get("source") {
        let source = source.as_object().expect("source condition");
        assert_eq!(source.len(), 3, "uncompiled source fields");
        assert_eq!(source["type"], "minecraft:entity_properties");
        assert_eq!(source["entity"], "this");
        let predicate = source["predicate"].as_object().expect("source predicate");
        assert_eq!(predicate.len(), 1, "uncompiled source predicate");
        let key = predicate["minecraft:entity_type"]
            .as_str()
            .expect("source entity type");
        let (namespace, path) = key.split_once(':').expect("entity identifier");
        assert_eq!(namespace, "minecraft");
        let entity = format_ident!("{}", path.to_shouty_snake_case());
        checks.push(quote! { source.is_some_and(|source| source.entity_type() == &steel_registry::vanilla_entities::#entity) });
    }
    quote! { if let CriterionEvent::EffectsChanged { source } = event { let _ = source; true #(&& #checks)* } else { false } }
}
