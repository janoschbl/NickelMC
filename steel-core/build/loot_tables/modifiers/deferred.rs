use super::super::{
    Compiler, TokenStream, ident,
    model::{
        CompileModifier, Condition, ContextFloatProvider, ContextIntProvider, Holder, HolderSet,
        StewEffect,
    },
    quote,
};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnchantRandomly {
    pub options: Option<HolderSet>,
    #[serde(default = "super::super::model::yes")]
    pub only_compatible: bool,
    #[serde(default)]
    pub include_additional_cost_component: bool,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for EnchantRandomly {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let options = enchantment_options(self.options.as_ref());
        let compatible = self.only_compatible;
        let additional = self.include_additional_cost_component;
        quote! { crate::loot::modifiers::enchanting::enchant_randomly(ctx, item, #options, #compatible, #additional); }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnchantWithLevels {
    pub levels: ContextIntProvider,
    pub options: Option<HolderSet>,
    #[serde(default)]
    pub include_additional_cost_component: bool,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for EnchantWithLevels {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, compiler: &mut Compiler) -> TokenStream {
        let options = enchantment_options(self.options.as_ref());
        let levels = compiler.int_provider(&self.levels);
        let additional = self.include_additional_cost_component;
        quote! {
            let levels = #levels;
            crate::loot::modifiers::enchanting::enchant_with_levels(ctx, item, levels, #options, #additional);
        }
    }
}

fn enchantment_options(options: Option<&HolderSet>) -> TokenStream {
    match options {
        None => {
            quote! { (0..REGISTRY.enchantments.len()).filter_map(|id| REGISTRY.enchantments.by_id(id)).collect() }
        }
        Some(HolderSet::Single(key)) if key.starts_with('#') => {
            let tag = super::super::identifier(&key[1..]);
            quote! { REGISTRY.enchantments.get_tag(&#tag).unwrap_or_else(|| panic!("Missing extracted enchantment tag {}", #tag)) }
        }
        Some(HolderSet::Single(key)) => {
            let key = ident(key);
            quote! { vec![&steel_registry::vanilla_enchantments::#key] }
        }
        Some(HolderSet::List(keys)) => {
            let keys = keys.iter().map(|key| ident(key));
            quote! { vec![#(&steel_registry::vanilla_enchantments::#keys),*] }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyState {
    pub block: String,
    pub properties: Vec<String>,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for CopyState {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let block = ident(&self.block);
        let properties = &self.properties;
        quote! {
            if let Some(state) = ctx.block_state {
                use steel_registry::data_components::vanilla_components::{BlockItemStateProperties, BLOCK_STATE};
                let mut properties = item.get(BLOCK_STATE)
                    .map(|value| value.properties().clone()).unwrap_or_default();
                #(
                    if steel_registry::vanilla_blocks::#block.default_state().get_property_str(#properties).is_some()
                        && let Some(value) = state.get_property_str(#properties)
                    {
                        properties.insert(#properties.to_owned(), value.to_owned());
                    }
                )*
                item.set(BLOCK_STATE, BlockItemStateProperties::new(properties));
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetComponents {
    pub components: ComponentPatch,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetComponents {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Some(trim) = &self.components.trim else {
            return quote! {};
        };
        let material = ident(&trim.material);
        let pattern = ident(&trim.pattern);
        quote! {
            item.set(steel_registry::data_components::vanilla_components::TRIM,
                steel_registry::data_components::vanilla_components::ArmorTrim::new(
                    steel_registry::RegistryHolder::reference(&steel_registry::vanilla_trim_materials::#material),
                    steel_registry::RegistryHolder::reference(&steel_registry::vanilla_trim_patterns::#pattern),
                ));
        }
    }
}

/// Typed component payloads present in extracted vanilla loot. New payloads must
/// gain a typed compiler instead of being discarded or parsed during gameplay.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentPatch {
    #[serde(rename = "minecraft:trim")]
    pub trim: Option<Trim>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trim {
    pub material: String,
    pub pattern: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetDamage {
    pub damage: ContextFloatProvider,
    #[serde(default)]
    pub add: bool,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetDamage {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, compiler: &mut Compiler) -> TokenStream {
        let damage = compiler.float_provider(&self.damage);
        let add = self.add;
        quote! {
            if item.is_damageable_item() {
                let max_damage = item.get_max_damage();
                let base = if #add { 1.0 - item.get_damage_value() as f32 / max_damage as f32 } else { 0.0 };
                let fraction = 1.0 - (#damage + base).clamp(0.0, 1.0);
                item.set_damage_value((fraction * max_damage as f32).floor() as i32);
            } else {
                log::warn!("Could not set damage of loot item {}", item.item.key);
            }
        }
    }
}

/// Exploration maps remain unsupported until loot generation can allocate saved
/// map data, render its biome preview, and synchronously search for the nearest
/// eligible structure.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplorationMapFunction {
    #[serde(rename = "destination")]
    pub _destination: Option<String>,
    #[serde(rename = "decoration")]
    pub _decoration: Option<String>,
    #[serde(rename = "zoom")]
    pub _zoom: Option<i32>,
    #[serde(rename = "skip_existing_chunks")]
    pub _skip_existing_chunks: Option<bool>,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for ExplorationMapFunction {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        quote! {
            log::error!("Cannot apply exploration_map: saved-map allocation, biome preview, and synchronous nearest-structure search are unavailable; leaving the stack unchanged");
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetName {
    pub name: Option<LootName>,
    #[serde(default)]
    pub target: NameTarget,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetName {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Some(name) = &self.name else {
            return quote! {};
        };
        let name = match name {
            LootName::Plain(text) => {
                quote! { text_components::TextComponent::plain(#text.to_owned()) }
            }
            LootName::Translated { translate } => quote! {
                text_components::TextComponent::translated(text_components::translation::TranslatedMessage::new(#translate, None))
            },
        };
        let target = match self.target {
            NameTarget::ItemName => quote! { ITEM_NAME },
            NameTarget::CustomName => quote! { CUSTOM_NAME },
        };
        quote! { item.set(steel_registry::data_components::vanilla_components::#target, #name); }
    }
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum LootName {
    Plain(String),
    Translated { translate: String },
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameTarget {
    #[default]
    CustomName,
    ItemName,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPotion {
    pub id: String,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetPotion {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let potion = ident(&self.id);
        quote! {
            use steel_registry::data_components::{PotionContents, vanilla_components::POTION_CONTENTS};
            let previous = item.get(POTION_CONTENTS).cloned().unwrap_or_default();
            item.set(POTION_CONTENTS, PotionContents::new(
                Some(steel_registry::RegistryReference::new(&steel_registry::vanilla_potions::#potion)),
                previous.custom_color(), previous.custom_effects().to_vec(), previous.custom_name().map(str::to_owned),
            ));
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetStewEffect {
    #[serde(default)]
    pub effects: Vec<StewEffect>,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetStewEffect {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, compiler: &mut Compiler) -> TokenStream {
        let mut seen = BTreeSet::new();
        let arms = self
            .effects
            .iter()
            .enumerate()
            .map(|(index, effect)| {
                assert!(
                    seen.insert(&effect.effect),
                    "Duplicate stew effect {}",
                    effect.effect
                );
                let effect_name = ident(&effect.effect);
                let duration = compiler.int_provider(&effect.duration);
                quote! { #index => (&steel_registry::vanilla_mob_effects::#effect_name, #duration) }
            })
            .collect::<Vec<_>>();
        let count = self.effects.len();
        if count == 0 {
            return quote! {};
        }
        quote! {
            if item.is(&vanilla_items::SUSPICIOUS_STEW) {
                use steel_registry::data_components::vanilla_components::{SUSPICIOUS_STEW_EFFECTS, SuspiciousStewEffects, SuspiciousStewEffect};
                let index = ctx.rng.next_i32_bounded(#count as i32) as usize;
                let (effect, mut duration) = match index { #(#arms,)* _ => unreachable!("bounded stew effect index") };
                if crate::behavior::MOB_EFFECT_BEHAVIORS.get_behavior(effect).as_instantaneous().is_none() {
                    duration = duration.wrapping_mul(20);
                }
                let mut effects = item.get(SUSPICIOUS_STEW_EFFECTS).map(|value| value.effects().to_vec()).unwrap_or_default();
                effects.push(SuspiciousStewEffect::new(effect, duration));
                item.set(SUSPICIOUS_STEW_EFFECTS, SuspiciousStewEffects::new(effects));
            }
        }
    }
}
