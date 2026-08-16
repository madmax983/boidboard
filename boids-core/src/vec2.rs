//! Two-dimensional vector arithmetic.
//!
//! Every operation here is **total**: no input produces `NaN` or a panic.
//! That is a hard requirement rather than a nicety, because a single `NaN`
//! introduced by a degenerate steering case (coincident agents, a zero-length
//! heading) propagates through the whole flock within one tick and is
//! unrecoverable.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

/// A 2D vector of `f64`s.
///
/// `f64` throughout: the simulation's reproducibility contract compares state
/// hashes bit-for-bit, and `f32` accumulation error would make long runs
/// diverge between otherwise identical executions.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f64,
    /// Vertical component.
    pub y: f64,
}

impl Vec2 {
    /// The origin, and the additive identity.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    /// Construct a vector from its components.
    #[must_use]
    pub fn new(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    /// Component-wise sum.
    ///
    /// The inherent form exists alongside [`std::ops::Add`] because chained
    /// force accumulation reads better as a method chain than as a pile of
    /// parenthesised operators.
    #[must_use]
    #[expect(
        clippy::should_implement_trait,
        reason = "Add is also implemented; both spellings are part of the API"
    )]
    pub fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }

    /// Component-wise difference, `self - o`.
    #[must_use]
    #[expect(
        clippy::should_implement_trait,
        reason = "Sub is also implemented; both spellings are part of the API"
    )]
    pub fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }

    /// Multiply both components by a scalar.
    #[must_use]
    pub fn scale(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// Euclidean length.
    ///
    /// Uses `hypot`, which is exact where `sqrt(x*x + y*y)` would overflow to
    /// infinity (any component above ~1e154) or flush to zero for subnormals.
    /// Prefer [`Vec2::length_squared`] on hot paths that only compare.
    #[must_use]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// Squared length. Cheaper than [`Vec2::length`] and sufficient for
    /// radius comparisons, which is how neighbour queries should use it.
    #[must_use]
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Unit vector in the same direction.
    ///
    /// **Total.** Returns [`Vec2::ZERO`] when the length is zero or the input
    /// is non-finite, and never returns `NaN`. Callers therefore never need a
    /// zero check before normalising a difference of two positions.
    #[must_use]
    pub fn normalize(self) -> Vec2 {
        let len = self.length();
        if len == 0.0 || !len.is_finite() {
            return Vec2::ZERO;
        }
        // Divide rather than multiply by a reciprocal: `1.0 / len` overflows
        // to infinity for subnormal lengths, which would reintroduce
        // non-finite output on exactly the inputs this guard exists for.
        Vec2::new(self.x / len, self.y / len)
    }

    /// Clamp the magnitude to `max`, preserving direction.
    ///
    /// Vectors already at or below `max` are returned **unchanged** — bit
    /// identical, not merely close — so that a clamp applied to an
    /// under-speed agent cannot perturb a reproducible run. This is the
    /// primitive behind both the `max_speed` and `max_force` invariants.
    ///
    /// **Total.** A non-finite input, a `NaN` cap, or a negative cap all
    /// yield [`Vec2::ZERO`] rather than propagating a poisoned value.
    #[must_use]
    pub fn limit(self, max: f64) -> Vec2 {
        if max.is_nan() || max < 0.0 {
            return Vec2::ZERO;
        }
        let len = self.length();
        if !len.is_finite() {
            return Vec2::ZERO;
        }
        if len <= max {
            return self;
        }
        // `len > max >= 0` and `len` is finite, so the ratio is a finite
        // value in [0,1) and cannot introduce a non-finite component.
        self.scale(max / len)
    }

    /// True when both components are finite (neither `NaN` nor infinite).
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::add(self, o)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::sub(self, o)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        self.scale(k)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        *self = Vec2::add(*self, o);
    }
}

#[cfg(test)]
mod tests {
    use super::Vec2;
    use crate::rng::Rng;

    /// Exact arithmetic on these operations is expected; the epsilon only
    /// absorbs the final rounding of `sqrt`/`hypot`.
    const EPS: f64 = 1e-12;

    fn assert_close(a: Vec2, b: Vec2) {
        assert!(
            (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS,
            "{a:?} != {b:?}"
        );
    }

    /// Assert `after` points the same way as `before`: zero cross product
    /// (parallel) and positive dot product (not merely antiparallel).
    fn assert_same_direction(before: Vec2, after: Vec2) {
        let cross = before.x * after.y - before.y * after.x;
        assert!(cross.abs() < 1e-9, "direction changed: {before:?} -> {after:?}");
        assert!(
            before.dot(after) > 0.0,
            "direction flipped: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn zero_is_the_additive_identity() {
        assert_eq!(Vec2::ZERO, Vec2::new(0.0, 0.0));
        assert_eq!(Vec2::ZERO, Vec2::default());
        let v = Vec2::new(3.0, -4.0);
        assert_eq!(v.add(Vec2::ZERO), v);
        assert_eq!(v.sub(Vec2::ZERO), v);
    }

    #[test]
    fn add_sub_scale_dot_are_correct() {
        let a = Vec2::new(3.0, -4.0);
        let b = Vec2::new(-1.5, 2.0);
        assert_eq!(a.add(b), Vec2::new(1.5, -2.0));
        assert_eq!(a.sub(b), Vec2::new(4.5, -6.0));
        assert_eq!(a.scale(2.0), Vec2::new(6.0, -8.0));
        assert_eq!(a.scale(0.0), Vec2::ZERO);
        assert_eq!(a.dot(b), 3.0 * -1.5 + -4.0 * 2.0);
        assert_eq!(a.dot(b), b.dot(a));
    }

    #[test]
    fn length_matches_the_three_four_five_triangle() {
        let v = Vec2::new(3.0, -4.0);
        assert_eq!(v.length(), 5.0);
        assert_eq!(v.length_squared(), 25.0);
        assert_eq!(Vec2::ZERO.length(), 0.0);
        assert_eq!(Vec2::ZERO.length_squared(), 0.0);
    }

    #[test]
    fn length_squared_is_the_square_of_length() {
        let mut r = Rng::seeded(11);
        for _ in 0..10_000 {
            let v = Vec2::new(r.range(-1e3, 1e3), r.range(-1e3, 1e3));
            let d = v.length() * v.length() - v.length_squared();
            assert!(d.abs() < 1e-6, "{v:?}: {d}");
        }
    }

    #[test]
    fn zero_normalize_is_exactly_zero_not_nan() {
        // AC-1's headline case: the classic NaN source in a boids kernel.
        let n = Vec2::ZERO.normalize();
        assert_eq!(n, Vec2::ZERO);
        assert!(!n.x.is_nan() && !n.y.is_nan());
    }

    #[test]
    fn non_finite_normalize_is_exactly_zero() {
        for v in [
            Vec2::new(f64::NAN, 0.0),
            Vec2::new(0.0, f64::NAN),
            Vec2::new(f64::NAN, f64::NAN),
            Vec2::new(f64::INFINITY, 0.0),
            Vec2::new(0.0, f64::NEG_INFINITY),
            Vec2::new(f64::INFINITY, f64::NEG_INFINITY),
        ] {
            assert_eq!(v.normalize(), Vec2::ZERO, "normalize({v:?}) must be zero");
        }
    }

    #[test]
    fn normalize_yields_unit_length_and_keeps_direction() {
        let mut r = Rng::seeded(23);
        for _ in 0..20_000 {
            let v = Vec2::new(r.range(-1e6, 1e6), r.range(-1e6, 1e6));
            if v.length() == 0.0 {
                continue;
            }
            let n = v.normalize();
            assert!((n.length() - 1.0).abs() < EPS, "{v:?} -> {n:?}");
            assert_same_direction(v, n);
        }
    }

    #[test]
    fn normalize_survives_extreme_magnitudes() {
        // A naive `sqrt(x*x + y*y)` overflows to infinity here and would
        // silently return ZERO for a perfectly well-defined direction.
        assert_eq!(Vec2::new(1e300, 0.0).normalize(), Vec2::new(1.0, 0.0));
        assert_eq!(Vec2::new(0.0, -1e300).normalize(), Vec2::new(0.0, -1.0));
        assert_eq!(Vec2::new(1e-300, 0.0).normalize(), Vec2::new(1.0, 0.0));
        // Subnormals cannot round-trip to unit length, but must stay finite.
        let sub = Vec2::new(5e-324, 5e-324).normalize();
        assert!(sub.is_finite(), "subnormal normalize went non-finite: {sub:?}");
    }

    #[test]
    fn limit_shortens_a_vector_over_the_limit() {
        // 3-4-5 triangle: length 5, limited to 2 -> length exactly 2 and
        // the same direction.
        let v = Vec2::new(3.0, -4.0);
        let l = v.limit(2.0);
        assert!((l.length() - 2.0).abs() < EPS, "{l:?}");
        assert_close(l, Vec2::new(1.2, -1.6));
    }

    #[test]
    fn limit_leaves_a_vector_exactly_at_the_limit_untouched() {
        let v = Vec2::new(3.0, -4.0);
        assert_eq!(v.limit(5.0), v, "a vector at the limit must not be scaled");
    }

    #[test]
    fn limit_leaves_a_shorter_vector_untouched() {
        let v = Vec2::new(0.3, -0.4);
        assert_eq!(v.limit(5.0), v);
        assert_eq!(Vec2::ZERO.limit(5.0), Vec2::ZERO);
    }

    #[test]
    fn limit_never_exceeds_the_limit() {
        // AC-2 as a property: whatever goes in, the length never exceeds the
        // cap. This is the invariant the speed/force clamps depend on.
        let mut r = Rng::seeded(41);
        for _ in 0..50_000 {
            let v = Vec2::new(r.range(-1e4, 1e4), r.range(-1e4, 1e4));
            let max = r.range(0.0, 500.0);
            let l = v.limit(max);
            assert!(
                l.length() <= max + EPS,
                "limit({max}) on {v:?} gave {l:?} of length {}",
                l.length()
            );
            assert!(l.is_finite(), "limit produced non-finite {l:?}");
        }
    }

    #[test]
    fn limit_preserves_direction_when_it_clamps() {
        let mut r = Rng::seeded(43);
        for _ in 0..10_000 {
            let v = Vec2::new(r.range(-1e3, 1e3), r.range(-1e3, 1e3));
            let l = v.limit(1.0);
            if v.length() == 0.0 {
                continue;
            }
            assert_same_direction(v, l);
        }
    }

    #[test]
    fn limit_to_zero_is_zero() {
        assert_eq!(Vec2::new(3.0, -4.0).limit(0.0), Vec2::ZERO);
    }

    #[test]
    fn limit_with_a_degenerate_maximum_stays_finite() {
        let v = Vec2::new(3.0, -4.0);
        // A NaN cap is a caller bug; swallowing it as ZERO keeps the totality
        // invariant, whereas `v * (NaN / len)` would poison the whole flock.
        assert_eq!(v.limit(f64::NAN), Vec2::ZERO);
        // A negative cap is unsatisfiable by any vector except the origin.
        assert_eq!(v.limit(-1.0), Vec2::ZERO);
        // An infinite cap constrains nothing.
        assert_eq!(v.limit(f64::INFINITY), v);
    }

    #[test]
    fn limit_of_a_degenerate_vector_stays_finite() {
        // A NaN must not be laundered into a "valid" clamped vector, and must
        // not escape as NaN either.
        assert_eq!(Vec2::new(f64::NAN, 1.0).limit(5.0), Vec2::ZERO);
        assert_eq!(Vec2::new(f64::INFINITY, 0.0).limit(5.0), Vec2::ZERO);
    }

    #[test]
    fn is_finite_detects_nan_and_infinity() {
        assert!(Vec2::ZERO.is_finite());
        assert!(Vec2::new(1e300, -1e300).is_finite());
        assert!(!Vec2::new(f64::NAN, 0.0).is_finite());
        assert!(!Vec2::new(0.0, f64::NAN).is_finite());
        assert!(!Vec2::new(f64::INFINITY, 0.0).is_finite());
        assert!(!Vec2::new(0.0, f64::NEG_INFINITY).is_finite());
    }

    #[test]
    fn operators_agree_with_the_named_methods() {
        let mut r = Rng::seeded(31);
        for _ in 0..5_000 {
            let a = Vec2::new(r.range(-100.0, 100.0), r.range(-100.0, 100.0));
            let b = Vec2::new(r.range(-100.0, 100.0), r.range(-100.0, 100.0));
            let k = r.range(-10.0, 10.0);
            assert_eq!(a + b, a.add(b));
            assert_eq!(a - b, a.sub(b));
            assert_eq!(a * k, a.scale(k));
            assert_eq!(-a, a.scale(-1.0));
            let mut acc = a;
            acc += b;
            assert_eq!(acc, a.add(b));
        }
    }

    #[test]
    fn negation_is_its_own_inverse() {
        let v = Vec2::new(2.5, -7.25);
        assert_eq!(-(-v), v);
        assert_close(v + -v, Vec2::ZERO);
    }

    #[test]
    fn serde_round_trips() {
        let v = Vec2::new(1.25, -8.5);
        let json = serde_json::to_string(&v).expect("serialize");
        let back: Vec2 = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, back);
    }
}
