//! The toroidal world the flock lives in.

use crate::vec2::Vec2;

/// A single simulated agent.
///
/// Position and velocity only: acceleration is recomputed from scratch each
/// tick from the steering forces, so it is not state and must not be stored
/// here — anything kept on this struct is part of the reproducibility hash
/// and of every persisted frame.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    /// Stable identity, unique within a run.
    ///
    /// Identity is what makes frames comparable across ticks and what gives
    /// the canonical state hash a deterministic ordering, so it must not be
    /// reused or reassigned mid-run.
    pub id: u32,
    /// Position, expected to lie within the world bounds after wrapping.
    pub pos: Vec2,
    /// Velocity in world units per unit time.
    pub vel: Vec2,
}

/// A rectangular world with wrap-around (toroidal) topology.
///
/// There are no walls: an agent leaving the right edge re-enters at the left,
/// and every pair of points has a shortest connecting path that may cross a
/// seam. All geometry in the kernel goes through this type so that the
/// wrap-around is impossible to forget at a call site.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct World {
    /// Extent along x. Positions live in `[0, width)`.
    pub width: f64,
    /// Extent along y. Positions live in `[0, height)`.
    pub height: f64,
}

impl World {
    /// Construct a world of the given size.
    #[must_use]
    pub fn new(width: f64, height: f64) -> World {
        World { width, height }
    }

    /// Wrap a position into `[0,width) x [0,height)`.
    ///
    /// Correct for negative coordinates and for offsets many world-widths
    /// out (`rem_euclid` semantics), so an agent driven far off-world still
    /// lands in bounds in a single call. The interval is **half-open**: a
    /// coordinate of exactly `width` wraps to `0.0`.
    #[must_use]
    pub fn wrap(&self, p: Vec2) -> Vec2 {
        Vec2::new(wrap_axis(p.x, self.width), wrap_axis(p.y, self.height))
    }

    /// Minimum-image displacement **from `a` to `b`**: the shortest vector
    /// on the torus, which may point across a seam.
    ///
    /// In a width-100 world, `displacement((1,0), (99,0))` is `(-2,0)` — two
    /// units to the *left*, not 98 to the right. Each component is at most
    /// half the corresponding world dimension.
    ///
    /// Inputs need **not** be pre-wrapped; callers may pass raw positions.
    #[must_use]
    pub fn displacement(&self, a: Vec2, b: Vec2) -> Vec2 {
        Vec2::new(
            min_image_axis(b.x - a.x, self.width),
            min_image_axis(b.y - a.y, self.height),
        )
    }

    /// Toroidal distance between two points.
    ///
    /// `distance((1,0), (99,0))` is `2.0` in a width-100 world.
    #[must_use]
    pub fn distance(&self, a: Vec2, b: Vec2) -> f64 {
        self.displacement(a, b).length()
    }

    /// Squared toroidal distance.
    ///
    /// Prefer this over [`World::distance`] when comparing against a radius:
    /// it avoids a square root per pair, which dominates neighbour queries.
    #[must_use]
    pub fn distance_squared(&self, a: Vec2, b: Vec2) -> f64 {
        self.displacement(a, b).length_squared()
    }
}

/// Reduce a raw axis difference to its minimum image in `[-size/2, size/2]`.
///
/// Subtracting `size * round(d / size)` handles differences of arbitrary
/// magnitude in one step, so unwrapped inputs are fine. At exactly half the
/// world size both images are equally short; `f64::round` breaks that tie
/// away from zero, which is arbitrary but **deterministic**, and determinism
/// is what the reproducibility contract needs.
fn min_image_axis(d: f64, size: f64) -> f64 {
    if axis_is_degenerate(d, size) {
        return 0.0;
    }
    d - size * (d / size).round()
}

/// True when an axis calculation cannot produce a meaningful finite answer.
///
/// Covers a world that is not positively sized (where `rem_euclid` and the
/// minimum-image division both yield `NaN`) and a coordinate that is already
/// non-finite. Stated as an explicit finiteness test rather than
/// `!(size > 0.0)`, so the `NaN` case is named rather than implied.
fn axis_is_degenerate(v: f64, size: f64) -> bool {
    !size.is_finite() || size <= 0.0 || !v.is_finite()
}

/// Wrap a single coordinate into `[0, size)`.
///
/// Kept separate from [`World::wrap`] because both axes need identical
/// treatment of three edge cases that a bare `rem_euclid` gets wrong.
fn wrap_axis(v: f64, size: f64) -> f64 {
    // A degenerate axis collapses to the origin rather than poisoning
    // downstream state with a NaN.
    if axis_is_degenerate(v, size) {
        return 0.0;
    }
    let wrapped = v.rem_euclid(size);
    // `rem_euclid` can return exactly `size`: for a tiny negative `v` it
    // computes `v + size`, which rounds up to `size`. The half-open interval
    // is load-bearing (bucket indices, seam arithmetic), so close the gap.
    if wrapped >= size { 0.0 } else { wrapped }
}

#[cfg(test)]
mod tests {
    use super::World;
    use crate::rng::Rng;
    use crate::vec2::Vec2;

    const EPS: f64 = 1e-12;

    fn w100() -> World {
        World::new(100.0, 100.0)
    }

    #[test]
    fn new_stores_the_dimensions() {
        let w = World::new(640.0, 480.0);
        assert_eq!(w.width, 640.0);
        assert_eq!(w.height, 480.0);
    }

    #[test]
    fn wrap_leaves_in_bounds_positions_untouched() {
        let w = w100();
        for p in [
            Vec2::ZERO,
            Vec2::new(0.5, 99.5),
            Vec2::new(50.0, 50.0),
            Vec2::new(99.999, 0.001),
        ] {
            assert_eq!(w.wrap(p), p, "in-bounds position must not be perturbed");
        }
    }

    #[test]
    fn wrap_handles_negative_coordinates() {
        let w = w100();
        assert_eq!(w.wrap(Vec2::new(-1.0, -1.0)), Vec2::new(99.0, 99.0));
        assert_eq!(w.wrap(Vec2::new(-0.25, -99.5)), Vec2::new(99.75, 0.5));
    }

    #[test]
    fn wrap_handles_multi_world_width_offsets() {
        // AC-4's named cases: an agent driven far off-world still lands in
        // bounds, not merely one width closer to it.
        let w = w100();
        assert_eq!(w.wrap(Vec2::new(-250.0, 350.0)), Vec2::new(50.0, 50.0));
        assert_eq!(w.wrap(Vec2::new(350.0, -250.0)), Vec2::new(50.0, 50.0));
        assert_eq!(w.wrap(Vec2::new(-1_000_000.0, 1_000_000.0)), Vec2::ZERO);
    }

    #[test]
    fn wrap_upper_bound_is_exclusive() {
        let w = w100();
        assert_eq!(w.wrap(Vec2::new(100.0, 100.0)), Vec2::ZERO);
        assert_eq!(w.wrap(Vec2::new(200.0, -100.0)), Vec2::ZERO);
    }

    #[test]
    fn wrap_never_returns_the_upper_bound() {
        // A bare `rem_euclid` returns exactly `width` for tiny negative
        // inputs, because `-1e-18 + 100.0` rounds to `100.0`. That silently
        // breaks the half-open [0,width) invariant every spatial-hash bucket
        // index depends on.
        let w = w100();
        for x in [-1e-18, -1e-30, -f64::MIN_POSITIVE, -1e-300] {
            let p = w.wrap(Vec2::new(x, x));
            assert!(
                p.x < 100.0 && p.y < 100.0,
                "wrap({x:e}) returned the exclusive upper bound: {p:?}"
            );
            assert!(p.x >= 0.0 && p.y >= 0.0, "wrap({x:e}) went negative: {p:?}");
        }
    }

    #[test]
    fn wrap_result_is_always_in_bounds() {
        let mut r = Rng::seeded(101);
        for _ in 0..50_000 {
            let w = World::new(r.range(0.5, 500.0), r.range(0.5, 500.0));
            let p = Vec2::new(r.range(-5e4, 5e4), r.range(-5e4, 5e4));
            let q = w.wrap(p);
            assert!(
                q.x >= 0.0 && q.x < w.width && q.y >= 0.0 && q.y < w.height,
                "wrap({p:?}) in {w:?} gave out-of-bounds {q:?}"
            );
        }
    }

    #[test]
    fn wrap_is_idempotent() {
        let mut r = Rng::seeded(103);
        for _ in 0..20_000 {
            let w = World::new(r.range(1.0, 300.0), r.range(1.0, 300.0));
            let p = Vec2::new(r.range(-1e4, 1e4), r.range(-1e4, 1e4));
            let once = w.wrap(p);
            assert_eq!(w.wrap(once), once, "wrap must be idempotent");
        }
    }

    #[test]
    fn wrap_shifts_by_a_whole_number_of_world_widths() {
        // The wrapped point must be the *same* point on the torus, i.e. the
        // displacement is an exact multiple of the world size.
        let mut r = Rng::seeded(107);
        for _ in 0..10_000 {
            let w = World::new(100.0, 250.0);
            let p = Vec2::new(r.range(-1e3, 1e3), r.range(-1e3, 1e3));
            let q = w.wrap(p);
            let kx = (p.x - q.x) / w.width;
            let ky = (p.y - q.y) / w.height;
            assert!((kx - kx.round()).abs() < 1e-9, "{p:?} -> {q:?}: kx={kx}");
            assert!((ky - ky.round()).abs() < 1e-9, "{p:?} -> {q:?}: ky={ky}");
        }
    }

    #[test]
    fn wrap_of_a_degenerate_world_stays_finite() {
        // A zero-width world makes `rem_euclid` produce NaN; the kernel's
        // totality rule says we clamp rather than poison the state.
        let w = World::new(0.0, 0.0);
        let p = w.wrap(Vec2::new(5.0, -5.0));
        assert!(p.is_finite(), "degenerate world produced {p:?}");
    }

    #[test]
    fn wrap_of_a_non_finite_position_stays_finite() {
        let w = w100();
        for p in [
            Vec2::new(f64::NAN, 0.0),
            Vec2::new(f64::INFINITY, 0.0),
            Vec2::new(0.0, f64::NEG_INFINITY),
        ] {
            let q = w.wrap(p);
            assert!(q.is_finite(), "wrap({p:?}) produced {q:?}");
            assert!(q.x >= 0.0 && q.x < 100.0 && q.y >= 0.0 && q.y < 100.0);
        }
    }

    #[test]
    fn distance_across_the_x_seam_is_the_short_way_round() {
        // AC-3's named case: 2 apart across the seam, not 98 the long way.
        let w = w100();
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(99.0, 0.0);
        assert_eq!(w.distance(a, b), 2.0);
        assert_eq!(w.distance(b, a), 2.0);
        assert_eq!(w.distance_squared(a, b), 4.0);
    }

    #[test]
    fn distance_across_the_y_seam_is_the_short_way_round() {
        let w = w100();
        let a = Vec2::new(0.0, 1.0);
        let b = Vec2::new(0.0, 99.0);
        assert_eq!(w.distance(a, b), 2.0);
        assert_eq!(w.distance_squared(a, b), 4.0);
    }

    #[test]
    fn displacement_points_the_short_way_and_is_signed() {
        // Direction matters: FROM a TO b, so crossing the left seam is -2.
        let w = w100();
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(99.0, 0.0);
        assert_eq!(w.displacement(a, b), Vec2::new(-2.0, 0.0));
        assert_eq!(w.displacement(b, a), Vec2::new(2.0, 0.0));
    }

    #[test]
    fn displacement_across_a_diagonal_seam() {
        let w = w100();
        let a = Vec2::new(2.0, 3.0);
        let b = Vec2::new(98.0, 96.0);
        assert_eq!(w.displacement(a, b), Vec2::new(-4.0, -7.0));
        assert_eq!(w.distance(a, b), 65.0_f64.sqrt());
    }

    #[test]
    fn displacement_does_not_require_wrapped_inputs() {
        // Neighbour code should not have to remember to wrap first.
        let w = w100();
        let inside = w.displacement(Vec2::new(1.0, 0.0), Vec2::new(99.0, 0.0));
        let outside = w.displacement(Vec2::new(101.0, 0.0), Vec2::new(-1.0, 0.0));
        assert_eq!(inside, outside);
        assert_eq!(outside, Vec2::new(-2.0, 0.0));
    }

    #[test]
    fn distance_within_the_world_is_ordinary_euclidean() {
        let w = w100();
        let a = Vec2::new(10.0, 10.0);
        let b = Vec2::new(13.0, 14.0);
        assert_eq!(w.distance(a, b), 5.0);
        assert_eq!(w.displacement(a, b), Vec2::new(3.0, 4.0));
    }

    #[test]
    fn distance_to_self_is_zero() {
        let w = w100();
        let mut r = Rng::seeded(211);
        for _ in 0..1_000 {
            let p = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0));
            assert_eq!(w.distance(p, p), 0.0);
            assert_eq!(w.displacement(p, p), Vec2::ZERO);
        }
    }

    #[test]
    fn displacement_never_exceeds_half_the_world() {
        // The defining property of the minimum-image convention.
        let mut r = Rng::seeded(223);
        for _ in 0..50_000 {
            let w = World::new(r.range(1.0, 400.0), r.range(1.0, 400.0));
            let a = Vec2::new(r.range(-500.0, 500.0), r.range(-500.0, 500.0));
            let b = Vec2::new(r.range(-500.0, 500.0), r.range(-500.0, 500.0));
            let d = w.displacement(a, b);
            assert!(
                d.x.abs() <= w.width / 2.0 + EPS && d.y.abs() <= w.height / 2.0 + EPS,
                "displacement {d:?} exceeds half of {w:?}"
            );
        }
    }

    #[test]
    fn displacement_is_antisymmetric() {
        let mut r = Rng::seeded(227);
        for _ in 0..20_000 {
            let w = World::new(100.0, 250.0);
            let a = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 250.0));
            let b = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 250.0));
            let ab = w.displacement(a, b);
            let ba = w.displacement(b, a);
            assert!(
                (ab.x + ba.x).abs() < EPS && (ab.y + ba.y).abs() < EPS,
                "displacement not antisymmetric: {ab:?} vs {ba:?}"
            );
        }
    }

    #[test]
    fn stepping_along_the_displacement_arrives_at_the_target() {
        // The displacement really is a path from a to b on the torus.
        let mut r = Rng::seeded(229);
        for _ in 0..20_000 {
            let w = World::new(100.0, 250.0);
            let a = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 250.0));
            let b = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 250.0));
            let arrived = w.wrap(a.add(w.displacement(a, b)));
            let target = w.wrap(b);
            assert!(
                w.distance(arrived, target) < 1e-9,
                "a={a:?} b={b:?} arrived={arrived:?}"
            );
        }
    }

    #[test]
    fn distance_is_symmetric_and_matches_its_square() {
        let mut r = Rng::seeded(233);
        for _ in 0..20_000 {
            let w = World::new(r.range(1.0, 300.0), r.range(1.0, 300.0));
            let a = Vec2::new(r.range(0.0, 300.0), r.range(0.0, 300.0));
            let b = Vec2::new(r.range(0.0, 300.0), r.range(0.0, 300.0));
            let d = w.distance(a, b);
            assert!((d - w.distance(b, a)).abs() < EPS, "asymmetric distance");
            let ds = w.distance_squared(a, b);
            assert!((d * d - ds).abs() < 1e-9, "d={d} ds={ds}");
        }
    }

    #[test]
    fn distance_never_exceeds_the_half_diagonal() {
        let w = w100();
        let mut r = Rng::seeded(239);
        let max = (50.0_f64 * 50.0 + 50.0 * 50.0).sqrt();
        for _ in 0..20_000 {
            let a = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0));
            let b = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0));
            let d = w.distance(a, b);
            assert!(d <= max + EPS, "distance {d} exceeds half-diagonal {max}");
        }
    }

    #[test]
    fn distance_is_translation_invariant_on_the_torus() {
        // Shifting both points by the same offset cannot change how far
        // apart they are; this is what makes AC-28 achievable.
        let w = w100();
        let mut r = Rng::seeded(241);
        for _ in 0..20_000 {
            let a = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0));
            let b = Vec2::new(r.range(0.0, 100.0), r.range(0.0, 100.0));
            let off = Vec2::new(r.range(-300.0, 300.0), r.range(-300.0, 300.0));
            let before = w.distance(a, b);
            let after = w.distance(w.wrap(a.add(off)), w.wrap(b.add(off)));
            assert!((before - after).abs() < 1e-9, "{before} != {after}");
        }
    }

    #[test]
    fn distance_of_a_degenerate_world_stays_finite() {
        let w = World::new(0.0, 0.0);
        let d = w.distance(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0));
        assert!(d.is_finite(), "degenerate world produced distance {d}");
    }

    #[test]
    fn serde_round_trips() {
        let w = World::new(640.0, 480.0);
        let json = serde_json::to_string(&w).expect("serialize");
        let back: World = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(w, back);
    }

    #[test]
    fn wrap_preserves_exact_representable_values() {
        // Guards against an implementation that adds and subtracts the world
        // size, which would round and drift.
        let w = w100();
        let p = Vec2::new(33.333_333_333_333_336, 66.666_666_666_666_67);
        assert!((w.wrap(p).x - p.x).abs() < EPS);
        assert_eq!(w.wrap(p), p);
    }
}
