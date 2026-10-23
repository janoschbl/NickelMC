use super::super::{
    Compiler, TokenStream,
    model::{CompileCondition, DamageSourcePredicate, EntityPredicate, EntityTarget, HolderSet},
    quote,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KilledByPlayer {}

impl CompileCondition for KilledByPlayer {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        quote! { ctx.killed_by_player }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityProperties {
    pub entity: EntityTarget,
    pub predicate: Option<EntityPredicate>,
}

impl CompileCondition for EntityProperties {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Self {
            entity, predicate, ..
        } = self;
        let target = match entity {
            EntityTarget::This => quote! { LootContextEntity::This },
            EntityTarget::Attacker => quote! { LootContextEntity::Killer },
            EntityTarget::DirectAttacker => quote! { LootContextEntity::DirectKiller },
            EntityTarget::LastDamagePlayer => quote! { LootContextEntity::KillerPlayer },
            EntityTarget::InteractingEntity => quote! { LootContextEntity::Interacting },
        };
        let Some(predicate) = predicate else {
            return quote! { true };
        };
        let predicate = Compiler::entity_predicate(predicate);
        quote! { match ctx.get_entity(#target) { Some(entity) => { let _ = &entity; #predicate }, None => false } }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageSourceProperties {
    pub predicate: Option<DamageSourcePredicate>,
}

impl CompileCondition for DamageSourceProperties {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Self { predicate, .. } = self;
        let predicate = predicate
            .as_ref()
            .map_or_else(|| quote! { true }, Compiler::damage_source_predicate);
        quote! { ctx.origin.is_some() && ctx.damage_source.is_some() && (#predicate) }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationCheck {
    #[serde(default, rename = "offsetX")]
    pub offset_x: i32,
    #[serde(default, rename = "offsetY")]
    pub offset_y: i32,
    #[serde(default, rename = "offsetZ")]
    pub offset_z: i32,
    pub predicate: Option<LocationPredicate>,
}

/// Fields used by extracted vanilla loot. Other payloads fail decoding at build
/// time rather than silently omitting a location constraint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationPredicate {
    pub biomes: Option<HolderSet>,
    pub block: Option<super::block::MatchBlock>,
}

impl CompileCondition for LocationCheck {
    fn compile(&self, _compiler: &mut Compiler) -> TokenStream {
        let Some(predicate) = &self.predicate else {
            return quote! { ctx.origin.is_some() };
        };
        let (dx, dy, dz) = (self.offset_x, self.offset_y, self.offset_z);
        let mut checks = vec![];
        if let Some(biomes) = &predicate.biomes {
            let matches = match biomes {
                HolderSet::Single(key) if key.starts_with('#') => {
                    let key = super::super::identifier(&key[1..]);
                    quote! { REGISTRY.biomes.is_in_tag(biome, &#key) }
                }
                HolderSet::Single(key) => {
                    let biome = super::super::ident(key);
                    quote! { biome == &*steel_registry::vanilla_biomes::#biome }
                }
                HolderSet::List(keys) => {
                    let biomes = keys.iter().map(|key| super::super::ident(key));
                    quote! { false #(|| biome == &*steel_registry::vanilla_biomes::#biomes)* }
                }
            };
            checks.push(quote! { world.biome_at(pos).is_some_and(|biome| #matches) });
        }
        if let Some(block) = &predicate.block {
            let block = block.state_matches();
            checks.push(quote! {{ let state = crate::world::LevelReader::get_block_state(world, pos); #block }});
        }
        if checks.is_empty() {
            return quote! { ctx.origin.is_some() };
        }
        let checks = Compiler::all(&checks);
        quote! {{
            match (ctx.origin, ctx.world) {
                (Some((x, y, z)), Some(world)) => {
                    let pos = steel_utils::BlockPos::new((x + f64::from(#dx)).floor() as i32, (y + f64::from(#dy)).floor() as i32, (z + f64::from(#dz)).floor() as i32);
                    world.chunk_map.with_full_chunk(steel_utils::ChunkPos::from_block_pos(pos), |_| ()).is_some() && (#checks)
                }
                _ => false,
            }
        }}
    }
}
