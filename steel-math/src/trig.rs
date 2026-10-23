//! Port of Mojang `net.minecraft.util.Mth`'s float trig lookup tables.
//!
//! Carvers, structures, entity physics, and a lot of other gameplay code call
//! `Mth.sin(float)` / `Mth.cos(float)`, not `Math.sin(double)`. Matching
//! block-level determinism with vanilla requires using the same 65536-entry
//! `float` table instead of the precise FPU sine.
//!
//! # Vanilla reference
//! ```java
//! private static final float[] SIN = new float[65536];
//! static {
//!     for (int i = 0; i < 65536; i++)
//!         SIN[i] = (float) Math.sin(i * (Math.PI * 2.0 / 65536.0));
//! }
//! public static float sin(double i)  { return SIN[(int)((long)(i * 10430.378350470453) & 65535L)]; }
//! public static float cos(double i)  { return SIN[(int)((long)(i * 10430.378350470453 + 16384.0) & 65535L)]; }
//! ```
//! Note that the multiplier `10430.378350470453` is exactly
//! `65536 / (2π)` rounded to a double, and the table is indexed by the
//! low 16 bits of the scaled angle.

use std::{
    array::from_fn,
    f64::consts::{FRAC_PI_2, PI},
    mem::swap,
    sync::LazyLock,
};

/// `65536 / (2π)` — Mojang's stored constant.
const INDEX_SCALE: f64 = 10_430.378_350_470_453;
const TABLE_LEN: usize = 65_536;
const TABLE_MASK: i64 = 0xFFFF;

/// The `Mth.SIN` table (one entry per 2π / 65536 radians, float-valued).
static SIN_TABLE: LazyLock<Box<[f32; TABLE_LEN]>> = LazyLock::new(|| {
    // Box on the heap to keep the binary's .bss section small.
    let mut table: Box<[f32; TABLE_LEN]> = vec![0.0_f32; TABLE_LEN]
        .into_boxed_slice()
        .try_into()
        .expect("65536-element vec");
    for (i, slot) in table.iter_mut().enumerate() {
        *slot = (i as f64 / INDEX_SCALE).sin() as f32;
    }
    table
});

/// Vanilla `Mth.sin(double)` — returns the float from the 65536-entry sine
/// table indexed by the angle's low-16-bit bucket.
#[inline]
#[must_use]
pub fn sin(angle: f64) -> f32 {
    let idx = (((angle * INDEX_SCALE) as i64) & TABLE_MASK) as usize;
    SIN_TABLE[idx]
}

/// Vanilla `Mth.cos(double)` — like [`sin`] but phase-shifted by +π/2 using
/// the same table.
#[inline]
#[must_use]
pub fn cos(angle: f64) -> f32 {
    let idx = (((angle * INDEX_SCALE + 16_384.0) as i64) & TABLE_MASK) as usize;
    SIN_TABLE[idx]
}

const FRAC_BIAS: f64 = f64::from_bits(4_805_340_802_404_319_232);
static ATAN2_TABLE: LazyLock<[(f64, f64); 257]> = LazyLock::new(|| {
    from_fn(|index| {
        let phi = (index as f64 / 256.0).asin();
        (phi, phi.cos())
    })
});

/// Approximates the angle using `Mth.atan2`'s lookup and correction steps.
#[must_use]
pub fn atan2(mut y: f64, mut x: f64) -> f64 {
    let squared_length = x * x + y * y;
    if squared_length.is_nan() {
        return f64::NAN;
    }

    let negative_y = y < 0.0;
    if negative_y {
        y = -y;
    }
    let negative_x = x < 0.0;
    if negative_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        swap(&mut x, &mut y);
    }

    let inverse = f64::from_bits(6_910_469_410_427_058_090 - (squared_length.to_bits() >> 1));
    let inverse = inverse * (1.5 - 0.5 * squared_length * inverse * inverse);
    x *= inverse;
    y *= inverse;
    let biased_y = FRAC_BIAS + y;
    let index = biased_y.to_bits() as u32 as usize;
    let (phi, cosine) = ATAN2_TABLE[index];
    let sine = biased_y - FRAC_BIAS;
    let difference = y * cosine - x * sine;
    let mut theta = phi + (6.0 + difference * difference) * difference * 0.166_666_666_666_666_66;
    if steep {
        theta = FRAC_PI_2 - theta;
    }
    if negative_x {
        theta = PI - theta;
    }
    if negative_y {
        theta = -theta;
    }
    theta
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn atan2_matches_java_squid_rotation_inputs() {
        // Java 25, using the 26.3 Mth implementation and casting before RAD_TO_DEG.
        for (y, x, expected) in [
            (0.0, 0.0, 0x0000_0000),
            (1.0, 1.0, 0x4233_ffff),
            (0.123, 0.456, 0x4171_8764),
            (-0.123, 0.456, 0xc171_8764),
            (0.456, -0.123, 0x42d2_30ec),
            (-0.456, -0.123, 0xc2d2_30ec),
            (1.0e-6, 0.2, 0x3995_ef46),
            (0.2, 1.0e-6, 0x42b3_ffda),
        ] {
            assert_eq!((atan2(y, x) as f32 * crate::RAD_TO_DEG).to_bits(), expected);
        }
        assert!(atan2(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn sin_at_zero_is_zero() {
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
    }

    #[test]
    fn sin_matches_table_entries() {
        // Index 0 is sin(0), should be 0.
        // Index 16384 is sin(π/2), should be 1.
        // Verify a few well-known table entries.
        for &i in &[0usize, 1000, 16_384, 32_768, 49_152] {
            let expected = (i as f64 / INDEX_SCALE).sin() as f32;
            assert_eq!(SIN_TABLE[i], expected);
        }
    }

    #[test]
    fn cos_is_sin_shifted() {
        // cos(x) == sin(x + π/2) via the same table.
        let angle = 0.7;
        let expected = {
            let idx = (((angle * INDEX_SCALE + 16_384.0) as i64) & TABLE_MASK) as usize;
            SIN_TABLE[idx]
        };
        assert_eq!(cos(angle), expected);
    }
}
