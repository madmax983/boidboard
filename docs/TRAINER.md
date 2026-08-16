# The trainer

A design sketch for an opening / tactics / endgame-theory trainer built on boidboard.

This document describes *structure and prerequisites*. It deliberately sets no pass/fail
standard for the project: under `docs/DECISIONS.md` D-0002 no in-repo document may define
or certify this project's own acceptance criteria until the perft harness is green. The
trainer's acceptance criteria live in its GitHub issues, which are upstream of the
repository.

## Why this belongs to *this* project

A trainer bolted onto a chess engine is normally a worse Lichess. This one has a reason to
exist that Lichess cannot copy: issue #15 builds `POST /why`, which decomposes an
evaluation into per-term boids forces and renders them as arrows and a Φ heatmap.

A conventional trainer tells you **what** the book move is, and — at best — attaches a
human-written comment explaining it. Ours can show **why** in the engine's own terms: the
force field before the move and after it, which force changed sign, which piece's
neighbourhood re-formed. The pedagogy and the debugging instrument are the same artefact.

The arrow points both ways, and that is the more interesting direction:

> The trainer is a test instrument for the thesis. Opening theory is a large corpus of
> moves that strong humans have already agreed are good, with reasons already articulated
> in words. If the boids evaluator's decomposition of *known-good* book moves is
> recognisable to a player — if "the knight comes to f3 because it joins the flock
> defending d4/e5" survives contact — that is evidence for the idea. If the explanations
> are noise on positions where the right answer is not in dispute, that is evidence
> against it, gathered from outside the SPRT gate and not reducible to Elo.

Issue #13's gate answers "does it play better?". The trainer is one of the few places that
can ask "does it *reason* in a way that corresponds to something?" — cheaply, on positions
whose evaluation is not controversial. That is worth having even if the answer is no.

## The three modes

| Mode | Content | The engine's job | Depends on |
|---|---|---|---|
| **Opening repertoire** | A PGN variation tree, one per colour | Play the opponent's replies from the tree; adjudicate yours | #4, #5, #6, SAN/PGN |
| **Tactics** | FEN + a forcing solution line | Adjudicate; verify the puzzle is sound | + #7, #9 (search) |
| **Endgame / theory** | Position + a goal ("win", "hold") | Play the defence, adjudicate the result | + repetition & 50-move state |

### Opening repertoire

Load a repertoire, pick a colour, and the trainer walks the tree: it plays the opponent's
moves and you must produce yours. A miss shows the move and requeues the card.

The part most trainers get wrong is **transposition**. A repertoire is written as a tree,
but it is really a DAG — 1.d4 Nf6 2.c4 e6 3.Nf3 and 1.Nf3 Nf6 2.c4 e6 3.d4 are the same
position, and drilling them as two unrelated cards is both wasted repetition and a missed
teaching moment. Keying cards on the **Zobrist key** rather than on the move path collapses
them exactly and for free, because issue #4 already builds the key and already thinks about
what makes two positions genuinely identical (its AC5 and AC6 are precisely this question).

One wrinkle to decide when it is built: the repertoire key should almost certainly exclude
the halfmove clock and fullmove number — two identical positions at different move numbers
are the same *lesson* — which is the key #4 produces, but the reasoning should be written
down rather than inherited by accident.

### Tactics

A puzzle is a FEN plus the solution line; you must play the whole forcing sequence, and the
trainer plays the defence.

Because there is an engine in the box, puzzles can be **verified rather than trusted**. A
puzzle with a second, equally winning first move is a broken puzzle, and a search can find
that. This is the same posture as `tests/fixtures/perft_oracle.txt`: third-party data,
checked by us, with the check recorded. A puzzle set that has been machine-audited for
uniqueness is a genuinely better artefact than the one it was derived from, and the audit
is a batch job over an engine we will have anyway.

### Endgame / theory

A position plus a goal — "win this", "hold this" — played out against the engine.

This mode surfaces a real consequence of a decision already taken. Issue #4 notes that
`Board` is `Copy` with no `unmake_move`, so "repetition and fifty-move detection require a
separately threaded zobrist history in the search stack" — which puts draw detection inside
the search, while a theory drill needs it in its own game loop. See the prerequisites
section below.

## What "once the movement stuff is built" actually requires

This is the load-bearing section, and the reason this document exists now rather than at
phase 10.

The trainer is genuinely not engine territory — but it is only *easy* later if phase 1 is
built with it in view. Checked against the issues as they are written today:

| Needs | Issue | Status |
|---|---|---|
| Position, FEN, Zobrist | #4 | Covered |
| Pseudo-legal move generation | #5 | Covered |
| A legal move list as a callable entry point | #6 | Covered, and better than needed — AC6 requires *one* `generate_legal_moves` with no test-only variant |
| Search, for tactics adjudication and endgame defence | #7, #9 | Covered |
| `POST /why` decomposition | #15 | Covered |
| Repetition / 50-move **reachable outside the search** | #7 AC7 | Scoped, but explicitly as a *search-threaded* history stack |
| **SAN parse and emit** | — | **Absent from every issue** |
| **PGN read/write with variations** | — | **Absent as a capability** |

Checked across all fifteen issues on 2026-08-16. "SAN" appears in none of them; the move
layer is specified in coordinate notation throughout (#5 and #7 both fix castling as
`e1g1`). "PGN" appears only in #7 and #10, and in both cases as an artefact of the
*external arbiter* — fastchess's opening book `8moves_v3.pgn`, and the match PGN that
fastchess writes and a human reads. No issue asks boidboard itself to parse or emit either
notation.

That absence is not incidental, because **every piece of content the trainer consumes is
SAN inside PGN**: opening repertoires are PGN with variations, published theory is SAN,
puzzle solutions are SAN, and Lichess studies export as PGN. Coordinate notation is not a
substitute — no existing content is written in it. The same dependency turns up in the
plan already, unremarked: #14 wires the STS and WAC tactical suites into the harness as
smoke tests, and both are EPD with `bm` fields written in SAN.

Two rows above are nearly-covered rather than covered, and both are worth naming so they
are not discovered late. #6's single `generate_legal_moves` entry point is exactly what a
trainer wants, and it exists because of an anti-self-certification argument rather than for
our benefit — a nice case of a rule paying out twice. #7's repetition detection, by
contrast, is specified as a stack threaded through the *search*; a theory drill needs the
same detection in its own game loop, because holding a Philidor position **is** a threefold
repetition and a trainer that cannot see one cannot tell the student they succeeded.
Whatever #7 builds should be callable by a non-search caller, or the trainer reimplements
it and the two drift.

SAN and PGN also want to be built at a specific moment. SAN *emit* needs disambiguation
(file, rank, or both) and `+`/`#` suffixes, and both are questions about the legal move
list: a move is ambiguous only if another legal move of the same piece type reaches the
same square, and it is check only if the resulting position is. So SAN sits directly on top
of #6 and is awkward to retrofit underneath anything. Built right after the perft harness
goes green it is a small, well-tested addition to a component whose correctness has just
been established against an external oracle; built later it is a change to the move layer
with the whole engine standing on it.

They are also unusually cheap to test, and the right oracle is *not* the one already in the
tree. Stockfish speaks UCI, which is coordinate notation — `bestmove e2e4`, and `go perft`
likewise — so it cannot check SAN at all, and the existing differential harness does not
generalise. The oracle that does work needs no new dependency and is stronger than a
hand-written fixture: **take a published PGN corpus and round-trip it.** Parse each game's
SAN, play the moves, re-emit SAN, and diff against the source bytes. Every disambiguation
rule, every check and mate suffix, every castling and promotion spelling is exercised by
real games in the proportions real games produce them, and a mismatch localises to a single
move in a single game. It is the perft fixture's posture applied to notation: third-party,
factual, falsifiable, and committed before the code that must satisfy it. A proptest over
generated legal positions, as #4's AC1 does for FEN, covers the rare shapes a corpus is
thin on — under-promotions, double-disambiguation — and the two together are cheap.

This is tracked as **#19**, its own phase-1 issue, rather than by editing #4 or #6, whose
acceptance criteria are the customer's. The trainer itself is **#20**.

## The data model

Sketched in prose, not in Rust, and deliberately so — see "what is buildable today" below.

- **Repertoire**: a DAG of positions keyed by Zobrist, each node holding the moves the
  student is expected to know from it, plus provenance (which PGN, which line, what the
  annotator said).
- **Card**: the unit of scheduling — a (repertoire, position-key, expected-move) triple.
  Note that a card is *not* a position: the same position in a White repertoire and a Black
  repertoire is two different lessons.
- **Attempt**: card, timestamp, what was played, the grade.
- **Session**: a state machine over cards — present, expect, adjudicate, advance or reset
  to the start of the line.

`Card`, `Attempt` and `Session` are the interesting observation: **none of them names a
chess type**. A card holds an opaque position key and an opaque expected-move token; the
session machine cares only whether an attempt matched. The scheduling and drilling logic is
move-agnostic and could be written and tested against integers today.

## Scheduling

Spaced repetition, one of the SM-2 family or FSRS. This is a solved problem and not a place
to be inventive; the only project-specific notes are:

- The `clippy::float_arithmetic` denial binds `boid-eval-classical` and `boid-eval-boids`
  only (D-0011). A scheduler is not an evaluator and may use floats.
- Reviews must be reproducible from the attempt log — same log, same schedule — for the
  same reason Zobrist keys come from a hardcoded seed and never from entropy (#4). A
  scheduler you cannot replay is a scheduler you cannot debug.
- Clock is injected, never read from the ambient system, so tests can drill a year of
  reviews in a millisecond.

## Content, and where it comes from

The trainer is worth exactly as much as its content, and content has provenance and
licensing in a way code does not.

D-0002 is directly relevant and directly permissive: external oracles — "third-party,
factual, falsifiable data" — may be committed at any time, "and the earlier the better".
A puzzle set and an opening corpus are that kind of data. Under the same reasoning that put
`perft_oracle.txt` in before any engine existed, the content can land early.

Candidates, each of which needs its licence confirmed at the point of vendoring rather than
taken on trust from this document:

- The Lichess puzzle database — large, and drawn from real games with engine-verified
  solutions.
- `lichess-org/chess-openings` — the opening name/ECO corpus.
- Any repertoire the user already owns, imported as PGN. This is the mode that matters
  most: a trainer that drills *your* repertoire beats one that drills a stranger's.

Whatever is vendored should carry its source, licence and retrieval date in the file, as
the perft fixture does, and large sets should be fetched by script rather than committed —
`scripts/setup-stockfish.sh` is the pattern.

## Where it lives in the workspace

Not settled here, because it cannot be settled unilaterally: the `repo-invariants` CI job
asserts that the workspace members are **exactly** the seven crates issue #3 names, and
diffs the intra-workspace dependency edges against `.github/expected-dep-edges.txt`. An
eighth crate is a deliberate amendment of a CI-enforced invariant, and both files are the
customer's to change.

The options, with the trade-off stated:

1. **A new `boid-train` crate** depending on `boid-board` (+ `boid-search` for tactics
   adjudication and endgame defence). Cleanest: the domain model is testable headless, and
   a CLI drill session needs no web server. Costs an amendment to the seven-crate assertion
   and to the DAG file.
2. **Inside `boid-web`.** No invariant to amend. But `boid-web` depends on both evaluators
   and on a web framework behind an off-by-default feature (D-0012), so the drill logic
   would be untestable without the web stack and unusable from a terminal — a poor home for
   a state machine whose best property is that it needs neither.
3. **Inside `boid-board`.** Rejected: `boid-board` is the zero-dependency root and is board
   representation. A spaced-repetition scheduler is not.

Recommendation is (1), and the recommendation is where it stops until the invariant is
amended on purpose.

## What is buildable today, and why it is not built yet

The move-agnostic core — the scheduler, the card/attempt model, and the session state
machine — depends on nothing that does not already exist, and is genuinely testable now.

It is nonetheless not written, for the reason this project has already recorded twice.
D-0010: the `Score` type and `Evaluator` trait "are deliberately NOT written yet — they
cannot be designed before `Position` exists (#4), and guessing them now would cost a
rewrite." D-0014, on perft divide: "its return shape depends on how #4 represents a move,
and inventing that before `Move` exists would cost a rewrite."

The same argument applies here with one qualification. The scheduler is genuinely
independent and would survive; the boundary types — how a card names a position, how an
attempt names a move — are exactly the ones that would be guessed wrong, and they are the
ones every other part touches. Writing the half that would survive while stubbing the half
that would not produces a crate that must be revisited anyway, plus an eighth workspace
member and a CI amendment, in exchange for a scheduler nobody can drill with until phase 1
lands.

The cheaper ordering is: land SAN and PGN in phase 1 while the move layer is fresh and its
oracle is green, then build the trainer on a move representation that exists.
