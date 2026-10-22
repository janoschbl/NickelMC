use std::convert::Infallible;

use rand::{Rng as _, TryRng};
use steel_registry::loot_table::{LootCondition, LootContext, LootFunction};
use steel_registry::{vanilla_attributes, vanilla_damage_types, vanilla_loot_tables};
use steel_utils::random::legacy_random::LegacyRandom;

use super::*;
use crate::entity::LivingEntity as _;
use crate::entity::entities::EndCrystalEntity;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};

// rand's f32 sampler uses the high 24 bits, just like LegacyRandom.next_f32.
struct JavaLootRandom(LegacyRandom);

impl TryRng for JavaLootRandom {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok(self.0.next_i32() as u32)
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        Ok(self.0.next_i64() as u64)
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        for chunk in dst.chunks_mut(4) {
            let bytes = self.next_u32().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

#[test]
fn tnt_and_creeper_blasts_apply_vanilla_damage_and_knockback() {
    for (radius, offset, expected_damage, resistance) in [
        (3.0, 4.5, 7.5625, 0.0),
        (3.0, 4.5, 7.5625, 0.5),
        (4.0, 6.0, 9.75, 0.0),
        (4.0, 6.0, 9.75, 0.5),
    ] {
        let fixture = ready_world("explosion_damage_parity");
        let world = &fixture.world;
        let center = DVec3::new(0.5, 64.0, 0.5);
        let pig = add_entity(
            world,
            PigEntity::new(
                &vanilla_entities::PIG,
                next_entity_id(),
                center + DVec3::new(offset, 0.0, 0.0),
                Arc::downgrade(world),
            ),
        );
        pig.attributes().lock().set_base_value(
            vanilla_attributes::EXPLOSION_KNOCKBACK_RESISTANCE,
            resistance,
        );
        let health_before = pig.get_health();
        let mut explosion = ServerExplosion::new(
            world,
            None,
            None,
            None,
            None,
            center,
            radius,
            false,
            BlockInteraction::Destroy,
        );

        explosion.hurt_entities();

        // Normalized distance is 0.75, exposure is 1, and impact is 0.25.
        assert_eq!(health_before - pig.get_health(), expected_damage);
        let direction = (pig.explosion_damage_origin() - center).normalize();
        assert!((pig.velocity() - direction * (0.25 * (1.0 - resistance))).length() < 1.0e-12);
        assert!(pig.velocity().x > 0.0 && pig.velocity().y > 0.0);
    }
}

#[test]
fn explosion_decay_and_survival_match_java_seed_one() {
    init_vanilla_registry();
    // BlastOracle.java --loot: 64 nextFloat() <= 1/radius draws, seed 1.
    for (radius, expected_count) in [(3.0, 24), (4.0, 20)] {
        let mut random = JavaLootRandom(LegacyRandom::from_seed(1));
        let mut context = LootContext::new(&mut random).with_explosion(radius);
        let mut stack = ItemStack::with_count(&vanilla_items::COBBLESTONE, 64);
        LootFunction::ExplosionDecay.apply(&mut stack, &mut context);
        assert_eq!(stack.count(), expected_count);

        let mut random = JavaLootRandom(LegacyRandom::from_seed(1));
        let mut context = LootContext::new(&mut random).with_explosion(radius);
        let survived = (0..64)
            .filter(|_| LootCondition::SurvivesExplosion.test(&mut context))
            .count();
        assert_eq!(survived, expected_count as usize);

        let mut random = JavaLootRandom(LegacyRandom::from_seed(1));
        let tool = ItemStack::empty();
        let mut context = LootContext::new(&mut random)
            .with_tool(&tool)
            .with_block_state(vanilla_blocks::STONE.default_state())
            .with_explosion(radius);
        let dropped: i32 = (0..64)
            .flat_map(|_| vanilla_loot_tables::BLOCKS_STONE.get_random_items(&mut context))
            .map(|item| {
                assert!(item.is(&vanilla_items::COBBLESTONE));
                item.count()
            })
            .sum();
        assert_eq!(dropped, expected_count);
    }
}

#[test]
fn destructive_callbacks_preserve_loot_without_decay_and_at_radius_one() {
    for interaction in [
        BlockInteraction::Destroy,
        BlockInteraction::DestroyWithDecay,
    ] {
        let fixture = ready_world("explosion_loot_context");
        let world = &fixture.world;
        let mut affected = (0..12).map(|x| BlockPos::new(x, 64, 0)).collect::<Vec<_>>();
        for &pos in &affected {
            assert!(world.set_block(
                pos,
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE
            ));
        }
        let radius = if interaction == BlockInteraction::Destroy {
            4.0
        } else {
            1.0
        };
        let explosion = ServerExplosion::new(
            world,
            None,
            None,
            None,
            None,
            TEST_BLOCK_CENTER,
            radius,
            false,
            interaction,
        );
        explosion.interact_with_blocks(&mut affected, &mut LegacyRandom::from_seed(1));
        assert!(
            affected
                .iter()
                .all(|&pos| world.get_block_state(pos).is_air())
        );
        let drops = world.get_entities_in_aabb_matching(
            &WorldAabb::new(-1.0, 63.0, -1.0, 13.0, 66.0, 2.0),
            |entity| entity.entity_type() == &vanilla_entities::ITEM,
        );
        let count: i32 = drops
            .iter()
            .filter_map(|entity| entity.downcast_ref::<ItemEntity>())
            .map(|item| {
                let stack = item.get_item();
                assert!(stack.is(&vanilla_items::COBBLESTONE));
                stack.count()
            })
            .sum();
        assert_eq!(count, 12);
    }
}

#[test]
fn explosion_removal_spills_barrel_contents_once() {
    let fixture = ready_world("explosion_barrel_contents");
    init_block_entities();
    let world = &fixture.world;
    let pos = BlockPos::new(4, 64, 4);
    assert!(world.set_block(
        pos,
        vanilla_blocks::BARREL.default_state(),
        UpdateFlags::UPDATE_NONE
    ));
    let block_entity = world.get_block_entity(pos).expect("barrel block entity");
    let container_ref = ContainerRef::from_block_entity(block_entity).expect("barrel container");
    {
        let mut guard = ContainerLockGuard::lock_all(&[&container_ref]);
        let container = guard
            .get_mut(container_ref.container_id())
            .expect("locked barrel");
        container.set_item(0, ItemStack::with_count(&vanilla_items::DIAMOND, 23));
    }
    let explosion = ServerExplosion::new(
        world,
        None,
        None,
        None,
        None,
        TEST_BLOCK_CENTER,
        4.0,
        false,
        BlockInteraction::Destroy,
    );
    explosion.interact_with_blocks(&mut [pos], &mut LegacyRandom::from_seed(1));
    assert!(world.get_block_state(pos).is_air());
    assert!(world.get_block_entity(pos).is_none());
    let drops = world.get_entities_in_aabb(&WorldAabb::new(3.0, 63.0, 3.0, 6.0, 67.0, 6.0));
    let diamonds: i32 = drops
        .iter()
        .filter_map(|entity| entity.downcast_ref::<ItemEntity>())
        .map(ItemEntity::get_item)
        .filter(|stack| stack.is(&vanilla_items::DIAMOND))
        .map(|stack| stack.count())
        .sum();
    assert_eq!(diamonds, 23);
}

#[test]
fn crystal_destruction_explodes_unless_the_damage_was_an_explosion() {
    for hit_by_explosion in [false, true] {
        let fixture = ready_world("explosion_crystal_source");
        let world = &fixture.world;
        let glass = BlockPos::new(5, 64, 4);
        assert!(world.set_block(
            glass,
            vanilla_blocks::GLASS.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
        let crystal = add_entity(
            world,
            EndCrystalEntity::new(
                &vanilla_entities::END_CRYSTAL,
                next_entity_id(),
                DVec3::new(4.5, 64.0, 4.5),
                Arc::downgrade(world),
            ),
        );
        let source = if hit_by_explosion {
            DamageSource::explosion(None, None)
        } else {
            DamageSource::environment(&vanilla_damage_types::GENERIC)
        };
        assert!(crystal.hurt(world, &source, 1.0));
        assert!(crystal.is_removed());
        assert_eq!(world.get_block_state(glass).is_air(), !hit_by_explosion);
    }
}
