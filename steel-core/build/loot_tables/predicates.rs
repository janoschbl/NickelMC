use super::{
    Compiler, TokenStream, ident, identifier,
    model::{
        DamageSourcePredicate, EntityFlag, EntityPredicate, EquipmentSlot, IntRange, ItemPredicate,
    },
    quote,
};
use std::collections::BTreeMap;

impl Compiler {
    pub(super) fn item_predicate(predicate: &ItemPredicate) -> TokenStream {
        let mut checks = Vec::new();
        if let Some(items) = &predicate.items {
            if let Some(tag) = items.strip_prefix('#') {
                let tag = identifier(tag);
                checks.push(quote! { REGISTRY.items.is_in_tag(item.item, &#tag) });
            } else {
                let item = ident(items);
                checks.push(quote! { item.is(&vanilla_items::#item) });
            }
        }
        if let Some(count) = &predicate.count {
            match count {
                IntRange::Exact(count) => checks.push(quote! { item.count() == #count }),
                IntRange::Bounds(bounds) => {
                    if let Some(min) = bounds.min {
                        checks.push(quote! {item.count() >= #min});
                    }
                    if let Some(max) = bounds.max {
                        checks.push(quote! {item.count() <= #max});
                    }
                }
            }
        }
        if let Some(predicates) = &predicate.predicates {
            let mut components = BTreeMap::new();
            if let Some(enchantments) = &predicates.enchantments {
                let mut checks = vec![
                    quote! { item.has(steel_registry::data_components::vanilla_components::ENCHANTMENTS) },
                ];
                checks.extend(enchantments.iter().map(|enchantment| {
                    let key = &enchantment.enchantments;
                    let tag = key.starts_with('#');
                    let key = identifier(key.trim_start_matches('#'));
                    let min = enchantment.levels.as_ref().and_then(|levels| levels.min).unwrap_or(0);
                    let max = enchantment.levels.as_ref().and_then(|levels| levels.max).unwrap_or(i32::MAX);
                    quote! { crate::loot::predicates::item_enchantment_matches(item, &#key, #tag, #min, #max) }
                }));
                components.insert("minecraft:enchantments", checks);
            }
            for key in predicates.presence.keys() {
                assert_eq!(
                    key, "minecraft:map_id",
                    "uncompiled extracted component predicate {key}"
                );
                let id = identifier(key);
                components.insert(key.as_str(), vec![quote! { item.has_component(&#id) }]);
            }
            checks.extend(components.into_values().flatten());
        }
        Self::all(&checks)
    }

    pub(super) fn entity_predicate(predicate: &EntityPredicate) -> TokenStream {
        let mut checks = Vec::new();
        if let Some(kind) = &predicate.entity_type {
            if let Some(tag) = kind.strip_prefix('#') {
                let tag = identifier(tag);
                checks.push(quote! { entity.entity_type.and_then(|key| REGISTRY.entity_types.by_key(key)).is_some_and(|kind| REGISTRY.entity_types.is_in_tag(kind, &#tag)) });
            } else {
                let kind = identifier(kind);
                checks.push(quote! { entity.entity_type == Some(&#kind) });
            }
        }
        // Keep short-circuit order stable across the typed input representation.
        if let Some(components) = &predicate.components {
            let mut component_checks = BTreeMap::new();
            if let Some(color) = &components.sheep_color {
                component_checks.insert("minecraft:sheep/color", quote! { entity.sheep_color.is_some_and(|color| color.serialized_name() == #color) });
            }
            if let Some(variant) = &components.chicken_variant {
                let variant = identifier(variant);
                component_checks.insert(
                    "minecraft:chicken/variant",
                    quote! { entity.chicken_variant == Some(&#variant) },
                );
            }
            for name in components.unsupported.keys() {
                // Frog variants require the later mob-AI phase's entity capability.
                let operation = format!("entity component {name}");
                component_checks.insert(
                    name.as_str(),
                    quote! { ctx.missing_entity_capability(#operation) },
                );
            }
            checks.extend(component_checks.into_values());
        }
        if let Some(equipment) = &predicate.equipment {
            for (slot, predicate) in equipment {
                let slot = match slot {
                    EquipmentSlot::Mainhand => quote! { MainHand },
                    EquipmentSlot::Offhand => quote! { OffHand },
                    EquipmentSlot::Head => quote! { Head },
                    EquipmentSlot::Chest => quote! { Chest },
                    EquipmentSlot::Legs => quote! { Legs },
                    EquipmentSlot::Feet => quote! { Feet },
                    EquipmentSlot::Body => quote! { Body },
                    EquipmentSlot::Saddle => quote! { Saddle },
                };
                let predicate = Self::item_predicate(predicate);
                checks.push(quote! { crate::loot::predicates::equipment_matches(entity, steel_registry::equipment::EquipmentSlot::#slot, |item| #predicate) });
            }
        }
        if let Some(flags) = &predicate.flags {
            for (flag, value) in flags {
                let flag = match flag {
                    EntityFlag::Baby => quote! { is_baby },
                    EntityFlag::OnFire => quote! { is_on_fire },
                    EntityFlag::Sneaking => quote! { is_sneaking },
                    EntityFlag::Sprinting => quote! { is_sprinting },
                    EntityFlag::Swimming => quote! { is_swimming },
                };
                checks.push(if *value {
                    quote! { entity.flags.#flag }
                } else {
                    quote! { !entity.flags.#flag }
                });
            }
        }
        for (key, present) in [
            (
                "minecraft:type_specific/cube_mob",
                predicate.cube_mob.is_some(),
            ),
            ("minecraft:type_specific/raider", predicate.raider.is_some()),
        ] {
            if present {
                // CubeMob size and Raider captain capabilities belong to the later mob-AI phase.
                let operation = format!("entity predicate {key}");
                checks.push(quote! { ctx.missing_entity_capability(#operation) });
            }
        }
        if let Some(sheep) = &predicate.sheep {
            checks.push(sheep.sheared.map_or_else(
                || quote! { entity.sheep_sheared.is_some() },
                |sheared| quote! { entity.sheep_sheared == Some(#sheared) },
            ));
        }
        if let Some(hook) = &predicate.fishing_hook
            && let Some(expected) = hook.in_open_water
        {
            checks.push(quote! { entity.entity.and_then(|entity| entity.fishing_hook_open_water()) == Some(#expected) });
        }
        if let Some(vehicle) = &predicate.vehicle {
            let predicate = Self::entity_predicate(vehicle);
            checks.push(quote! {
                entity.entity.and_then(|entity| entity.vehicle()).is_some_and(|vehicle| {
                    let equipment = crate::entity::loot_equipment(vehicle.as_ref());
                    let entity = crate::entity::entity_loot_ref(vehicle.as_ref(), equipment.as_ref());
                    #predicate
                })
            });
        }
        Self::all(&checks)
    }

    pub(super) fn damage_source_predicate(predicate: &DamageSourcePredicate) -> TokenStream {
        let mut checks = Vec::new();
        for tag in &predicate.tags {
            let key = identifier(tag.id.strip_prefix('#').unwrap_or(&tag.id));
            checks.push(if tag.expected {
                quote! { crate::loot::predicates::damage_has_tag(source, &#key) }
            } else {
                quote! { !crate::loot::predicates::damage_has_tag(source, &#key) }
            });
        }
        for (predicate, entity) in [
            (
                predicate.direct_entity.as_ref(),
                quote! {ctx.direct_killer_entity},
            ),
            (predicate.source_entity.as_ref(), quote! {ctx.killer_entity}),
        ] {
            if let Some(predicate) = predicate {
                let check = Self::entity_predicate(predicate);
                checks.push(quote! { match #entity { Some(entity) => { let _ = &entity; #check }, None => false } });
            }
        }
        if let Some(value) = predicate.is_direct {
            checks.push(if value {
                quote! { source.is_direct }
            } else {
                quote! { !source.is_direct }
            });
        }
        let checks = Self::all(&checks);
        quote! { match ctx.damage_source { Some(source) => { let _ = &source; #checks }, None => false } }
    }

    pub(super) fn all(terms: &[TokenStream]) -> TokenStream {
        let Some((first, rest)) = terms.split_first() else {
            return quote! { true };
        };
        if rest.is_empty() {
            return first.clone();
        }
        quote! { (#first) #(&& (#rest))* }
    }
    pub(super) fn any(terms: &[TokenStream]) -> TokenStream {
        let Some((first, rest)) = terms.split_first() else {
            return quote! { false };
        };
        if rest.is_empty() {
            return first.clone();
        }
        quote! { (#first) #(|| (#rest))* }
    }
}
