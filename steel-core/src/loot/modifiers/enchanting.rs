use steel_registry::{
    REGISTRY, TaggedRegistryExt,
    data_components::vanilla_components::{ADDITIONAL_TRADE_COST, ENCHANTABLE},
    enchantment::{Enchantment, EnchantmentRef},
    item_stack::ItemStack,
    vanilla_items,
};
use steel_utils::{Identifier, random::Random};

use crate::loot::LootContext;

pub(crate) fn enchant_randomly(
    ctx: &mut LootContext<'_>,
    item: &mut ItemStack,
    options: Vec<EnchantmentRef>,
    only_compatible: bool,
    additional_cost: bool,
) {
    let book = item.is(&vanilla_items::BOOK);
    let candidates: Vec<_> = options
        .into_iter()
        .filter(|enchantment| book || !only_compatible || enchantment.can_enchant(item.item))
        .collect();
    if candidates.is_empty() {
        log::warn!(
            "Could not find a compatible loot enchantment for {}",
            item.item.key
        );
        return;
    }
    let selected = candidates[ctx.rng.next_i32_bounded(candidates.len() as i32) as usize];
    let level = if selected.max_level <= 1 {
        1
    } else {
        ctx.rng.next_i32_between(1, selected.max_level as i32)
    };
    if book {
        *item = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
    }
    item.upgrade_enchantment(selected.key.clone(), level as u32);
    if additional_cost && ctx.additional_cost_component_allowed {
        item.set(
            ADDITIONAL_TRADE_COST,
            2 + ctx.rng.next_i32_bounded(5 + level * 10) + 3 * level,
        );
    }
}

fn primary_item(enchantment: EnchantmentRef, item: &ItemStack) -> bool {
    enchantment.can_enchant(item.item)
        && enchantment.primary_items.is_none_or(|primary| {
            let Some(tag) = primary.strip_prefix('#') else {
                return false;
            };
            let Ok(tag) = tag.parse::<Identifier>() else {
                return false;
            };
            REGISTRY.items.is_in_tag(item.item, &tag)
        })
}

fn weighted_choice(
    ctx: &mut LootContext<'_>,
    candidates: &[(EnchantmentRef, u32)],
) -> Option<(EnchantmentRef, u32)> {
    let total: u64 = candidates
        .iter()
        .map(|(enchantment, _)| u64::from(enchantment.weight))
        .sum();
    let Ok(total) = i32::try_from(total) else {
        panic!("enchantment weights exceed the vanilla signed integer range");
    };
    if total == 0 {
        return None;
    }
    let mut choice = ctx.rng.next_i32_bounded(total);
    candidates.iter().copied().find(|(enchantment, _)| {
        choice -= enchantment.weight as i32;
        choice < 0
    })
}

pub(crate) fn enchant_with_levels(
    ctx: &mut LootContext<'_>,
    item: &mut ItemStack,
    mut cost: i32,
    options: Vec<EnchantmentRef>,
    additional_cost: bool,
) {
    let original_cost = cost;
    let mut selected = Vec::new();
    if let Some(enchantable) = item.get(ENCHANTABLE) {
        let bound = enchantable.value() / 4 + 1;
        cost = cost
            .wrapping_add(1 + ctx.rng.next_i32_bounded(bound) + ctx.rng.next_i32_bounded(bound));
        let span = (ctx.rng.next_f32() + ctx.rng.next_f32() - 1.0) * 0.15;
        cost = super::round_count(cost as f32 + cost as f32 * span).max(1);
        let mut candidates: Vec<_> = options
            .into_iter()
            .filter(|enchantment| item.is(&vanilla_items::BOOK) || primary_item(enchantment, item))
            .filter_map(|enchantment| {
                (1..=enchantment.max_level)
                    .rev()
                    .find(|level| {
                        let min = enchantment.min_cost.base.wrapping_add(
                            enchantment
                                .min_cost
                                .per_level_above_first
                                .wrapping_mul(*level as i32 - 1),
                        );
                        let max = enchantment.max_cost.base.wrapping_add(
                            enchantment
                                .max_cost
                                .per_level_above_first
                                .wrapping_mul(*level as i32 - 1),
                        );
                        cost >= min && cost <= max
                    })
                    .map(|level| (enchantment, level))
            })
            .collect();
        if let Some(first) = weighted_choice(ctx, &candidates) {
            selected.push(first);
            let mut remaining_cost = cost;
            while ctx.rng.next_i32_bounded(50) <= remaining_cost {
                if let Some((last, _)) = selected.last() {
                    candidates
                        .retain(|(candidate, _)| Enchantment::are_compatible(last, candidate));
                }
                if candidates.is_empty() {
                    break;
                }
                if let Some(next) = weighted_choice(ctx, &candidates) {
                    selected.push(next);
                }
                remaining_cost /= 2;
            }
        }
    }
    if item.is(&vanilla_items::BOOK) {
        *item = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
    }
    for (enchantment, level) in selected {
        item.upgrade_enchantment(enchantment.key.clone(), level);
    }
    if additional_cost
        && ctx.additional_cost_component_allowed
        && !item.is_empty()
        && original_cost > 0
    {
        item.set(ADDITIONAL_TRADE_COST, original_cost);
    }
}
