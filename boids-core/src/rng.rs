//! Seeded, self-contained pseudo-random number generation.
//!
//! The kernel deliberately implements its own PRNG rather than depending on
//! `rand`: reproducibility is a product requirement (a stored seed must
//! regenerate a run exactly), and that means the bit-level algorithm is part
//! of the crate's contract, not an upgradable dependency.

/// Golden-ratio odd increment from the SplitMix64 reference implementation.
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;
/// First mixing multiplier from the SplitMix64 reference implementation.
const MIX_1: u64 = 0xbf58_476d_1ce4_e5b9;
/// Second mixing multiplier from the SplitMix64 reference implementation.
const MIX_2: u64 = 0x94d0_49bb_1331_11eb;

/// `f64` has a 53-bit significand, so 53 random bits scaled by 2^-53 covers
/// [0,1) with uniform spacing and can never round up to 1.0.
const F64_BITS: u32 = 53;
/// 2^-53, written as a literal because it must be exact.
const F64_SCALE: f64 = 1.0 / 9_007_199_254_740_992.0;

/// A deterministic SplitMix64 generator.
///
/// The same seed yields the same sequence in every process, on every
/// platform: the state is a `u64` and every step is wrapping integer
/// arithmetic, so there is no float, no address, and no ambient entropy
/// anywhere in the pipeline.
#[derive(Debug, Clone)]
pub struct Rng {
    /// The full generator state; advanced by `GAMMA` on every draw.
    state: u64,
}

impl Rng {
    /// Create a generator from an explicit seed. Every seed is valid,
    /// including zero.
    #[must_use]
    pub fn seeded(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// Draw the next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(MIX_1);
        z = (z ^ (z >> 27)).wrapping_mul(MIX_2);
        z ^ (z >> 31)
    }

    /// Draw a uniform `f64` in `[0,1)`.
    ///
    /// The upper bound is exclusive by construction, which callers rely on
    /// when scaling into a half-open range.
    pub fn next_f64(&mut self) -> f64 {
        // Take the high bits: SplitMix64's low bits are as good as its high
        // bits, but using the high ones matches the conventional recipe.
        let bits = self.next_u64() >> (u64::BITS - F64_BITS);
        bits as f64 * F64_SCALE
    }

    /// Draw a uniform `f64` in `[lo,hi)`.
    ///
    /// An empty span (`lo == hi`) returns `lo`; a reversed span is not
    /// meaningful and simply produces values in `(hi,lo]`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    /// Golden vectors taken from the published SplitMix64 reference
    /// (Vigna, public domain), not from this implementation's output. They
    /// pin the byte-exact sequence so a refactor cannot silently change it.
    const SEED_0: [u64; 4] = [
        0xe220_a839_7b1d_cdaf,
        0x6e78_9e6a_a1b9_65f4,
        0x06c4_5d18_8009_454f,
        0xf88b_b8a8_724c_81ec,
    ];
    const SEED_1: [u64; 4] = [
        0x910a_2dec_8902_5cc1,
        0xbeeb_8da1_658e_ec67,
        0xf893_a2ee_fb32_555e,
        0x71c1_8690_ee42_c90b,
    ];

    #[test]
    fn matches_published_splitmix64_vectors() {
        let mut r = Rng::seeded(0);
        for expected in SEED_0 {
            assert_eq!(r.next_u64(), expected);
        }
        let mut r = Rng::seeded(1);
        for expected in SEED_1 {
            assert_eq!(r.next_u64(), expected);
        }
    }

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut a = Rng::seeded(0xDEAD_BEEF);
        let mut b = Rng::seeded(0xDEAD_BEEF);
        for _ in 0..10_000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Rng::seeded(1);
        let mut b = Rng::seeded(2);
        let differing = (0..64).filter(|_| a.next_u64() != b.next_u64()).count();
        assert_eq!(differing, 64, "two seeds must not share a prefix");
    }

    #[test]
    fn next_f64_is_in_unit_interval() {
        for seed in 0..16u64 {
            let mut r = Rng::seeded(seed);
            for _ in 0..50_000 {
                let x = r.next_f64();
                assert!((0.0..1.0).contains(&x), "next_f64 out of [0,1): {x}");
            }
        }
    }

    #[test]
    fn next_f64_is_roughly_uniform() {
        // A weak distribution check: catches a scale/shift blunder without
        // pretending to be a statistical test suite.
        let mut r = Rng::seeded(7);
        let n = 200_000;
        let mut sum = 0.0;
        let mut buckets = [0usize; 10];
        for _ in 0..n {
            let x = r.next_f64();
            sum += x;
            buckets[(x * 10.0) as usize] += 1;
        }
        let mean = sum / f64::from(n);
        assert!((mean - 0.5).abs() < 0.01, "mean {mean} far from 0.5");
        for (i, count) in buckets.iter().enumerate() {
            let share = *count as f64 / f64::from(n);
            assert!(share > 0.08 && share < 0.12, "bucket {i} share {share}");
        }
    }

    #[test]
    fn range_stays_within_bounds() {
        let mut r = Rng::seeded(99);
        for _ in 0..100_000 {
            let x = r.range(-40.0, 12.5);
            assert!((-40.0..12.5).contains(&x), "range out of bounds: {x}");
        }
    }

    #[test]
    fn range_with_empty_span_is_the_endpoint() {
        let mut r = Rng::seeded(3);
        for _ in 0..1_000 {
            assert_eq!(r.range(5.0, 5.0), 5.0);
        }
    }
}
