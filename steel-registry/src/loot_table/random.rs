use rand::RngExt as _;

/// Draws used by loot evaluation, independent of a particular RNG implementation.
/// World-backed implementations preserve Java's float and bounded-integer draws.
pub trait LootRng {
    /// Draws a float in [0, 1).
    fn next_float(&mut self) -> f32;

    /// Draws an integer in [0, bound), where bound must be positive.
    fn next_int(&mut self, bound: i32) -> i32;
}

// Existing callers may still supply incidental runtime randomness directly.
impl<R: rand::Rng> LootRng for R {
    fn next_float(&mut self) -> f32 {
        self.random()
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        self.random_range(0..bound)
    }
}
