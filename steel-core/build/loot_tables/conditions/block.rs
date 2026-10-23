use super::super::{
    Compiler, TokenStream, ident,
    model::{CompileCondition, HolderSet, ItemPredicate, StateProperties},
    quote,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurvivesExplosion {}

impl CompileCondition for SurvivesExplosion {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        quote! { ctx.explosion_radius.is_none_or(|radius| ctx.rng.next_f32() <= 1.0 / radius) }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchBlock {
    pub blocks: Option<HolderSet>,
    #[serde(default)]
    pub state: StateProperties,
}

impl CompileCondition for MatchBlock {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let predicate = self.state_matches();
        quote! { ctx.block_state.is_some_and(|state| #predicate) }
    }
}

impl MatchBlock {
    pub(super) fn state_matches(&self) -> TokenStream {
        let Self { blocks, state, .. } = self;
        let block = match blocks {
            None => quote! { true },
            Some(HolderSet::Single(key)) if key.starts_with('#') => {
                let key = super::super::identifier(&key[1..]);
                quote! { REGISTRY.blocks.is_in_tag(state.get_block(), &#key) }
            }
            Some(HolderSet::Single(key)) => {
                let block = ident(key);
                quote! { state.get_block() == &steel_registry::vanilla_blocks::#block }
            }
            Some(HolderSet::List(keys)) => {
                let blocks = keys.iter().map(|key| ident(key));
                quote! { false #(|| state.get_block() == &steel_registry::vanilla_blocks::#blocks)* }
            }
        };
        let properties = state
            .iter()
            .map(|(name, expected)| {
                quote! { state.get_property_str(#name).is_some_and(|value| value == #expected) }
            })
            .collect::<Vec<_>>();
        quote! { (#block) #(&& #properties)* }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchTool {
    pub predicate: Option<ItemPredicate>,
}

impl CompileCondition for MatchTool {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Self { predicate, .. } = self;
        let predicate = predicate
            .as_ref()
            .map_or_else(|| quote! { true }, Compiler::item_predicate);
        quote! { ctx.tool.is_some_and(|item| #predicate) }
    }
}
