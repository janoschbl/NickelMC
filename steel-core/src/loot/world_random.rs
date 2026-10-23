use crate::loot::LootTable;
use steel_utils::random::{RandomSource, legacy_random::LegacyRandom};

use super::generation_guard::LootGenerationGuard;
use super::{LootError, LootResult};
use crate::world::World;

impl World {
    /// Selects the saved seed, domain sequence, or live world stream in vanilla order.
    /// Generation and container placement must both complete inside the callback.
    /// Concurrent draws serialize. Same-thread recursion is a recoverable error;
    /// nested tables must reuse the callback's RNG through their LootContext.
    pub fn with_loot_random<T>(
        &self,
        table: &LootTable,
        seed: i64,
        use_random: impl FnOnce(&mut RandomSource) -> LootResult<T>,
    ) -> LootResult<T> {
        if seed != 0 {
            let _generation = LootGenerationGuard::enter()?;
            return use_random(&mut RandomSource::Legacy(LegacyRandom::from_seed(
                seed as u64,
            )));
        }
        if let Some(key) = &table.random_sequence {
            let sequences = self.random_sequences.read().as_ref().cloned().ok_or(
                LootError::RandomUnavailable("world is not bound to its domain sequences"),
            )?;
            return sequences.with_random(key, use_random);
        }
        let _generation = LootGenerationGuard::enter()?;
        let mut random = self.loot_random.lock();
        use_random(&mut random)
    }
}
