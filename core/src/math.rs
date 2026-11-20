//! Math: fungsi skalar dan generator angka acak.
//! Operasi satu instruksi seperti abs, min, max, floor, ceil, dan round tetap di JavaScript.

use std::sync::atomic::{AtomicU64, Ordering};

static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// xorshift64*. Cukup untuk permainan dan efek visual, bukan untuk kriptografi.
pub fn next_u64() -> u64 {
    let mut x = SEED.load(Ordering::Relaxed);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    SEED.store(x, Ordering::Relaxed);
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn random_int(a: i32, b: i32) -> i32 {
    let span = (b as i64 - a as i64 + 1).max(1) as u64;
    a.wrapping_add((next_u64() % span) as i32)
}

#[no_mangle]
pub extern "C" fn fb_sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[no_mangle]
pub extern "C" fn fb_pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}

#[no_mangle]
pub extern "C" fn fb_sin(x: f64) -> f64 {
    x.sin()
}

#[no_mangle]
pub extern "C" fn fb_cos(x: f64) -> f64 {
    x.cos()
}

#[no_mangle]
pub extern "C" fn fb_clamp(v: f64, lo: f64, hi: f64) -> f64 {
    clamp(v, lo, hi)
}

#[no_mangle]
pub extern "C" fn fb_lerp(a: f64, b: f64, t: f64) -> f64 {
    lerp(a, b, t)
}

#[no_mangle]
pub extern "C" fn fb_random() -> f64 {
    (next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

#[no_mangle]
pub extern "C" fn fb_random_int(a: i32, b: i32) -> i32 {
    random_int(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_and_lerp() {
        assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
    }

    #[test]
    fn random_int_stays_in_range() {
        for _ in 0..1000 {
            let v = random_int(3, 7);
            assert!((3..=7).contains(&v));
        }
    }

    #[test]
    fn random_is_unit_interval() {
        for _ in 0..1000 {
            let v = (next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            assert!((0.0..1.0).contains(&v));
        }
    }
}
