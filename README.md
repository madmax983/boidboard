# Boidboard 🐦

A **durable boids experiment bench**. Configure a flocking scenario, submit it, and a
durable [Autumn Harvest](https://crates.io/crates/autumn-harvest) workflow executes the
simulation in checkpointed tick batches while the
[Autumn Web](https://crates.io/crates/autumn-web) UI shows it progressing, renders the
flock, and lets you compare runs.

## Why a workflow engine for something this fast

A boids run is quick — thousands of ticks a second — so durability here is not about
crash recovery alone. **The activity boundary is the checkpoint boundary**, and
checkpoints are what make the product work: resuming a killed run, scrubbing back
through history, steering parameters mid-flight, and cancelling a runaway run all fall
out of the same mechanism.

Two design rules follow, and everything else is downstream of them:

- **The workflow carries only a cursor** — `(run_id, next_tick)`, never the agent
  array. Workflow history stays O(1) in agent count.
- **Postgres owns the frames**, under `UNIQUE (run_id, tick)`. Harvest activities are
  at-least-once, so retries have to be idempotent; the schema makes them idempotent by
  construction rather than by careful coding.

## Architecture

| Crate | Role | Rule |
|---|---|---|
| `boids-core` | Pure simulation kernel | Zero dependencies on the web framework, workflow engine, or database — enforced by `boids-core/tests/purity.rs`, which checks the manifest against an allow-list. |
| `boidboard` | Autumn Web app + Harvest workflow | Owns persistence, routes, views, and orchestration. |

The kernel being genuinely pure is what makes the simulation test-drivable without any
infrastructure: 200+ of its tests run in a few seconds with no database and no network.

### The simulation

Classic Reynolds steering — separation, alignment, cohesion — plus the two
task-oriented behaviours, goal-seeking and obstacle avoidance, blended as a weighted
sum and clamped to a maximum force:

```
F = w_sep·F_separation + w_align·F_alignment + w_coh·F_cohesion
  + w_goal·F_goal + w_avoid·F_avoidance
```

The world is a **torus**, so every position difference goes through a minimum-image
displacement. Neighbour queries have two backends — brute-force O(N²) and a spatial
hash — and a property test asserts they return **exactly equal** neighbour sets across
randomised configurations, including agents straddling the wrap seam.

## Running it

Requires Rust 1.88+ and PostgreSQL.

```bash
createdb boidboard_dev
# point boidboard/autumn.toml at your database, then:
cd boidboard && AUTUMN_PROFILE=dev cargo run
```

Visit <http://localhost:3000/runs>. Migrations (the app's and Harvest's) apply
automatically on the `dev` profile. On any other profile, run them explicitly first.

```bash
cargo test --workspace                                    # everything
cargo test -p boids-core                                  # kernel only, no database needed
cargo clippy --workspace --all-targets -- -D warnings
```

The `boidboard` suite needs a live Postgres — nothing is `#[ignore]`d, because a
skipped test proves nothing.

## Reproducibility

Every run records a provenance fingerprint: kernel version, config hash, seed, and a
canonical `state_hash` over the final flock. The hash is deliberately boring in the
ways that matter — it sorts agents by id so a permuted array hashes identically, hashes
f64 bit patterns rather than using Rust's per-process-seeded hasher, and canonicalises
`-0.0`. Re-running from stored provenance reproduces it exactly, and a cross-process
test proves the same scenario hashes identically in a separate OS process.

Checkpointing rests on a contract the tests pin directly: **1 batch of 1000 ticks
produces the same state hash as 10 batches of 100 ticks**, with the state serialized and
deserialized in between. Making that true required carrying coordinates as exact decimal
strings — `serde_json`'s default float path is not a lossless `f64` channel, and the
naive version silently diverged by one ULP.

## How this was built

Test-first throughout, red → green → refactor, by a team of agents working in parallel
on disjoint modules. `docs/tdd/` holds the per-module logs with the real captured
failure output from every red phase; `docs/planning/` holds the brainstorming, reverse
brainstorming, and six-hats sessions that set the scope; `docs/ACCEPTANCE_CRITERIA.md`
is the specification the tests are written against.

Some of what the tests caught, which review alone would not have:

- Updating agents in place instead of double-buffering — the canonical boids bug.
- Neighbour lists ordered by slice index rather than agent id, which changes results
  because float addition is not associative.
- A spatial-hash cell block clamped at the grid edge, silently dropping neighbours
  across the seam.
- `f64::rem_euclid` returning exactly its modulus for tiny negative inputs, placing an
  agent in a grid cell that does not exist.
- An SVG decoder reading a different serialization shape than the workflow writes, so
  every real frame rendered as an empty flock while the tests drew boids happily.

## License

MIT
