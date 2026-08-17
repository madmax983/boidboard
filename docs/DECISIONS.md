# boidboard decision log

Append-only. Each entry is immutable once merged; a reversal is a NEW entry that
supersedes an old one by number. Entries are the project's binding rules — the
`Rule:` line of each is written to be greppable and to be quotable in review.

Format:

```
## D-NNNN — title
Status:       Accepted | Superseded by D-NNNN
Date:         YYYY-MM-DD
Context:      the situation or the issue text at stake
Decision:     what we did
Rule:         one imperative sentence binding later phases
Evidence:     a command and its result, or a SHA
Consequences: what a later phase pays or gains
```

---

## D-0001 — None of the prior attempt's code is to be reused

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 requires recording "that none of its code is to be reused: its
  geometry is toroidal and continuous, ours is discrete and attack-set-based, and keeping
  the door open keeps the gravitational pull alive." The prior attempt is preserved at tag
  `archive/boids-sim-attempt` -> commit `a1f70323c6d56629a18e959894f0815efa0d3ac9`, which
  is the head of branch `claude/boidboard-autumn-web-tdd-eae90o` and of PR #2 (closed,
  unmerged). Its tree is 79 files: a boids *simulator* web application built on autumn-web
  with diesel migrations over scenarios/runs/frames. It is not a chess engine.

Decision:     The archived tree is quarantined. It is preserved as a historical record and
  for no other purpose. No file, function, type, constant, schema, test, or design sketch
  from it is to be copied, adapted, or consulted for ideas. The two codebases model
  different things and share only a name:

    - the archived attempt's geometry is TOROIDAL and CONTINUOUS -- boids move through
      wrapped real-valued space under floating-point forces;
    - boidboard's geometry is DISCRETE and ATTACK-SET-BASED -- the "neighbourhood" of a
      piece is the set of squares it attacks and the pieces attacking it, on 64 squares,
      evaluated in fixed-point integers.

  Consulting the archived code would import continuous-space reasoning into a discrete
  problem. Keeping the door open keeps the gravitational pull alive, so the door is shut.

Rule:         Do not read, copy, adapt, or cite any code from `archive/boids-sim-attempt`.
  No commit reachable from `main` may have that tag's commit as an ancestor, and no blob
  from that tree may reappear in this repository (`LICENSE` excepted -- it is unchanged
  from the repository's initial commit and predates the attempt).

Evidence:     Enforced mechanically by the `repo-invariants` CI job, which fails if
  `git merge-base --is-ancestor a1f70323c6d56629a18e959894f0815efa0d3ac9 HEAD` succeeds, or
  if the blob-hash sets of `HEAD` and that commit intersect anywhere other than `LICENSE`.
  Both checks key off the COMMIT SHA, not the tag, and the job fetches the commit if the
  runner does not already have it. That is deliberate: the tag could not be pushed
  (D-0016), so enforcement that depended on it would not run at all.

Consequences: Phase 5 (#11) writes the boids force model from scratch against the discrete
  attack-set formulation, with no reference implementation to fall back on. That cost is
  the point of this entry.

---

## D-0002 — The no-acceptance-document rule, in narrowed and operable form

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 AC6 requires recording "the rule that no acceptance-criteria document
  may be authored before the perft harness is green". Read literally, that rule forbids
  issue #3 itself -- which is a list of acceptance criteria -- and forbids the evidence
  report that issue #3 asks for. A rule that nullifies its own source cannot be applied as
  written, so it is recorded in narrowed form and the narrowing is disclosed here rather
  than applied silently.

Decision:     The rule binds *in-repo, self-authored, self-graded* acceptance documents.
  The distinction it turns on is the load-bearing idea of issue #3: **an oracle is not an
  acceptance criterion.** An oracle is third-party, factual and falsifiable; an acceptance
  criterion is a standard this project sets for itself and then grades itself against.
  The failure mode being guarded is a project awarding itself a passing grade against a
  standard it wrote, which is exactly what the prior attempt did.

Rule:         No document in this repository may DEFINE or CERTIFY this project's own
  acceptance criteria until the perft harness is green against
  `tests/fixtures/perft_oracle.txt`. Specifically:
    - Requirements authored by the customer in GitHub issues are upstream of the
      repository and are NOT such documents.
    - Per-issue evidence reports delivered OUTSIDE the repository (a pull request body, an
      issue comment) are reports against externally-given criteria, not self-set
      standards, and are NOT such documents.
    - External test ORACLES -- third-party, factual, falsifiable data such as published
      perft node counts -- may be committed at any time, and the earlier the better.
    - Anything else that grades this project against criteria this project wrote IS such a
      document and is forbidden until perft is green.

Evidence:     The prior attempt (D-0001) committed `docs/AC_VERIFICATION.md` certifying
  "All 52 acceptance criteria are met", with 393 passing tests, for a boids simulator
  rather than a chess engine. Every criterion was met and the product was wrong.
  Enforced by the `repo-invariants` CI job: any TRACKED path whose basename begins with
  `accept` or `criteria` fails the build. The check is repository-wide -- an earlier version
  searched only `docs/`, which made the rule evadable by putting the document anywhere else.

Consequences: Issue #3's own AC evidence table is delivered in the pull request body and as
  an issue comment. It is deliberately NOT committed to this repository -- committing it
  would violate this rule in the same change that records it.

---

## D-0003 — "First commit" is read as the first commit of this branch

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 AC3 requires `tests/fixtures/perft_oracle.txt` to exist "in the first
  commit", so that "the project's acceptance criteria physically precede the implementation
  and cannot be retrofitted".

Decision:     This repository's literal first commit is `cd0522c` (`.gitignore` + `LICENSE`
  only), which predates issue #3 and is already published as `main`. Satisfying the phrase
  literally would require rewriting published history -- a worse outcome than the phrase's
  literal reading, and one that would destroy the very audit trail the AC exists to create.
  The fixture is therefore the first commit OF THIS BRANCH, landed together with this
  decision log and before any file under `crates/` exists.

  The sentence explains its own intent: it is a claim about ORDERING, not about commit
  identity. Because branch history is trivially rewritable, ordering alone is weak
  evidence, so the anti-retrofit property is additionally enforced by two mechanisms that
  commit ordering cannot provide:
    (a) the fixture is loaded with `include_str!`, so removing or renaming it is a COMPILE
        ERROR across the whole workspace, not a silently skipped test;
    (b) a SHA-256 of the fixture is pinned as a constant in the test source, so editing any
        digit of any node count is an immediate, loud test failure that forces the editor
        to state their intent.

Rule:         No file under `crates/` may be added in a commit earlier than the commit that
  adds `tests/fixtures/perft_oracle.txt`.

Evidence:     `git log --diff-filter=A --format='%H %ad' -- tests/fixtures/perft_oracle.txt`
  precedes the first commit touching `crates/`.

Consequences: A reviewer verifies AC3 with `git log --diff-filter=A`, and independently by
  `mv tests/fixtures/perft_oracle.txt /tmp && cargo build --workspace`, which fails to
  build rather than passing with fewer tests.

---

## D-0004 — The prior attempt's commit is not dangling; the tag guards a different threat

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 says to "tag the prior attempt's dangling commit `a1f7032` as
  `archive/boids-sim-attempt` before `git gc` reclaims it".

Decision:     `a1f70323c6d56629a18e959894f0815efa0d3ac9` is NOT dangling. On origin it is
  the head of `refs/heads/claude/boidboard-autumn-web-tdd-eae90o` and of `refs/pull/2/head`.
  A `git gc` would reclaim only an ephemeral clone's copy, re-fetchable in one command, and
  `git gc` on the server would not touch a commit reachable from a branch.

  The tag was created anyway, because the durability threat is real but different:
  PR #2 is closed, and a closed PR's branch is exactly the kind of ref that gets deleted.
  After that the commit survives only via `refs/pull/2/head`, which is GitHub-specific and
  which default clones do not fetch. `refs/tags/` is fetched by default clones and survives
  both branch deletion and migration off GitHub. The tag is ANNOTATED so the quarantine
  rationale lives in the git object itself rather than only in this file.

Rule:         Do not restate the "dangling commit" or "before `git gc` reclaims it" framing
  in commit messages, tag messages, documentation, or reports. It has been verified false
  and repeating it would propagate a factual error.

Evidence:     `git ls-remote origin | grep a1f70323` at 2026-08-16 returned:
    a1f70323c6d56629a18e959894f0815efa0d3ac9  refs/heads/claude/boidboard-autumn-web-tdd-eae90o
    a1f70323c6d56629a18e959894f0815efa0d3ac9  refs/pull/2/head

Consequences: The tag, not the branch, is the permanent record. Deleting
  `claude/boidboard-autumn-web-tdd-eae90o` is therefore safe once the tag is on origin.

---

## D-0005 — AC7 is satisfied by invariant; the real defect is over-broad ignore patterns

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 AC7 requires that "the stale `target/` directory from the prior
  attempt is removed and gitignored".

Decision:     No `target/` directory existed in the working tree at branch start, no path
  under `target/` was ever tracked in git history at any ref, and the prior attempt's
  79-file tree contains zero `target/` paths. There was nothing to remove, and this project
  will not report removing something that never existed -- a false statement of work is
  exactly the class of dishonesty issue #3 exists to prevent.

  The AC does however contain one genuine defect. The inherited `.gitignore` used the
  slashless patterns `target` and `debug`, which silently untrack source paths. Measured
  with `git check-ignore` against a scratch repository:

      pattern     target/debug/x   src/target/mod.rs   crates/a/target/debug/x
      target      ignored          IGNORED             ignored
      target/     ignored          IGNORED             ignored
      /target/    ignored          tracked             tracked

  Note the middle row: a trailing slash restricts the pattern to DIRECTORIES but not to the
  repository root, so `target/` still shadows `crates/boid-board/src/target/mod.rs`. Only
  the LEADING slash does the work. This project uses `/target/` and `/debug/`.

  Root-anchoring loses no coverage here because this is a cargo WORKSPACE: every build
  writes to the root `target/` regardless of which member is built. It keeps `target` and
  `debug` usable as source-tree names, which matters in a chess engine -- a search has a
  target square, and a debug module is a debug module.

Rule:         Build-artifact ignore patterns must be root-anchored (`/target/`, not
  `target` and not `target/`). Report AC7 as satisfied by invariant, never as work
  performed.

Evidence:     Baseline capture before any change:
    `git ls-files | grep -c '^target/'`                     -> 0
    `git log --all --diff-filter=A --name-only -- '*target/*'` -> empty
    `ls -d target`                                          -> No such file or directory
  After the fix, `git check-ignore -q target/debug/x` succeeds while
  `git check-ignore -q crates/boid-board/src/target/mod.rs` fails.

Consequences: The `.gitignore` change is a real behavioural change with a real regression
  test; the "removal" half of AC7 is reported as vacuously true rather than performed.

---

## D-0006 — The perft oracle fixture format

Status:       Accepted
Date:         2026-08-16

Context:      `tests/fixtures/perft_oracle.txt` is the external oracle that issue #6's perft
  harness and Stockfish differential harness will be judged against. Its format must serve
  a consumer that does not yet exist.

Decision:     One line per position: `id | fen | depth:nodes<flag> ...`, pipe-delimited,
  with `#` comments. Chosen over EPD-with-opcodes (`;D1 20`) and over TOML because it is the
  only shape that carries a PER-DEPTH provenance flag, and the published table genuinely
  mixes exhaustively-computed counts with forum estimates. The FEN appears exactly once per
  position, so the one string that must be byte-exact has one chance to be wrong, not one
  per depth.

  Node counts are `u64`. `startpos` is committed through depth 13; the published depth-14
  and depth-15 values exceed `u64::MAX` and are recorded in the fixture header as
  explicitly-excluded prose, with their values and the reason.

  Seven rows are committed, not six: the six standard positions plus `position4-mirror`,
  the colour-mirror the wiki publishes with identical counts at every depth. Because the
  counts are identical, NO count-based test can distinguish the mirror from the original --
  only byte-identity of the FEN strings can. Committing both is what makes a
  mirror-for-original substitution detectable.

Rule:         Every node count in the fixture carries exactly one provenance flag:
  `v` = independently re-derived with Stockfish by this project; `p` = published only, not
  corroborated here. Never add a count without a flag, and never mark `v` without having
  actually run the engine.

Evidence:     The grammar is specified in the fixture's own header and enforced by
  `crates/boid-board/tests/oracle_validation.rs`.

Consequences: Issue #6 can replay a budgeted subset by node count without a schema change,
  and can tell corroborated rows from merely-published ones.

---

## D-0007 — The fixture is load-bearing, not decorative

Status:       Accepted
Date:         2026-08-16

Context:      A committed data file that nothing reads is indistinguishable from a file that
  is wrong. The repository-root `tests/` directory named by AC3 is INERT to Cargo: a virtual
  workspace has no root package, so Cargo never compiles anything there.

Decision:     The fixture is embedded at compile time by exactly one declaration,
  `boid_board::perft::oracle::ORACLE_TEXT`, using
  `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/perft_oracle.txt"))`.
  A SHA-256 of its bytes is pinned as a constant in the test source -- held by the checker,
  not by the file, because a self-referential hash is unverifiable.

Rule:         No `.rs` file may be placed under the repository-root `tests/` directory; code
  there silently never runs. Integration tests live in `crates/<crate>/tests/`.

Evidence:     Enforced by the `repo-invariants` CI job. Removing the fixture is a compile
  error; editing a digit fails `oracle_transcription::fixture_body_hash_is_pinned`.

Consequences: The anti-retrofit property of AC3 is enforced by the compiler and by a hash,
  not merely by commit ordering, which is rewritable.

---

## D-0008 — Published FENs are normalised to ASCII, and the normalisation is declared

Status:       Accepted
Date:         2026-08-16

Context:      Six of the seven FENs published on the source page separate their fields with
  U+00A0 NON-BREAKING SPACE, not U+0020.

Decision:     The fixture is ASCII-only. U+00A0 separators are normalised to U+0020 and
  trailing whitespace is stripped. This is a declared transcription normalisation, not
  verbatim storage, because "verbatim" cannot survive U+00A0 here: Stockfish's tokeniser is
  ASCII-only and silently mis-parses an NBSP-separated FEN into a DIFFERENT legal position,
  reporting `Nodes searched: 0`, while Rust's `str::split_whitespace` treats U+00A0 as
  whitespace and parses it correctly. That divergence would make the fixture parse cleanly
  in our code and disagree with the engine only in issue #6, where it would be blamed on
  move generation for days.

  The FIELD COUNT of each published FEN is preserved. Kiwipete is published with four
  fields (no halfmove/fullmove counters) and is stored that way; ` 0 1` is NOT appended.
  Stockfish accepts the four-field form (`perft 3` = 97862, confirmed).

Rule:         `tests/fixtures/perft_oracle.txt` must contain no byte outside U+0000..U+007F.

Evidence:     Verified in this container on 2026-08-16:
    NBSP-separated Kiwipete FEN -> Stockfish reports `Nodes searched: 0`
    U+0020-separated, same FEN  -> Stockfish reports `Nodes searched: 97862`
    Rust `"a\u{a0}b".split_whitespace()` -> `["a", "b"]`
  Asserted by `oracle_transcription::fixture_is_ascii_only` and by a byte-level grep in the
  `repo-invariants` CI job.

Consequences: The fixture is not a byte-for-byte copy of the page, and says so in its own
  header. Issue #6's differential harness is protected from a silent wrong-answer path.

---

## D-0009 — Edition 2024, MSRV, and the unsafe-code policy

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 mandates edition 2024. Issue #5 will implement magic bitboards, which
  conventionally use unchecked indexing in their hot path.

Decision:     Edition 2024, `resolver = "3"`, MSRV `1.94`, and the toolchain pinned in
  `rust-toolchain.toml` to `1.94.1`. `resolver = "3"` must be stated explicitly because a
  virtual workspace has no edition to infer it from. The toolchain is pinned because
  `clippy -D warnings` with a floating toolchain means a future Rust release can turn CI red
  with no change to this repository.

  `unsafe_code` is `deny` at the workspace level, not `forbid`, and the six reserved crates
  additionally `#![forbid(unsafe_code)]` in their own roots. `boid-board` uses `deny` only,
  which leaves issue #5 a documented exception path: a module-scoped
  `#[allow(unsafe_code)]`, a `// SAFETY:` comment on every `unsafe` block, and explicit
  inner `unsafe { }` blocks inside any `unsafe fn` -- because `unsafe_op_in_unsafe_fn` is a
  warning in edition 2024 and therefore an ERROR under `-D warnings`.

  The pin has one measured cost. A runner without that exact toolchain downloads it on the
  first cargo invocation, and that download is occasionally reset mid-flight -- observed on
  GitHub Actions as `component download failed for rust-std-x86_64-unknown-linux-gnu:
  Connection reset by peer (os error 104)`, which failed one job while four others on the
  same run succeeded. A network hiccup that reads as a build failure is exactly the noise
  that teaches people to ignore red CI, so every CI job installs the toolchain through
  `scripts/setup-toolchain.sh`, which retries with backoff and then fails loudly rather
  than letting the error surface inside an unrelated cargo command.

  That step must run BEFORE `Swatinem/rust-cache`, not after. The cache action computes its
  key from `rustc -vV`, and that invocation is itself what triggers the rustup download for
  the pinned version — so ordered the other way round the retry sits downstream of the step
  that actually fails. This is not a hypothetical: run 1's error was literally
  `Command failed: rustc -vV`, raised inside the rust-cache step, and the first version of
  this mitigation was placed after it and would not have helped.

Rule:         Do not introduce `unsafe` outside `boid-board`, and do not introduce it there
  without a module-scoped allow and a `// SAFETY:` comment on every block. Every CI job
  that runs cargo must first run `scripts/setup-toolchain.sh`.

Evidence:     `rustc 1.94.1`, `cargo 1.94.1`; `rust-toolchain.toml` is committed.

Consequences: Issue #5 does not have to relitigate the lint policy or mechanically rework
  every accessor.

---

## D-0010 — The crate dependency DAG

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 mandates seven crates. Issues #13 and #16 run SPRT arms that must
  select an evaluator at RUNTIME.

Decision:     The intra-workspace dependency edges are exactly:

    boid-board          -> (nothing)
    boid-eval-classical -> boid-board
    boid-eval-boids     -> boid-board
    boid-search         -> boid-board          # NEVER a concrete evaluator
    boid-uci            -> boid-board, boid-search, boid-eval-classical, boid-eval-boids
    boid-web            -> boid-board, boid-search, boid-eval-classical, boid-eval-boids
    boid-tune           -> boid-board, boid-search, boid-eval-classical, boid-eval-boids,
                           boid-uci

  `boid-uci` is the composition root and will own the `boidboard` binary from issue #7.
  `boid-search` is generic over an `Evaluator` trait defined in `boid-board`; it must never
  name a concrete evaluator.

Rule:         Evaluator selection is a RUNTIME UCI option, never a cargo feature, and
  `boid-search` must never depend on `boid-eval-classical` or `boid-eval-boids`.

Evidence:     The edge set is committed as `.github/expected-dep-edges.txt` and diffed
  against `cargo metadata` by the `repo-invariants` CI job.

Consequences: If `boid-search` ever imported a concrete evaluator, issue #13's decision gate
  and issue #16's ablation campaign would become a loop over cargo builds instead of a loop
  over UCI options, and every SPRT result would carry a build-provenance question. The
  `Score` type and `Evaluator` trait are deliberately NOT written yet -- they cannot be
  designed before `Position` exists (#4), and guessing them now would cost a rewrite.

---

## D-0011 — Evaluation is fixed-point; floating-point is denied in the evaluator crates

Status:       Accepted
Date:         2026-08-16

Context:      D-0001 quarantines the prior attempt because its geometry is continuous.
  A documented rule that is not mechanically enforced decays.

Decision:     Evaluation is fixed-point integer arithmetic. `clippy::float_arithmetic` is
  `deny` in `boid-eval-classical` and `boid-eval-boids`.

  The denial is a crate-root attribute (`#![deny(clippy::float_arithmetic)]`) rather than a
  `[lints.clippy]` table in those two manifests, because Cargo rejects a manifest that both
  inherits the workspace lint table (`[lints] workspace = true`) and adds local lints:
  "cannot override `workspace.lints` in `lints`". Spelling the whole workspace table out
  again in two manifests would guarantee drift; the crate attribute composes instead.

Rule:         No floating-point arithmetic in `boid-eval-classical` or `boid-eval-boids`.

Evidence:     The `#![deny(clippy::float_arithmetic)]` attribute at the root of each of
  those two crates; enforced by `cargo clippy --workspace --all-targets -- -D warnings`.

Consequences: Continuous-space boids code -- the exact thing D-0001 quarantines -- fails the
  build on contact, so the architecture rejects the old idea rather than a grep rejecting
  the old text. Costs nothing today because both crates are empty. `boid-tune`'s Texel
  tuner (#14) is deliberately outside this lint: gradient descent needs floats.

---

## D-0012 — autumn-web and autumn-harvest are pinned, optional, and off by default

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 requires pinning `autumn-web = "0.6"` and `autumn-harvest = "0.5"`
  from crates.io rather than path deps on trunk. Neither is needed until phase 8 (#15) and
  phase 9 (#16). `autumn-web` pulls a large tree including diesel/postgres by default.

Decision:     Both are declared in `[workspace.dependencies]` with the issue's literal
  version strings, and consumed by exactly one crate each -- `boid-web` and `boid-tune` --
  as `optional = true` behind an off-by-default feature (`serve`, `orchestrate`).

  This is the only configuration that is simultaneously (a) byte-identical to the issue's
  text, (b) a REAL pin -- an optional dependency is still resolved into `Cargo.lock`,
  unlike a declare-only `[workspace.dependencies]` entry, which is silently absent from the
  lock and pins nothing -- and (c) fast: zero autumn artifacts compile in the AC1 path, so
  the TDD ladder stays affordable.

  The phase-0 reference is a plain re-export (`pub use autumn_web as framework;`), not a
  macro invocation, because lints fired inside a third-party macro expansion are attributed
  to our crate and could fail `-D warnings` in a way we cannot fix.

Rule:         Do not enable `serve` or `orchestrate` in the default feature set, and do not
  add either dependency to a second crate.

Evidence:     `cargo tree -i autumn-web` finds nothing without `--features serve`;
  `Cargo.lock` nonetheless contains `autumn-web 0.6.0` and `autumn-harvest 0.5.0`. The
  nightly CI job builds both with their features ON, so a broken or yanked pin is caught.

Consequences: `cargo build --workspace` -- AC1 -- does not pay for a 400-package dependency
  tree, while the pin remains real and exercised.

---

## D-0013 — Cargo.lock is committed, because "0.6" is not a pin

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 says "pinning `autumn-web = "0.6"`". In Cargo, `"0.6"` is a caret
  RANGE (`>=0.6.0, <0.7.0`), not a pin.

Decision:     `Cargo.lock` is committed. The manifest keeps the issue's literal `"0.6"` and
  `"0.5"` strings; the lockfile is where the actual pin lives. A dedicated `locked` CI job
  runs `cargo build --workspace --locked`, separate from the three verbatim AC1 commands,
  so AC1's evidence matches the issue's text byte-for-byte while the lockfile is still
  proven to be honoured.

Rule:         Commit `Cargo.lock`. Never add `--locked` to the three AC1 commands.

Evidence:     `git ls-files Cargo.lock` is non-empty; the `locked` CI job is green.

Consequences: Issue #13's decision gate can say exactly which dependency versions played
  which SPRT.

---

## D-0014 — Deferrals recorded rather than silently skipped

Status:       Accepted
Date:         2026-08-16

Context:      Several defensible practices are out of scope for phase 0. Silently skipping
  them is indistinguishable from not knowing about them.

Decision:     The following are deliberately deferred:

  - **GitHub Actions pinned by tag, not commit SHA.** SHA pinning is better supply-chain
    practice, but a wrong SHA is an unrecoverable CI failure and SHAs cannot be verified as
    confidently as tags from this environment. Revisit when CI is stable.
  - **No `.devcontainer/`.** `scripts/setup-stockfish.sh` is the reproducible entry point;
    a devcontainer would be a second, drifting source of truth for one apt package.
  - **No `.claude/settings.json`.** A `SessionStart` hook running the stockfish script is
    documented in `README.md`, but writing a project's agent configuration unprompted is
    out of scope for an issue about a chess engine.
  - **No `xtask/`.** Nothing yet needs it; adding it now would be an eighth crate the issue
    does not name.
  - **No third-party Rust chess crate as a second oracle.** Triangulating the fixture
    against e.g. `shakmaty` was considered and rejected for phase 0: the real defence
    against "numbers generated by the checker" is procedural -- the counts are transcribed
    from the fetched page BEFORE Stockfish is consulted -- not a second engine. Recorded as
    an option for issue #6.
  - **`include_str!` reaching outside the crate directory** breaks `cargo package --verify`
    for `boid-board`. Accepted: nothing here publishes to crates.io.
  - **`validate_fen` returns `Result<(), String>`.** An unmatchable error, immediately
    re-stringified into `OracleError::MalformedFen`. Correct for a fixture validator, wrong
    for the FEN reader issue #4 will build on it: a caller that wants to branch on *why* a
    FEN was rejected cannot. Deferred to #4, which should introduce a typed `FenError` and
    have this function return it.
  - **`PerftEngine` has no `divide`.** Perft divide -- the per-root-move breakdown -- is the
    only practical way to localise a mismatch, and issue #6 names it explicitly. Deliberately
    not guessed at now: its return shape depends on how #4 represents a move, and inventing
    that before `Move` exists would cost a rewrite. #6 adds it to the trait.

Rule:         Revisit each deferral in the issue named against it; do not let a deferral
  become an unexamined default.

Evidence:     This entry.

Consequences: A reviewer can tell "considered and deferred" from "not considered".

---

## D-0015 — Stockfish is provisioned by script, because the package does not land on PATH

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 AC5 requires that `stockfish` is installed and that
  `echo -e "position startpos\ngo perft 3" | stockfish` outputs `Nodes searched: 8902`.
  "Installed" is a property of a machine, not of a repository, and this project's
  development containers are ephemeral.

Decision:     `scripts/setup-stockfish.sh` is the repository's durable answer: it installs
  the package, creates the PATH symlink, and VERIFIES the perft output, idempotently. CI
  runs it, so AC5's evidence is a CI job log on a clean `ubuntu-latest` runner rather than a
  transcript from one developer's container.

  Two environment-specific traps are handled:
    - Ubuntu's `stockfish` package installs ONLY to `/usr/games/stockfish`, and `/usr/games`
      is not on the PATH of every environment (it is not on this container's). Without a
      symlink into `/usr/local/bin`, AC5's bare `stockfish` invocation fails with
      "command not found" even though the package is installed.
    - `echo -e` is a bashism. Under `dash` (`/bin/sh` on Debian and Ubuntu) it PRINTS the
      literal `-e` and the escape is not interpreted, so the script uses `printf`.

  The differential test skips loudly when the binary is absent, but panics instead when
  `BOIDBOARD_REQUIRE_STOCKFISH=1` -- which CI sets -- so the coverage cannot silently
  evaporate on the runner while still letting `cargo test --workspace` pass on a clean
  checkout, as AC1 demands.

Rule:         Never assume `stockfish` is on PATH; provision it with
  `scripts/setup-stockfish.sh` and require it in CI.

Evidence:     `dpkg -L stockfish` lists only `/usr/games/stockfish`. After the symlink,
  `echo -e "position startpos\ngo perft 3" | stockfish` outputs `Nodes searched: 8902`.

Consequences: AC5 is reproducible rather than a one-shot manual step in a container that no
  longer exists.

---

## D-0016 — Corrections found by the post-merge review, and the AC4 tag blocker

Status:       Accepted
Date:         2026-08-16

Context:      After issue #3's pull request was merged, a six-angle code review with
  adversarial verification was run against it. It found real defects, including several in
  this project's own claims about itself. An error in a decision log is worse than an error
  in code, because the log is what later phases trust without re-checking.

Decision:     The corrections are recorded here rather than by editing history.

  **Miscounted oracle totals.** The pull request said the fixture held "51 node counts", of
  which "44" were verified and "7" published-only. The true totals are **55 counts, 44
  verified, 11 published-only**. The verified figure was right; the total was understated,
  which flattered the verified fraction (80%, not 86%). The three numbers are now asserted
  by `oracle_transcription::fixture_provenance_totals_are_what_the_project_claims`, so they
  cannot drift again unnoticed.

  **Mis-glossed provenance flag.** The `p` flag was described as "too deep to replay". Two
  of the eleven `p` counts are `perft(0) = 1`, which is not too deep -- it is 1 by
  definition, and `go perft 0` cannot report it because it falls through to a real search.
  Corrected in the fixture header and `README.md`.

  **Wrong redirect code.** The fixture header recorded the source URL redirect as HTTP 308.
  A GET returns **301**; 308 is what a HEAD request answers. Corrected.

  **Wrong CI-run attribution.** The pull request attributed the transient toolchain-download
  failure to CI run 2. It happened in run **1** (`8f93a43`), which failed on both the
  toolchain download and the dependency-DAG diff; run 2 (`7fabd23`) failed on the DAG diff
  alone.

  **Two accessors weakened the tests they were meant to serve.** The refactor that
  introduced `PerftCase::fen_fields()` and `verified_counts()` routed every call site
  through them, and mutation testing showed both could be sabotaged with the whole suite
  still green: `fen_fields()` could `return 4`, and `verified_counts()` could drop its
  provenance filter. An accessor that every assertion trusts is a single point at which all
  of them can be made to lie. Both mutations are now caught.

  **A miscount in a commit message.** `111a360` says the second transcription holds "39
  canonical counts typed by hand". It holds **37**. The commit is merged and its message is
  immutable, so the correction lives here.

  **A blind spot the budget creates.** `cargo test --workspace` replays only the 30 verified
  rows under the default 5,000,000-node ceiling, so a corrupted verified count *deeper* than
  the budget is invisible locally. It is not invisible in CI: the nightly `deep-perft` job
  raises the ceiling and replays all 44. The differential harness's module doc now says so
  rather than leaving the reader to infer coverage it does not have.

Rule:         A numeric claim about this project, made anywhere -- commit message, README,
  decision log, pull request -- must be asserted by a test if it is asserted at all.
  Introducing an accessor that existing assertions route through requires a test pinning
  the accessor itself against a direct computation.

Evidence:     `cargo test --workspace`; the mutations above now fail
  `fen_fields_accessor_agrees_with_the_raw_fen`,
  `verified_counts_excludes_published_only_rows`, and
  `fixture_provenance_totals_are_what_the_project_claims`.

Consequences: The fixture's SHA-256 changed when its header was corrected, so
  `ORACLE_SHA256` moved with it -- which is the pinning mechanism working as designed
  rather than a weakening of it.

---

## D-0017 — AC4's tag could not be pushed by the authoring session

Status:       Accepted
Date:         2026-08-16

Context:      Issue #3 AC4 requires `archive/boids-sim-attempt` to point at
  `a1f70323c6d56629a18e959894f0815efa0d3ac9` **and be pushed to origin**.

Decision:     The annotated tag was created locally at the correct commit. It could not be
  pushed. `git push origin archive/boids-sim-attempt` returned HTTP 403 on four attempts,
  while branch pushes from the same session succeeded and the egress proxy recorded no
  relay failures -- so the refusal is GitHub-side and the session credential is scoped to
  `refs/heads/*`. No GitHub API tool available to the session creates tag refs either.

  This is reported as an unmet criterion rather than worked around. A lightweight tag, a
  branch named like a tag, or a local-only tag would each have produced a green tick for
  something the criterion does not ask for.

Rule:         AC4 remains open until `git ls-remote --tags origin` lists
  `refs/tags/archive/boids-sim-attempt`. Do not substitute another ref type for it.

Evidence:     `git ls-remote --tags origin` returns nothing. Locally,
  `git cat-file -t archive/boids-sim-attempt` is `tag` and
  `git rev-parse archive/boids-sim-attempt^{commit}` is the required SHA.
  A maintainer completes it with:
      git tag -a archive/boids-sim-attempt a1f70323c6d56629a18e959894f0815efa0d3ac9 \
        -m 'Quarantine: prior attempt, PR #2 (closed unmerged).'
      git push origin archive/boids-sim-attempt

Consequences: The `repo-invariants` job enforces the tag's target the moment it exists and
  warns until then. Quarantine enforcement (D-0001) is unaffected: it keys off the commit
  SHA, so it is live today.

---

## D-0018 — The trainer is roadmap, and SAN/PGN are its phase-1 prerequisite

Status:       Accepted
Date:         2026-08-16

Context:      The customer asked for an opening / tactics / theory trainer, describing it as
  "not in engine territory" and "easy to build once we have all the movement/stuff built".
  The first half is right; the second half is conditional, and the condition is not met by
  the issues as written. A trainer's content is SAN inside PGN -- repertoires are PGN with
  variations, published theory and puzzle solutions are SAN, Lichess studies export as PGN.
  Issue #4 covers FEN and Zobrist; #5 and #6 cover generation and legality. **No issue
  covers SAN or PGN at all.** Coordinate notation is not a substitute: no existing content
  is written in it.

  SAN also has a natural moment. Emitting it requires disambiguation and check/mate
  suffixes, both of which are questions about the legal move list, so it sits directly on
  top of #6 -- a small addition to a component whose correctness has just been established
  against an external oracle, or a change to the move layer with the whole engine standing
  on it. The same choice, six phases apart.

Decision:     The trainer is accepted onto the roadmap as a later phase and designed in
  `docs/TRAINER.md`. Its acceptance criteria are NOT in that document: they are in its
  GitHub issues, because D-0002 forbids in-repo self-authored acceptance criteria until
  perft is green, and issues are upstream of the repository.

  SAN and PGN I/O are scoped as #19, a phase-1 issue landing after #6, rather than by
  editing #4's or #6's acceptance criteria, which are the customer's. The trainer is #20.

  Crate placement is recommended (`boid-train` -> `boid-board`, `boid-search`) and
  deliberately not executed. The `repo-invariants` job asserts the workspace members are
  exactly the seven crates issue #3 names and diffs the DAG against
  `.github/expected-dep-edges.txt`; an eighth crate amends a CI-enforced invariant, which
  is not a side effect of a feature request.

  No trainer code is written now. The scheduler and session state machine are genuinely
  move-agnostic and buildable today, but the boundary types -- how a card names a position,
  how an attempt names a move -- are exactly the ones that would be guessed wrong, and
  every other part touches them. This is the argument D-0010 made for `Evaluator` and
  D-0014 made for perft divide, applied a third time.

Rule:         The trainer must not carry its own move notation: SAN and PGN I/O belong in
  `boid-board` and land in phase 1. No eighth workspace crate may be added without an entry
  superseding this one.

Evidence:     The prerequisite table in `docs/TRAINER.md`, checked against the full text of
  all fifteen issues on 2026-08-16, not against their titles. "SAN" appears in none of them;
  the move layer is coordinate-notation throughout (#5 and #7 both fix castling as `e1g1`).
  "PGN" appears only in #7 and #10, both times as an artefact of the external arbiter --
  fastchess's `8moves_v3.pgn` book and the match PGN a human reads -- never as something
  boidboard parses or emits. Re-checkable with:
      gh issue list --state all --limit 100 --json number,body \
        | jq -r '.[] | select(.body | test("SAN|PGN")) | .number'

Consequences: Phase 1 gains a small addition, proptest-able exactly as #4's AC1 is for FEN.
  Note what it does NOT get: Stockfish speaks UCI coordinate notation and emits no SAN at
  all, so the existing differential harness does not extend to it. SAN's oracle is a
  published PGN corpus round-tripped byte-for-byte -- parse, play, re-emit, diff -- which is
  the perft fixture's posture applied to notation and can likewise be committed before the
  code that must satisfy it. The trainer, in return, gets to be the cheap part the customer
  expects. Deferring SAN would not remove this work; it would move it under a load-bearing
  engine.

---

## D-0019 — `Board` is 152 bytes of representation and nothing else

Status:       Accepted
Date:         2026-08-17

Context:      Issue #4 specifies the value type: six piece bitboards, two colour bitboards,
  a redundant `mailbox: [u8; 64]`, an incrementally maintained zobrist key, a pawn hash, and
  packed side-to-move / castling rights / en-passant square / halfmove clock / fullmove
  number, `Copy`, "~160 bytes", with AC3 asserting `size_of::<Board>() <= 256`.

Decision:     The layout is

      pieces:   [Bitboard; 6]        48   indexed by PieceKind: P,N,B,R,Q,K
      colours:  [Bitboard; 2]        16   indexed by Colour: White, Black
      key:      u64                   8
      pawn_key: u64                   8
      mailbox:  [Option<Piece>; 64]  64   LERF: index = rank*8 + file, a1 = 0
      fullmove: u16                   2
      stm:      Colour                1
      castling: CastlingRights        1
      ep:       Option<File>          1
      halfmove: u8                    1

  measured at `size_of` 152, `align_of` 8. "Packed" is honoured as adjacent declaration
  rather than bit-packing, because bit-packing the six state bytes into a `u32` measures
  148 -> 152: the same 152 for a hand-written bit layout no test could read at a glance.
  The issue's "~160 bytes" is prose about the intended magnitude and is not restated as a
  number anywhere in this repository (D-0016).

  `[Option<Piece>; 64]` is a DECLARED NARROWING of the issue's `[u8; 64]`: `Piece` is a
  twelve-variant `#[repr(u8)]` enum, `size_of::<Option<Piece>>()` is 1 and the array is 64
  bytes, so the representation is byte-identical while the 244 invalid `u8` states are
  deleted by the type. No `u8` mailbox accessor is exposed. That matters specifically here:
  issue #5's magic bitboards are the one place D-0009 permits `unsafe`, and an out-of-range
  index reaching a `get_unchecked` is memory corruption that surfaces as a wrong perft
  count rather than as a crash.

  `Piece`'s discriminant IS the zobrist piece index (`colour * 6 + kind`), so there is one
  index map in the crate rather than two that can drift.

  `PartialEq`/`Eq` are derived and INCLUDE the clocks, which is why board equality is not
  the repetition relation; `Hash` is deliberately not derived, because a 152-byte structural
  hash sitting beside a 64-bit zobrist key is a trap for #6 and #8. `Debug` is hand-written
  and prints the FEN and both keys: a derived `Debug` dumps 64 mailbox entries and makes
  every `assert_eq!` between two boards unreadable, and assertion messages are evidence in
  this repository.

Rule:         Nothing but representation may enter `Board` -- no zobrist history, no attack
  or checker cache, no FEN dialect bit. `recomputed_key`/`recomputed_pawn_key` iterate the
  BITBOARDS while `to_fen` iterates the MAILBOX; a refactor that merges those iterations
  must add an equivalent cross-check, because a mailbox/bitboard desync is invisible to
  every acceptance criterion at once if both consumers read the same representation. Issue
  #5 may add an integer mailbox path only with a superseding entry and a measurement, and
  may add `debug_assert!(self.consistency().is_ok())` at the end of `make_move`.

Evidence:     `board_layout::board_is_one_hundred_and_fifty_two_bytes` asserts 152, `<= 256`
  and `align_of == 8`; `option_piece_is_one_byte` asserts the narrowing; measured on the
  toolchain `rust-toolchain.toml` pins, rustc 1.94.1.

Consequences: Issue #5 gets a struct whose every field is representation, and a stated
  invariant (`Board::consistency`) it can assert after each move rather than inventing one.

---

## D-0020 — The zobrist key set, its seed, and what AC4's evidence actually proves

Status:       Accepted
Date:         2026-08-17

Context:      Issue #4 requires keys generated by splitmix64 from a hardcoded seed in a
  `const fn`, never from entropy, and AC4 requires them to be byte-identical across two
  separate process runs.

Decision:     781 keys in one flat index space:

      0..768    piece_square[piece][square],  i = piece * 64 + square,
                piece = colour * 6 + kind,    square LERF (a1 = 0)
      768       side_to_move, XORed when BLACK is to move
      769..773  castling K, Q, k, q, XORed per right present
      773..781  en_passant a..h, XORed when the FEN records an ep square

  One flat space rather than four tables, because the hygiene checks only mean anything run
  over all of it together: a cross-table dependency (`ep[e] == castling[K] ^ side`) is
  exactly as fatal as an intra-table one and invisible to per-table checks.

      GAMMA = 0x9E3779B97F4A7C15
      SEED  = 0x626F6964626F6172        ASCII "boidboar", big-endian
      key_at(i) = splitmix64(SEED + (i + 1) * GAMMA)

  The INDEXED form, not a running stream. splitmix64's state update is `state += GAMMA`, so
  after i steps the state is `SEED + (i+1)*GAMMA` and the two forms are identical rather
  than merely equivalent -- but the indexed form makes each slot a pure function of its
  index, so reordering the build loop cannot silently reassign keys. Const evaluation checks
  integer overflow unconditionally regardless of profile, so the `wrapping_*` calls are
  compiler-enforced rather than a matter of discipline, and the table cannot be
  profile-dependent.

  The seed was NOT searched for. No seed sweep was run and the hygiene properties held on
  the first seed tried. A searched seed and an arbitrary one are indistinguishable from the
  constant alone, so this project says which it is.

  AC4's evidence is ordered by what each mechanism actually proves, strongest first:
    1. per-position key literals -- the only pins that can see the index formula;
    2. the pinned SHA-256 of the 781 keys -- the only one that also kills per-BUILD entropy;
    3. `const _: () = assert!(...)` forcing const evaluation of the table -- the const
       interpreter has no clock, no I/O, no entropy and no FFI, so per-process variation is
       not detected but impossible;
    4. re-executing the test binary and comparing digests -- the literal reading of AC4.
  An O(N^2) const-time distinctness check was measured at roughly +450 ms on every compile
  of the workspace's dependency root and rejected against D-0014's stated value that
  `boid-board` "should compile in well under a second, forever"; distinctness is a runtime
  test instead.

Rule:         The zobrist tables must be `const`-evaluated from the hardcoded seed;
  `boid-board` must contain no `build.rs`; no key may derive from entropy, time, the
  environment, or any file hash. Determinism evidence must include at least one pin that
  fails under a per-BUILD entropy source, not only a per-PROCESS one. Named and rejected:
  `assert_eq!(build_tables(), build_tables())` within one process, which defeats none of
  those threat models and which `scripts/anti-theatre.sh` cannot catch.

Evidence:     Derived independently in Python before any Rust existed, and reproduced by the
  crate: `sha256(781 keys, little-endian) =
  79dbfe5ac62eb22d3e1961835c668ca10c79b467fa36b9c63391e78f5ea985a2`; 781 non-zero, 781
  distinct, all 304,590 pairwise XORs distinct and non-zero; splitmix64's published seed-0
  vectors reproduce (`E220A8397B1DCDAF 6E789E6AA1B965F4 06C45D188009454F F88BB8A8724C81EC
  1B39896A51A8749B`). The digest literal is committed in the RED commit that precedes any
  implementation, so `git show` proves it was not read off the code.

Consequences: A bug repro from #6 or #13 carries its position's key and that key means the
  same thing on every machine, every build and every process, forever -- which is the whole
  reason the issue forbids entropy.

---

## D-0021 — En passant is stored as a file and hashed unconditionally

Status:       Accepted
Date:         2026-08-17

Context:      Issue #4 states that "the en-passant square is set after every double push
  regardless of whether a capture is available, matching the convention the published perft
  counts use", and that the eight zobrist ep keys are FILE keys because "the rank is implied
  by side to move".

Decision:     `Board` stores `ep: Option<File>` and derives the target square from the side
  to move (rank 3 when Black is to move, rank 6 when White is to move). Storing the file
  rather than the square makes the contradictory state -- an ep square whose rank disagrees
  with the side to move -- unconstructible rather than merely rejected at the parser, and it
  costs nothing: both layouts measure 152 bytes.

  The file key is XORed whenever the FEN records an ep square, with no test for whether a
  capture is available. Stockfish does the opposite: it sets the ep field only when an enemy
  pawn attacks the target, and it NORMALISES AN UNCAPTURABLE EP SQUARE AWAY ON INPUT.
  Measured in this container: `position fen rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR
  w KQkq c6 0 2` then `d` echoes the FEN with `-` in the ep field, while the same position
  with a black pawn able to capture echoes the ep square unchanged. The divergence is
  invisible to perft -- neither convention adds or removes a move -- and visible to every
  FEN and key comparison.

  Over-distinguishing costs a transposition-table miss, which is slow and correct.
  Under-distinguishing, if the availability test is even slightly wrong, costs a table HIT
  on a different position, which is a wrong node count. That asymmetry is the whole
  argument, and it is why this project follows the issue rather than the engine.

Rule:         Issue #8 must not add an ep-availability test to the hash. Issue #5's
  `make_move` sets the ep file after every double push, unconditionally. `Board::from_fen`
  must not require that an ep capture be legal, or that a capturer even exist beyond the
  pawn that made the double push; the position `8/2p5/3p4/KP5r/1R2Pp1k/8/6P1/8 b - e3 0 1`
  -- position3 after `e2e4`, where the ep capture `f4e3` is illegal by a horizontal pin --
  is committed as the test of that.

Evidence:     Stockfish's echo behaviour, measured above. `f4e3` absent from Stockfish's
  16-move list for the pin position, and present once the b4 rook is removed.
  `zobrist_tables::nine_en_passant_states_are_pairwise_distinct` and
  `fen_roundtrip::uncapturable_en_passant_changes_the_key` pin our side.

Consequences: Threefold repetition will under-detect relative to a strict FIDE 9.2.2
  reading -- two positions differing only in a dead ep square hash differently and will not
  be seen as a repetition. That is the price of matching the published perft counts, and it
  is recorded here so #8 does not "fix" it without reading this entry.

---

## D-0022 — The pawn hash covers pawns only

Status:       Accepted
Date:         2026-08-17

Context:      Issue #4 requires `Board` to carry "a pawn hash" and says nothing else about
  it. No acceptance criterion constrains it. That combination makes it the field most likely
  to ship undecided and be quietly wrong forever -- and it is baked into every pinned
  position literal, so changing it later is not a local edit.

Decision:     The pawn hash is the XOR of the piece-square keys of the pawns of BOTH
  colours, drawn from the same `piece_square` table as the main key, with a base of 0 and
  with side-to-move, castling and en-passant excluded.

  Kings are deliberately NOT included, although many engines include them so that a
  pawn-structure cache can hold king-safety terms. The consumer here is issue #12's
  pawn-structure evaluator, which does not exist. Guessing the shape of a consumer that has
  not been written is exactly what D-0010 refused for `Evaluator`, D-0014 for perft divide
  and D-0018 for the trainer's boundary types; this is the fourth time and the answer is the
  same.

  Sharing the piece-square table rather than seeding a second one is not only cheaper. It
  makes an algebraic relation between the two hashes statable, and therefore testable: on a
  board of pawns and kings only, `key ^ pawn_key` is exactly the two king keys XORed with
  the side, castling and en-passant contributions. With two independently seeded tables
  there is no such relation to assert, and "both hashes are maintained" would have to be
  taken on trust.

Rule:         The pawn-key update lives inside `place` and `take` themselves, so no move
  application path can forget it -- a promotion and an en-passant capture are ordinary
  `take`/`place` sequences and are covered by construction. `pawn_key` must be invariant
  under a null move. Adding kings to it requires a superseding entry, because every pinned
  `pawn_key` literal in the test suite moves with that change.

Evidence:     `zobrist_incremental::pawn_key_is_invariant_under_every_non_pawn_edit` (all
  five non-pawn kinds and all three state setters), `pawn_key_tracks_a_promotion`,
  `pawnless_positions_have_a_zero_pawn_key`, and
  `key_xor_pawn_key_is_the_non_pawn_contribution`, which is the relation above.

Consequences: Issue #12 gets a hash that changes only when the pawn structure changes, and
  gets it maintained by the representation rather than by a rule its author has to remember.
  If it turns out to want kings, that is a superseding entry and a re-pinning of the
  literals, which is a visible change rather than a silent one.

---

## D-0023 — FEN strictness follows from byte-identical round-trip

Status:       Accepted
Date:         2026-08-17

Context:      Issue #4 AC1 requires that FEN parse then emit reproduces its input byte for
  byte. The emitter has exactly one spelling for any position, so any non-canonical input the
  parser ACCEPTS is an AC1 failure by construction. AC1 and AC2 are therefore one requirement
  seen from two sides, and every judgement call about leniency is already decided.

Decision:     Every normalisable spelling is an error, never a normalisation:

    castling rights out of KQkq order          CastlingOrder
    a repeated castling right                  CastlingDuplicate
    Shredder-FEN / X-FEN castling (HAha)       CastlingShredderNotation, named not mapped
    a leading zero in a counter (01)           ClockLeadingZero
    two adjacent placement digits (44 for 8)   ConsecutiveDigits
    the digits 0 and 9 in a placement rank     DigitOutOfRange
    an uppercase en-passant square (E6)        EnPassantSquare
    a doubled, leading or trailing separator   EmptyField
    a counter wider than the field stores      ClockOutOfRange -- rejected, never saturated

  Counter bounds are the widths of the fields the board stores -- 0..=255 plies and
  1..=65535 moves -- not a chess rule. Rejecting at 100 because of the fifty-move rule would
  refuse real puzzle exports, whose exporters routinely reset the fullmove number while
  leaving a large halfmove clock; nothing representable is lost, because the seventy-five-
  move rule ends a game at 150 plies.

  Parsing does not normalise the POSITION either: an en-passant square whose capture is
  unavailable is kept (D-0021), and castling rights are never dropped for being
  unexercisable. Stockfish does both of those on input. Either would break AC1.

  Two mechanical rules make AC2's "never panics" a property of the code rather than a claim
  about how much testing was done. First, the parser rejects any non-ASCII byte BEFORE
  anything else looks at the string, after which every byte index is a character boundary
  and no slice can split a code point. Second, `src/fen.rs` denies
  `clippy::indexing_slicing`, `unwrap_used`, `expect_used`, `panic` and
  `arithmetic_side_effects` at module scope, so the usual routes to a panic are closed by
  the compiler under the existing `-D warnings` CI job.

Rule:         `Board::from_fen` splits on ASCII `' '` and never uses `split_whitespace`,
  which treats U+00A0 as a separator and would parse an NBSP-separated FEN into a valid
  position here while Stockfish parses the same bytes into a different one (D-0008, followed
  through into our own parser). Parsing never rewrites what it was given: an inconsistent or
  non-canonical FEN is an error, never silently altered.

Evidence:     `fen_rejection::adversarial_inputs_map_to_specific_variants` pins ~70 inputs to
  named variants rather than to `is_err()`;
  `fen_rejection::every_declared_variant_is_reached_by_the_corpus` fails if the corpus stops
  exercising a rule; `nbsp_separated_fen_is_rejected`; `the_ascii_check_comes_first`;
  `single_byte_substitutions_never_panic` (several thousand near-miss FENs);
  `every_prefix_of_every_fixture_fen_is_rejected_or_parses`.

Consequences: Issue #7's UCI layer will meet FENs this crate refuses that other engines
  accept. That leniency belongs there, as a documented policy over `FenError`, and not here
  -- a lenient parser under a canonicalising emitter is exactly the pair that round-trips its
  own output forever and never round-trips its input.

---

## D-0024 — `FenLayout` resolves the four-field Kiwipete; no dialect bit enters `Board`

Status:       Accepted
Date:         2026-08-17

Context:      D-0008 stores the published Kiwipete FEN with four fields, as published,
  guarded by `kiwipete_fen_has_four_fields` and by the fixture's pinned SHA-256. AC1 requires
  byte-identical round-tripping of all six perft positions, and one of them therefore has no
  counters to emit.

Decision:     The field count is returned by the parser and passed to the emitter --
  `from_fen_with_layout` / `to_fen_with_layout` -- with `from_fen` / `to_fen` as the
  canonical six-field pair. Absent counters default to a halfmove clock of 0 and a fullmove
  number of 1.

  A "dialect bit" inside `Board` was rejected, and not merely because it would be meaningless
  after a move is made. Beside a zobrist key it is actively hazardous: it would make two
  positions with identical pieces, side, castling and en-passant compare unequal, and it
  would have to be excluded by hand from `Eq` and from every hash.

  Four-field emission from a board whose clocks are not the defaults is lossy, documented,
  and deliberately NOT fallible: the layout exists to reproduce a published FEN byte for
  byte, and a position that came from one carries the defaults anyway.

  Named and rejected so that review does not have to rediscover it: "emit four fields when
  the clocks are 0 and 1". The starting position IS 0 and 1, so that rule would emit four
  fields for the most-quoted FEN in chess and fail its own round-trip.

Rule:         `tests/fixtures/perft_oracle.txt` and `ORACLE_SHA256` are not to be edited by
  this or any later issue in order to simplify a parser. The fixture stores what was
  published; the code adapts to it.

Evidence:     `fen_roundtrip::kiwipete_round_trips_in_its_published_four_field_form`,
  `four_field_parse_defaults_the_clocks`, `four_field_emission_is_documented_lossy`,
  `startpos_would_not_round_trip_under_clock_sniffing`, and
  `zobrist_incremental::four_and_six_field_kiwipete_hash_identically`, which is the
  assertion a dialect bit inside `Board` would fail.

Consequences: Issue #19's PGN writer gets `write_fen`, which takes the layout as a
  parameter and writes into a caller-owned buffer, rather than a `Board` that remembers how
  it was spelled.

---

## D-0026 — proptest is `boid-board`'s first dev-dependency, and the zero-dependency stance is now enforced

Status:       Accepted
Date:         2026-08-17

Context:      D-0014 records that `boid-board` takes no external dependencies deliberately,
  and that this repository hand-rolled SHA-256 in a test rather than take one. Issue #4's AC1
  names proptest explicitly: "200 randomly generated legal positions (proptest)".

Decision:     `proptest = { version = "1.11.0", default-features = false, features =
  ["std", "bit-set"] }` under `[dev-dependencies]`, adding five entries to `Cargo.lock`
  (432 -> 437, measured).

  This NARROWS D-0014 rather than breaking it. A dev-dependency does not propagate to
  dependents, and `cargo build --workspace` -- AC1 command 1 of 3 from issue #3 -- compiles
  none of it. Shipping a hand-rolled substitute for a tool the customer named by name would
  be grading ourselves against a criterion we had rewritten, which is exactly what D-0002
  exists to prevent.

  `fork` and `timeout` are off. Both re-execute the test binary, which is unwanted next to
  this crate's own two-process determinism check, and `catch_unwind` -- the part that
  matters for shrinking a panic -- is gated on `std`, not on `fork`. A crate that denies
  unsafe code has no segfaults to survive.

  The 200-position corpus AC1 asks for is driven by this crate's own splitmix64 from a
  hardcoded seed rather than by proptest's value stream, for the reason issue #4 gives about
  the zobrist keys: a reproducible corpus means a reproducible failure. proptest is used for
  what it is better at, which is shrinking adversarial input for AC2.

  Discovered while checking this: the `repo-invariants` dependency-DAG diff filters
  `.path != null`, so it has only ever seen INTRA-workspace edges. The zero-dependency
  stance that `README.md` and `crates/boid-board/Cargo.toml` both assert has never been
  enforced by anything. A `cargo metadata` step now enforces it, with a positive control in
  the same step so that an empty result cannot mean "the query broke".

Rule:         `boid-board` has no `kind == null` dependencies, enforced by CI.
  `proptest-regressions/` is committed and must never be added to `.gitignore`: a
  counterexample that CI finds and then discards is precisely the shape
  `scripts/anti-theatre.sh` exists to punish. AC1's case count is asserted inside the test,
  not configured in a `ProptestConfig` field -- a number in a config is a setting, and
  D-0016 requires a numeric claim to be asserted.

Evidence:     `cargo metadata` reports no non-dev dependencies for `boid-board`;
  `Cargo.lock` grew by exactly five entries; `gitignore_anchoring::
  gitignore_does_not_ignore_proptest_regressions`;
  `fen_proptest::two_hundred_generated_positions_round_trip` asserts the corpus length.

Consequences: The nightly cold-cache job pays proptest's full compile every night. That is
  single-digit seconds beside a 23-billion-node perft replay, and the per-push AC1 path pays
  nothing at all.
