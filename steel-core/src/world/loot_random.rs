use serde::{Deserialize, Serialize};
use std::{io, sync::Arc};
use steel_registry::loot_table::{LootRng, LootTableRef};
use steel_utils::{
    Identifier,
    random::{Random as _, legacy_random::LegacyRandom, xoroshiro::Xoroshiro},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct PersistedLootRandomSequences {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: Vec<PersistedLootRandomSequence>,
}

impl Default for PersistedLootRandomSequences {
    fn default() -> Self {
        Self {
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PersistedLootRandomSequence {
    key: Identifier,
    seed_lo: u64,
    seed_hi: u64,
}

pub(super) struct LootRandomSequences {
    world_seed: i64,
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: rustc_hash::FxHashMap<Identifier, Xoroshiro>,
}

impl LootRandomSequences {
    pub(super) fn new(world_seed: i64, persisted: PersistedLootRandomSequences) -> Self {
        let sequences = persisted
            .sequences
            .into_iter()
            .map(|sequence| {
                (
                    sequence.key,
                    Xoroshiro::from_state(sequence.seed_lo, sequence.seed_hi),
                )
            })
            .collect();
        Self {
            world_seed,
            salt: persisted.salt,
            include_world_seed: persisted.include_world_seed,
            include_sequence_id: persisted.include_sequence_id,
            sequences,
        }
    }

    pub(super) fn sequence(&mut self, key: &Identifier) -> &mut Xoroshiro {
        self.sequences.entry(key.clone()).or_insert_with(|| {
            let seed = (if self.include_world_seed {
                self.world_seed
            } else {
                0
            }) ^ i64::from(self.salt);
            if self.include_sequence_id {
                Xoroshiro::from_sequence_seed(seed, &key.to_string())
            } else {
                Xoroshiro::from_seed(seed as u64)
            }
        })
    }

    pub(super) fn save(&self) -> PersistedLootRandomSequences {
        let mut sequences: Vec<_> = self
            .sequences
            .iter()
            .map(|(key, random)| {
                let (seed_lo, seed_hi) = random.state();
                PersistedLootRandomSequence {
                    key: key.clone(),
                    seed_lo,
                    seed_hi,
                }
            })
            .collect();
        sequences.sort_unstable_by_key(|sequence| sequence.key.to_string());
        PersistedLootRandomSequences {
            salt: self.salt,
            include_world_seed: self.include_world_seed,
            include_sequence_id: self.include_sequence_id,
            sequences,
        }
    }
}

pub(crate) enum LootRandom<'a> {
    Legacy(&'a mut LegacyRandom),
    Xoroshiro(&'a mut Xoroshiro),
}

impl LootRng for LootRandom<'_> {
    fn next_float(&mut self) -> f32 {
        match self {
            Self::Legacy(random) => random.next_f32(),
            Self::Xoroshiro(random) => random.next_f32(),
        }
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        match self {
            Self::Legacy(random) => random.next_i32_bounded(bound),
            Self::Xoroshiro(random) => random.next_i32_bounded(bound),
        }
    }
}

impl super::World {
    pub(crate) fn share_loot_random_sequences(&self, primary: &Self) {
        *self.loot_random_sequences.write() = Arc::clone(&primary.loot_random_sequences.read());
    }

    pub(crate) async fn save_loot_random_sequences(&self) -> io::Result<()> {
        if !self.level_data.read().owns_game_time() {
            return Ok(());
        }
        let snapshot = self.loot_random_sequences.read().lock().save();
        self.saved_data
            .save(
                steel_utils::saved_data::names::LOOT_RANDOM_SEQUENCES,
                &snapshot,
            )
            .await
    }

    pub(crate) fn with_loot_rng<T>(
        &self,
        table: LootTableRef,
        optional_seed: i64,
        evaluate: impl FnOnce(&mut LootRandom<'_>) -> T,
    ) -> T {
        if optional_seed != 0 {
            let mut random = LegacyRandom::from_seed(optional_seed as u64);
            return evaluate(&mut LootRandom::Legacy(&mut random));
        }

        if let Some(key) = table.random_sequence.as_ref() {
            let shared = self.loot_random_sequences.read();
            let mut sequences = shared.lock();
            return evaluate(&mut LootRandom::Xoroshiro(sequences.sequence(key)));
        }

        let mut random = self.level_random.lock();
        evaluate(&mut LootRandom::Legacy(&mut random))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fresh_test_world, test_domain};
    use steel_registry::{init_vanilla_registry, loot_table::LootContext, vanilla_loot_tables};

    #[tokio::test]
    async fn sequence_draws_continue_after_persistence_round_trip() {
        let key = Identifier::vanilla_static("blocks/stone");
        let mut sequences = LootRandomSequences::new(1234, PersistedLootRandomSequences::default());
        let first = sequences.sequence(&key).next_i64();
        let path =
            std::env::temp_dir().join(format!("nickel-loot-random-{}", uuid::Uuid::new_v4()));
        let storage = steel_utils::saved_data::SavedDataManager::new(Some(&path));
        let name = steel_utils::saved_data::names::LOOT_RANDOM_SEQUENCES;
        storage
            .save(name, &sequences.save())
            .await
            .expect("save sequences");
        let storage = steel_utils::saved_data::SavedDataManager::new(Some(&path));
        let saved = storage.load_or_default(name).await.expect("load sequences");
        let mut restored = LootRandomSequences::new(1234, saved);
        let continued = restored.sequence(&key).next_i64();

        let mut uninterrupted =
            LootRandomSequences::new(1234, PersistedLootRandomSequences::default());
        let expected_first = uninterrupted.sequence(&key).next_i64();
        let expected_continued = uninterrupted.sequence(&key).next_i64();
        assert_eq!(first, expected_first);
        assert_eq!(continued, expected_continued);
        tokio::fs::remove_dir_all(path)
            .await
            .expect("remove test save");
    }

    #[test]
    fn sequence_seed_matches_26_3_random_sequence() {
        let mut random = Xoroshiro::from_sequence_seed(0, "minecraft:blocks/stone");
        assert_eq!(random.next_i64(), 2_256_863_541_085_145_868);
    }

    #[test]
    fn persisted_seed_defaults_control_new_sequences() {
        let saved = PersistedLootRandomSequences {
            salt: -7,
            include_world_seed: false,
            include_sequence_id: false,
            sequences: Vec::new(),
        };
        let mut sequences = LootRandomSequences::new(1234, saved);
        let mut expected = Xoroshiro::from_seed((-7_i64) as u64);
        assert_eq!(
            sequences
                .sequence(&Identifier::vanilla_static("blocks/stone"))
                .next_i64(),
            expected.next_i64()
        );
        let mut expected = Xoroshiro::from_seed((-7_i64) as u64);
        assert_eq!(
            sequences
                .sequence(&Identifier::vanilla_static("blocks/dirt"))
                .next_i64(),
            expected.next_i64()
        );
        let saved = sequences.save();
        assert_eq!(saved.salt, -7);
        assert!(!saved.include_world_seed && !saved.include_sequence_id);
    }

    #[test]
    fn mixed_draws_match_java_26_3_oracle() {
        // test_assets/loot_random/SequenceOracle.java, JDK 25, seed 0.
        let mut source = Xoroshiro::from_sequence_seed(0, "minecraft:blocks/stone");
        source.next_i64();
        let mut random = LootRandom::Xoroshiro(&mut source);
        for (bits, bounded) in [
            (1_054_887_710, 414_632_810),
            (1_058_359_051, 802_832_202),
            (1_027_773_600, 482_438_334),
            (1_064_124_762, 1_007_245_737),
        ] {
            assert_eq!(random.next_float().to_bits(), bits);
            assert_eq!(random.next_int(1_073_741_825), bounded);
        }
        let mut source = LegacyRandom::from_seed(1);
        let mut random = LootRandom::Legacy(&mut source);
        for (bits, bounded) in [
            (1_060_838_101, 215_764_588),
            (1_053_947_420, 874_970_313),
            (1_045_738_288, 77_814_904),
            (1_051_351_522, 13_136_569),
        ] {
            assert_eq!(random.next_float().to_bits(), bits);
            assert_eq!(random.next_int(1_073_741_825), bounded);
        }
    }

    #[test]
    fn world_loot_selects_explicit_seed_before_persisted_sequence() {
        init_vanilla_registry();
        let fixture = fresh_test_world("loot_random_selection");
        let table = &vanilla_loot_tables::BLOCKS_STONE;
        let drops = |seed, count| {
            fixture.world.with_loot_rng(table, seed, |random| {
                let mut context = LootContext::new(random).with_explosion(4.0);
                (0..count)
                    .flat_map(|_| table.get_random_items(&mut context))
                    .map(|stack| stack.count())
                    .sum::<i32>()
            })
        };
        // Explicit Java legacy seed 1 survives 20/64; it must not advance a named sequence.
        assert_eq!(drops(1, 64), 20);
        assert!(
            fixture
                .world
                .loot_random_sequences
                .read()
                .lock()
                .save()
                .sequences
                .is_empty()
        );
        let mut expected =
            Xoroshiro::from_sequence_seed(fixture.world.seed(), "minecraft:blocks/stone");
        let survived = (0..64).filter(|_| expected.next_f32() <= 0.25).count() as i32;
        assert_eq!(drops(0, 64), survived);
        let snapshot = fixture.world.loot_random_sequences.read().lock().save();
        let mut restored = LootRandomSequences::new(fixture.world.seed(), snapshot);
        assert_eq!(
            restored
                .sequence(table.random_sequence.as_ref().expect("sequence"))
                .next_i64(),
            expected.next_i64()
        );
    }

    #[test]
    fn dimensions_share_primary_sequences_but_not_level_randomness() {
        let domain = test_domain("loot_sequence_domain", &["primary", "derived"]);
        let primary = domain
            .get(&Identifier::new_static("loot_sequence_domain", "primary"))
            .expect("primary");
        let derived = domain
            .get(&Identifier::new_static("loot_sequence_domain", "derived"))
            .expect("derived");
        assert!(Arc::ptr_eq(
            &primary.loot_random_sequences.read(),
            &derived.loot_random_sequences.read()
        ));
        assert!(!std::ptr::eq(&primary.level_random, &derived.level_random));
        let key = Identifier::vanilla_static("blocks/stone");
        let mut expected = Xoroshiro::from_sequence_seed(primary.seed(), &key.to_string());
        assert_eq!(
            primary
                .loot_random_sequences
                .read()
                .lock()
                .sequence(&key)
                .next_i64(),
            expected.next_i64()
        );
        assert_eq!(
            derived
                .loot_random_sequences
                .read()
                .lock()
                .sequence(&key)
                .next_i64(),
            expected.next_i64()
        );
    }
}
