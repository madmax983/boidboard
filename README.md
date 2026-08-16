# boidboard

A chess engine in Rust whose evaluation function is a **boids flocking model**: pieces are
agents, a piece's neighbourhood is the set of squares it attacks and the set of pieces
attacking it, and the position's value emerges from a small number of forces over that
neighbourhood rather than from a hand-tuned table of terms.

The geometry is **discrete and attack-set-based** — 64 squares, fixed-point integers. It is
not a continuous flocking simulation.

Whether the idea is any good is an open question, and the project is arranged so that the
question gets a real answer: the boids evaluator is measured against a conventional
control evaluator through an SPRT decision gate (issue #13), not declared successful.

## Status

Phase 0. The workspace is scaffolded and the external oracle is committed; there is no
engine yet. Phase 1 (issues #4, #5, #6) implements board representation, magic bitboard
move generation, and the perft harness.

## The perft oracle

`tests/fixtures/perft_oracle.txt` holds published node counts for the six standard perft
positions, transcribed from the [Chess Programming Wiki][cpw] and committed **before any
engine code existed**. That ordering is the point: the standard this engine will be judged
against was fixed before there was an implementation that could have shaped it.

Each count carries a provenance flag — `v` for counts this project independently re-derived
with Stockfish, `p` for counts that are published but too deep to replay here. The fixture
is embedded with `include_str!`, so deleting or renaming it is a compile error, and its
SHA-256 is pinned in the test source, so editing a digit is a test failure.

[cpw]: https://www.chessprogramming.org/Perft_Results

## Prerequisites

Rust is pinned by `rust-toolchain.toml`; `rustup` will honour it automatically.

Stockfish is used as an external oracle by the differential tests:

```sh
./scripts/setup-stockfish.sh
```

The script installs the package, symlinks it onto `PATH` (Ubuntu's package installs only
to `/usr/games`, which is not always on `PATH`), and verifies that
`perft 3` of the initial position reports 8902 nodes.

Without Stockfish the differential tests skip loudly rather than failing, so
`cargo test --workspace` still passes on a clean checkout. CI sets
`BOIDBOARD_REQUIRE_STOCKFISH=1`, which turns that skip into a hard failure.

To run the same check on every Claude Code session, add a `SessionStart` hook to
`.claude/settings.json` — deliberately not committed (D-0014):

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "./scripts/setup-stockfish.sh" } ] }
    ]
  }
}
```

## Building and testing

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The differential test's replay budget is tunable:

```sh
BOID_PERFT_MAX_NODES=200000000 cargo test --workspace -- --nocapture
```

## Workspace layout

| Crate | Phase | Purpose |
|---|---|---|
| `boid-board` | 1 | Board representation, move generation, the perft oracle. No dependencies. |
| `boid-search` | 2–3 | PVS, move ordering, LMR. Generic over the evaluator. |
| `boid-eval-classical` | 4 | Conventional evaluation — the control arm and Elo anchor. |
| `boid-eval-boids` | 5–6 | The boids force-field evaluation — the experiment. |
| `boid-uci` | 2 | UCI protocol, time management, the `boidboard` binary. Composition root. |
| `boid-web` | 8 | Analysis board with the live force overlay. |
| `boid-tune` | 7–9 | Texel tuning, the ablation matrix, the SPRT gauntlet. |

`boid-search` never depends on a concrete evaluator: evaluator selection is a runtime UCI
option, so an ablation campaign is a loop over options rather than a loop over builds.

## Decisions

`docs/DECISIONS.md` is an append-only log of the binding decisions, including the two rules
issue #3 requires be recorded: none of the archived prior attempt's code may be reused
(D-0001), and this project may not author its own acceptance criteria in-repo before the
perft harness is green (D-0002).

## Licence

MIT. See `LICENSE`.
