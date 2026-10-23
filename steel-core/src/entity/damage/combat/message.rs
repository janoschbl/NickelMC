use steel_registry::{
    damage_type::DeathMessageType, data_components::vanilla_components::CUSTOM_NAME,
    vanilla_damage_type_tags::DamageTypeTag,
};
use text_components::{
    Modifier, TextComponent,
    interactivity::{ClickEvent, HoverEvent},
    translation::TranslatedMessage,
};

use super::{CombatEntry, CombatTracker};
use crate::behavior::ITEM_BEHAVIORS;
use crate::entity::{Entity, LivingEntity, get_kill_credit};
use crate::inventory::equipment::EquipmentSlot;

fn translated(key: String, args: Vec<TextComponent>) -> TextComponent {
    TranslatedMessage {
        key: key.into(),
        fallback: None,
        args: Some(args.into_boxed_slice()),
    }
    .component()
}

impl CombatTracker {
    pub fn death_message(&self, victim: &dyn LivingEntity) -> TextComponent {
        let Some(killing_blow) = self.entries.last() else {
            return translated(
                "death.attack.generic".to_owned(),
                vec![victim.display_name()],
            );
        };
        let source = &killing_blow.source;
        match source.damage_type.death_message_type {
            DeathMessageType::FallVariants => {
                if let Some(entry) = self.most_significant_fall() {
                    return Self::fall_message(
                        victim,
                        entry,
                        source.causing_entity().map(AsRef::as_ref),
                    );
                }
            }
            DeathMessageType::IntentionalGameDesign => {
                let base = format!("death.attack.{}", source.damage_type.message_id);
                let link = TextComponent::const_plain("[")
                    .add_child(translated(format!("{base}.link"), vec![]))
                    .add_child("]")
                    .click_event(ClickEvent::open_url(
                        "https://bugs.mojang.com/browse/MCPE-28723",
                    ))
                    .hover_event(HoverEvent::show_text("MCPE-28723"));
                return translated(format!("{base}.message"), vec![victim.display_name(), link]);
            }
            DeathMessageType::Default => {}
        }
        let key = format!("death.attack.{}", source.damage_type.message_id);
        if source.causing_entity().is_none() && source.direct_entity().is_none() {
            let killer = victim
                .level()
                .and_then(|world| get_kill_credit(victim, &world));
            return if let Some(killer) = killer {
                translated(
                    format!("{key}.player"),
                    vec![victim.display_name(), killer.display_name()],
                )
            } else {
                translated(key, vec![victim.display_name()])
            };
        }
        let attacker = source.causing_entity().or(source.direct_entity());
        let mut args = vec![victim.display_name()];
        if let Some(attacker) = attacker {
            args.push(attacker.display_name());
        }
        if let Some(item_name) = source
            .causing_entity()
            .and_then(|entity| Self::named_weapon(entity.as_ref()))
        {
            args.push(item_name);
            return translated(format!("{key}.item"), args);
        }
        translated(key, args)
    }

    fn named_weapon(attacker: &dyn Entity) -> Option<TextComponent> {
        let living = attacker.as_living_entity()?;
        let mut result = None;
        living.with_equipment_slot(EquipmentSlot::MainHand, &mut |item| {
            if !item.is_empty() && item.has(CUSTOM_NAME) {
                result = Some(ITEM_BEHAVIORS.display_name(item));
            }
        });
        result
    }

    fn fall_message(
        victim: &dyn LivingEntity,
        entry: &CombatEntry,
        killer: Option<&dyn Entity>,
    ) -> TextComponent {
        if entry.source.is(&DamageTypeTag::IS_FALL)
            || entry
                .source
                .is(&DamageTypeTag::ALWAYS_MOST_SIGNIFICANT_FALL)
        {
            return translated(
                format!(
                    "death.fell.accident.{}",
                    entry.fall_location.unwrap_or("generic")
                ),
                vec![victim.display_name()],
            );
        }
        let attacker = entry.source.causing_entity();
        let attacker_name = attacker.map(|entity| entity.display_name());
        let killer_name = killer.map(Entity::display_name);
        let (key, entity, name) = if attacker_name.is_some() && attacker_name != killer_name {
            (
                "death.fell.assist",
                attacker.map(AsRef::as_ref),
                attacker_name,
            )
        } else if killer_name.is_some() {
            ("death.fell.finish", killer, killer_name)
        } else {
            return translated("death.fell.killer".to_owned(), vec![victim.display_name()]);
        };
        let mut args = vec![victim.display_name()];
        if let Some(name) = name {
            args.push(name);
        }
        if let Some(item_name) = entity.and_then(Self::named_weapon) {
            args.push(item_name);
            return translated(format!("{key}.item"), args);
        }
        translated(key.to_owned(), args)
    }

    fn most_significant_fall(&self) -> Option<&CombatEntry> {
        let mut result = None;
        let mut alternative = None;
        let mut alt_damage = 0.0;
        let mut best_fall = 0.0;
        for (index, entry) in self.entries.iter().enumerate() {
            let fake_fall = entry
                .source
                .is(&DamageTypeTag::ALWAYS_MOST_SIGNIFICANT_FALL);
            let distance = if fake_fall {
                f32::MAX
            } else {
                entry.fall_distance
            };
            if (entry.source.is(&DamageTypeTag::IS_FALL) || fake_fall)
                && distance > 0.0
                && (result.is_none() || distance > best_fall)
            {
                result = Some(&self.entries[index.saturating_sub(1)]);
                best_fall = distance;
            }
            if entry.fall_location.is_some() && (alternative.is_none() || entry.damage > alt_damage)
            {
                alternative = Some(entry);
                alt_damage = entry.damage;
            }
        }
        if best_fall > 5.0 && result.is_some() {
            result
        } else if alt_damage > 5.0 {
            alternative
        } else {
            None
        }
    }
}
