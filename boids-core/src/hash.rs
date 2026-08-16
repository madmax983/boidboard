//! Canonical, cross-process state hashing.
//!
//! This hash is a **persisted reproducibility token**: it is stored in a
//! run's provenance and shown in the UI, so two executions of the same
//! scenario must produce the same value on any machine, in any process, in
//! any version of the toolchain. That rules out `std::hash::DefaultHasher`
//! entirely — it is randomly seeded per process, so it would produce a
//! different answer on every run of the same program.
//!
//! The scheme is therefore written out by hand and pinned by a golden test:
//!
//! 1. FNV-1a (64-bit) over an explicit little-endian byte stream.
//! 2. The agent count first, so flocks of different sizes cannot alias.
//! 3. Agents in ascending canonical order, so a permuted input array — for
//!    instance from a different neighbour backend — yields the same hash.
//! 4. `f64`s by their bit pattern, with `-0.0` and `NaN` canonicalised.

use crate::world::Agent;

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
/// The single quiet `NaN` all `NaN`s collapse to.
///
/// `NaN` has millions of bit patterns, and which one an operation produces is
/// not guaranteed across platforms; without this, a `NaN` in the state could
/// hash differently on two machines.
const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;

/// Bit pattern used to hash an `f64`.
///
/// `-0.0` and `0.0` compare equal and denote the same position, but their bit
/// patterns differ — and `World::wrap` can legitimately produce `-0.0`. Both
/// collapse to positive zero so that physically identical states hash alike.
fn canonical_bits(v: f64) -> u64 {
    if v == 0.0 {
        0
    } else if v.is_nan() {
        CANONICAL_NAN
    } else {
        v.to_bits()
    }
}

/// An agent reduced to the exact words that get hashed: `(id, pos.x, pos.y,
/// vel.x, vel.y)`, with the `f64`s already canonicalised to bit patterns.
///
/// Deriving `Ord` on this tuple is what defines "ascending canonical order".
type CanonicalKey = (u32, u64, u64, u64, u64);

/// The full canonical key of an agent: identity followed by state.
///
/// Doubles as the sort key. Ordering on the complete key rather than on `id`
/// alone keeps the hash permutation-invariant even if two agents were to
/// share an `id`, so the invariant does not silently depend on uniqueness.
fn canonical_key(a: &Agent) -> CanonicalKey {
    (
        a.id,
        canonical_bits(a.pos.x),
        canonical_bits(a.pos.y),
        canonical_bits(a.vel.x),
        canonical_bits(a.vel.y),
    )
}

/// Absorb bytes into an FNV-1a accumulator.
fn absorb(h: u64, bytes: &[u8]) -> u64 {
    let mut h = h;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Hash agent state deterministically and reproducibly.
///
/// Stable across processes and platforms, and **invariant under permutation
/// of the input slice**: agents are hashed in ascending canonical order, so
/// two neighbour backends that visit agents in different orders still agree.
#[must_use]
pub fn state_hash(agents: &[Agent]) -> u64 {
    let mut order: Vec<CanonicalKey> = agents.iter().map(canonical_key).collect();
    order.sort_unstable();

    // Length first: without it, hashing is a plain byte-stream concatenation
    // and distinct flocks could in principle produce the same stream.
    let mut h = absorb(FNV_OFFSET, &(agents.len() as u64).to_le_bytes());
    for (id, px, py, vx, vy) in order {
        h = absorb(h, &id.to_le_bytes());
        for word in [px, py, vx, vy] {
            h = absorb(h, &word.to_le_bytes());
        }
    }
    h
}

/// Hex rendering of [`state_hash`], for display in the UI.
///
/// Always exactly 16 lowercase hex digits, zero-padded, so hashes line up
/// when compared side by side.
#[must_use]
pub fn state_hash_hex(agents: &[Agent]) -> String {
    format!("{:016x}", state_hash(agents))
}

#[cfg(test)]
mod tests {
    use super::{state_hash, state_hash_hex};
    use crate::rng::Rng;
    use crate::vec2::Vec2;
    use crate::world::Agent;

    fn agent(id: u32, px: f64, py: f64, vx: f64, vy: f64) -> Agent {
        Agent {
            id,
            pos: Vec2::new(px, py),
            vel: Vec2::new(vx, vy),
        }
    }

    fn flock() -> Vec<Agent> {
        vec![
            agent(0, 1.0, 2.0, 0.5, -0.5),
            agent(1, 10.25, -3.5, -1.0, 2.0),
            agent(2, 99.75, 0.0, 0.0, 1.5),
            agent(3, 50.0, 50.0, -2.5, -2.5),
        ]
    }

    /// Fisher-Yates using the kernel's own RNG, so the shuffle is itself
    /// reproducible and a failure can be re-run.
    fn shuffled(agents: &[Agent], seed: u64) -> Vec<Agent> {
        let mut out = agents.to_vec();
        let mut r = Rng::seeded(seed);
        for i in (1..out.len()).rev() {
            let j = (r.next_u64() % (i as u64 + 1)) as usize;
            out.swap(i, j);
        }
        out
    }

    #[test]
    fn hash_is_stable_for_the_same_input() {
        let a = flock();
        assert_eq!(state_hash(&a), state_hash(&a));
        assert_eq!(state_hash(&a), state_hash(&flock()));
    }

    #[test]
    fn hash_is_permutation_invariant() {
        // AC-6: order-independent in representation. A neighbour backend that
        // returns agents in a different order must not change the hash.
        let base = flock();
        let expected = state_hash(&base);
        for seed in 0..64 {
            let mixed = shuffled(&base, seed);
            assert_eq!(
                state_hash(&mixed),
                expected,
                "permutation changed the hash (seed {seed}): {mixed:?}"
            );
        }
    }

    #[test]
    fn hash_is_permutation_invariant_for_a_large_flock() {
        let mut r = Rng::seeded(1234);
        let base: Vec<Agent> = (0..500)
            .map(|id| {
                agent(
                    id,
                    r.range(0.0, 100.0),
                    r.range(0.0, 100.0),
                    r.range(-5.0, 5.0),
                    r.range(-5.0, 5.0),
                )
            })
            .collect();
        let expected = state_hash(&base);
        for seed in 0..16 {
            assert_eq!(state_hash(&shuffled(&base, seed)), expected);
        }
    }

    #[test]
    fn hash_changes_when_any_single_field_changes() {
        // AC-6: sensitive to every field, each tested independently.
        let base = flock();
        let expected = state_hash(&base);

        let mut m = base.clone();
        m[2].pos.x += 1e-9;
        assert_ne!(state_hash(&m), expected, "pos.x change was not detected");

        let mut m = base.clone();
        m[2].pos.y += 1e-9;
        assert_ne!(state_hash(&m), expected, "pos.y change was not detected");

        let mut m = base.clone();
        m[2].vel.x += 1e-9;
        assert_ne!(state_hash(&m), expected, "vel.x change was not detected");

        let mut m = base.clone();
        m[2].vel.y += 1e-9;
        assert_ne!(state_hash(&m), expected, "vel.y change was not detected");

        let mut m = base.clone();
        m[2].id = 77;
        assert_ne!(state_hash(&m), expected, "id change was not detected");
    }

    #[test]
    fn hash_detects_a_one_ulp_change_in_every_agent_and_field() {
        // Every agent, not just a chosen one: catches an implementation that
        // hashes only the first or last element.
        let base = flock();
        let expected = state_hash(&base);
        for i in 0..base.len() {
            for field in 0..4 {
                let mut m = base.clone();
                let slot = match field {
                    0 => &mut m[i].pos.x,
                    1 => &mut m[i].pos.y,
                    2 => &mut m[i].vel.x,
                    _ => &mut m[i].vel.y,
                };
                *slot = f64::from_bits(slot.to_bits() ^ 1);
                assert_ne!(
                    state_hash(&m),
                    expected,
                    "one-ulp change to agent {i} field {field} was not detected"
                );
            }
        }
    }

    #[test]
    fn hash_distinguishes_different_flock_sizes() {
        let base = flock();
        let mut longer = base.clone();
        longer.push(agent(4, 0.0, 0.0, 0.0, 0.0));
        assert_ne!(state_hash(&base), state_hash(&longer));
        assert_ne!(state_hash(&[]), state_hash(&base));
    }

    #[test]
    fn hash_of_an_empty_flock_is_defined() {
        // Must not panic and must be stable; a run can legitimately have
        // zero agents.
        assert_eq!(state_hash(&[]), state_hash(&[]));
    }

    #[test]
    fn hash_treats_negative_zero_as_zero() {
        // `-0.0 == 0.0` is the same point, but `to_bits()` differs. `wrap()`
        // can legitimately produce `-0.0`, so without canonicalisation two
        // physically identical flocks would hash differently and a resumed
        // run would look like a divergence.
        let a = [agent(0, 0.0, 0.0, 0.0, 0.0)];
        let b = [agent(0, -0.0, -0.0, -0.0, -0.0)];
        assert_eq!(a[0].pos.x, b[0].pos.x, "precondition: -0.0 == 0.0");
        assert_eq!(
            state_hash(&a),
            state_hash(&b),
            "negative zero must hash identically to positive zero"
        );
    }

    #[test]
    fn hash_is_pinned_to_a_known_value() {
        // A golden value: this is a persisted reproducibility token, so a
        // change to the hashing scheme must be a deliberate, visible edit
        // rather than an accident that silently invalidates stored runs.
        // Derived from the documented canonical encoding by an independent
        // implementation, not captured from this crate's output.
        assert_eq!(
            state_hash(&flock()),
            0x68ec_a323_7979_8a40,
            "state_hash changed; stored provenance hashes would be invalidated"
        );
        assert_eq!(state_hash(&[]), 0xa8c7_f832_281a_39c5);
    }

    #[test]
    fn hex_matches_the_numeric_hash() {
        let a = flock();
        assert_eq!(state_hash_hex(&a), format!("{:016x}", state_hash(&a)));
    }

    #[test]
    fn hex_is_sixteen_lowercase_hex_digits() {
        // Fixed width matters: the UI shows these side by side.
        for n in 0..8 {
            let a: Vec<Agent> = (0..n).map(|i| agent(i, 1.0, 2.0, 3.0, 4.0)).collect();
            let hex = state_hash_hex(&a);
            assert_eq!(hex.len(), 16, "{hex} is not 16 chars");
            assert!(
                hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
                "{hex} is not lowercase hex"
            );
        }
    }

    #[test]
    fn distinct_states_mostly_produce_distinct_hashes() {
        // A weak collision check over many states: a hash that ignored a
        // field would collapse these into far fewer buckets.
        let mut r = Rng::seeded(4242);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..20_000 {
            let a = [
                agent(0, r.range(0.0, 100.0), r.range(0.0, 100.0), 0.0, 0.0),
                agent(1, r.range(0.0, 100.0), r.range(0.0, 100.0), 0.0, 0.0),
            ];
            seen.insert(state_hash(&a));
        }
        assert!(seen.len() > 19_990, "too many collisions: {}", seen.len());
    }
}
