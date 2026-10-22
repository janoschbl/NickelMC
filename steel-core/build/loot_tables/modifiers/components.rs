use super::super::{
    Compiler, TokenStream, ident, identifier,
    model::{CompileModifier, Condition, ContextIntProvider, Holder, HolderSet},
    quote,
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetInstrument {
    pub options: HolderSet,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetInstrument {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Self { options, .. } = self;
        let options = match options {
            HolderSet::Single(key) if key.starts_with('#') => {
                let tag = identifier(&key[1..]);
                quote! {{
                    let Some(instruments) = REGISTRY.instruments.get_tag(&#tag) else {
                        return ctx.unsupported("missing instrument tag");
                    };
                    instruments
                }}
            }
            HolderSet::Single(key) => {
                let key = ident(key);
                quote! { vec![&steel_registry::vanilla_instruments::#key] }
            }
            HolderSet::List(keys) => {
                let instruments = keys.iter().map(|key| {
                    let key = ident(key);
                    quote! { &steel_registry::vanilla_instruments::#key }
                });
                quote! { vec![#(#instruments),*] }
            }
        };
        quote! { let options = #options;
            if !options.is_empty() { let index = ctx.rng.next_i32_bounded(options.len() as i32) as usize;
                item.set(steel_registry::data_components::vanilla_components::INSTRUMENT,
                    steel_registry::data_components::InstrumentComponent::new(steel_registry::RegistryHolder::reference(options[index])));
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetEnchantments {
    #[serde(default)]
    pub enchantments: BTreeMap<String, ContextIntProvider>,
    #[serde(default)]
    pub add: bool,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for SetEnchantments {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, compiler: &mut Compiler) -> TokenStream {
        let Self {
            enchantments, add, ..
        } = self;
        let enchantments = enchantments.iter().map(|(key, value)| {
                    let key = identifier(key);
                    let level = compiler.int_provider(value);
                    quote! { let level = #level; crate::loot::modifiers::set_enchantment(item, #key, level, #add); }
                }).collect::<Vec<_>>();
        quote! {
            if item.is(&vanilla_items::BOOK) { item.set_item(&vanilla_items::ENCHANTED_BOOK.key); }
            if item.get_enchantments_for_crafting().is_some() { #(#enchantments)* }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// Banner patterns and pot decorations require their absent block-entity owners.
/// Missing owners are logged; no pattern, decoration or type-ID guesses are used.
pub struct CopyComponents {
    pub source: String,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub condition: Option<Holder<Condition>>,
}

impl CompileModifier for CopyComponents {
    fn condition(&self) -> Option<&Holder<Condition>> {
        self.condition.as_ref()
    }
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Self {
            source,
            include,
            exclude,
            ..
        } = self;
        assert_eq!(
            source, "block_entity",
            "unhandled extracted component source"
        );
        let included = if let Some(include) = include {
            let keys = include.iter().map(|key| identifier(key));
            quote! { false #(|| key == &#keys)* }
        } else {
            quote! { true }
        };
        let mut checks = vec![included];
        checks.extend(exclude.iter().flatten().map(|key| {
            let key = identifier(key);
            quote! { key != &#key }
        }));
        let included = Compiler::all(&checks);
        let needs_deferred_owner = include.as_ref().is_some_and(|keys| {
            keys.iter()
                .any(|key| key == "minecraft:banner_patterns" || key == "minecraft:pot_decorations")
        });
        quote! {
            if #needs_deferred_owner && ctx.block_entity.is_none() {
                log::error!("Cannot copy banner/pot loot components: the required block-entity foundation is unavailable");
            }
            if let Some(block_entity) = ctx.block_entity {
                use crate::block_entity::BlockEntityLifecycleExt as _;
                let components = block_entity.collect_components()?;
                let mut patch = item.patch().clone();
                for (key, value) in components.iter() {
                    let included = #included;
                    if included {
                        assert!(patch.set_raw(key.clone(), value.clone()), "collected component must retain its registered type");
                    }
                }
                *item = ItemStack::with_count_and_patch(item.item(), item.count(), patch);
            }
        }
    }
}
