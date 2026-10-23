use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use serde::Deserialize;
use steel_utils::Identifier;

use crate::generator_functions::{generate_identifier, read_json_asset};

#[derive(Deserialize)]
pub(crate) struct ActivityAttributeEntry {
    pub key: Identifier,
    default_value: Identifier,
    full_resolution_biomes: bool,
    dimensions: Vec<Override>,
    biomes: Vec<Override>,
}

#[derive(Deserialize)]
struct Override {
    key: Identifier,
    value: Identifier,
}

pub(crate) fn activity_ref(key: &Identifier) -> TokenStream {
    assert_eq!(
        key.namespace, "minecraft",
        "Unknown vanilla activity namespace"
    );
    let name = Ident::new(&key.path.to_shouty_snake_case(), Span::call_site());
    quote! { &crate::vanilla_activities::#name }
}

pub(crate) fn build() -> TokenStream {
    let attributes: Vec<ActivityAttributeEntry> =
        read_json_asset("build_assets/activity_environment_attributes.json");
    let declarations = attributes.iter().map(|attribute| {
        let name = Ident::new(&attribute.key.path.to_shouty_snake_case(), Span::call_site());
        let key = generate_identifier(&attribute.key);
        let default_value = activity_ref(&attribute.default_value);
        let full_resolution = attribute.full_resolution_biomes;
        let dimensions = attribute.dimensions.iter().map(|entry| {
            assert_eq!(entry.key.namespace, "minecraft");
            let name = Ident::new(&entry.key.path.to_shouty_snake_case(), Span::call_site());
            let value = activity_ref(&entry.value);
            quote! { (&crate::vanilla_dimension_types::#name, #value) }
        });
        let biomes = attribute.biomes.iter().map(|entry| {
            assert_eq!(entry.key.namespace, "minecraft");
            let name = Ident::new(&entry.key.path.to_shouty_snake_case(), Span::call_site());
            let value = activity_ref(&entry.value);
            quote! { (&crate::vanilla_biomes::#name, #value) }
        });
        quote! {
            pub static #name: crate::environment_attribute::EnvironmentAttribute<crate::activity::ActivityRef> =
                crate::environment_attribute::EnvironmentAttribute {
                    key: #key,
                    default_value: #default_value,
                    full_resolution_biomes: #full_resolution,
                    dimensions: &[#(#dimensions),*],
                    biomes: &[#(#biomes),*],
                };
        }
    });
    quote! {
        use std::borrow::Cow;
        use steel_utils::Identifier;
        #(#declarations)*
    }
}
