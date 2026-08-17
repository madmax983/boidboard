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

## D-0018 — The representation contract, frozen before any key is computed

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4 fixes the shape of the zobrist key set (768 piece-square keys, one
  side-to-move key, four castling keys, eight en-passant FILE keys) but not the orderings
  that turn that shape into 781 specific numbers. Those orderings are unobservable from
  outside -- two implementations can agree on every published perft count and still hold
  different tables -- so they must be written down before the first key exists, or the
  first digest to be pinned silently becomes the specification.

Decision:     Four orderings are frozen, in this entry, before `src/zobrist.rs` is written:

    1. SQUARES are LERF: a1 = 0, b1 = 1, ... h8 = 63. `square = rank * 8 + file`.
    2. PIECES are `kind * 2 + colour`, with kind order Pawn, Knight, Bishop, Rook, Queen,
       King and colour order White, Black. So WhitePawn = 0, BlackPawn = 1, ...,
       BlackKing = 11. This ordering, not the more common colour-major one, is chosen so
       that the pawn keys are the CONTIGUOUS prefix `[0, 128)` of the piece-square block
       and the pawn hash needs no second table -- which keeps the issue's "768 keys"
       literal rather than approximate.
    3. TABLE ORDER is piece-square (768), then side-to-move (1), then castling (4, in the
       order WK WQ BK BQ), then en-passant file (8, a..h). Index arithmetic for the
       piece-square block is `piece * 64 + square`, never `square * 12 + piece`.
    4. DIGEST SERIALISATION is table index order, each key `to_be_bytes()`, SHA-256 of the
       concatenation. Big-endian is deliberate: every runner this project has is x86-64, so
       a `to_ne_bytes` digest would make an endianness bug permanently invisible.

Rule:         The four orderings above are frozen; changing any of them requires a
  superseding entry AND a re-derived digest. A number measuring this project may not enter
  this log before the commit that lands the test asserting it (D-0016).

Evidence:     `crates/boid-board/tests/zobrist_table.rs` asserts the index arithmetic
  against a directly computed index, and `zobrist_digest.rs` pins the digest.

Consequences: Issue #5's magic tables and issue #6's hashed perft can assume the square
  numbering without re-deriving it, and a reviewer can recompute any single key from this
  entry plus the seed.

---

## D-0019 — En passant is stored as a FILE, and it is always set

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4 mandates eight en-passant FILE keys, "the rank is implied by side to
  move", and that "the en-passant square is set after every double push regardless of
  whether a capture is available, matching the convention the published perft counts use".
  There is a second, equally common convention -- record the square only when a capture is
  actually available -- and Stockfish 16 uses it.

Decision:     `Board` stores `Option<File>`, never a square. The rank is derived on read:
  rank 6 when White is to move, rank 3 when Black is. That derivation is not an assumption;
  `oracle::validate_fen` already REJECTS the contradictory pairing, so the invariant is
  enforced at every entry point into the type.

  Storing a file rather than a square is what makes AC6's second clause -- "positions
  differing only in an ep square that FEN records identically do not [produce different
  keys]" -- a theorem about the type rather than a property of the hash that a test must
  chase. There is no representable state in which two boards differ by an en-passant RANK.

  The always-set convention (issue #4's) diverges from Stockfish's FEN echo. Measured in
  this container on 2026-08-16:

      position startpos moves e2e4    -> Stockfish `d` prints "... b KQkq - 0 1"
      fen "... b KQkq e3 0 1"         -> Stockfish `d` prints "... b KQkq - 0 1"
      fen ".../3pP3/... w KQkq d6 0 3" -> Stockfish `d` prints "... w KQkq d6 0 3"

  Perft counts are unaffected: the conventions differ only in whether a legally unusable
  target is RECORDED, never in which moves exist. The always-set convention is also the
  conservative one for issue #6's hashed perft -- it can fail to merge two equivalent
  positions, costing speed, but can never merge two different ones, costing correctness.

Rule:         `Board` must never store an en-passant rank, and the en-passant file is set
  after every double push regardless of capturability. No field of `Board` may be undefined
  after `apply_move`.

Evidence:     `zobrist_incremental::e3_with_black_to_move_and_e6_with_white_differ_by_exactly_the_side_key`,
  and the Stockfish differential harness excludes the ep field by name rather than by
  accident.

Consequences: The Stockfish FEN oracle corroborates five of the six FEN fields. The sixth is
  a declared divergence with a test of its own, not an unexamined gap.

---

## D-0020 — The accepted FEN language is canonical FEN plus one declared elision

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4 AC1 requires FEN parse -> emit to "round-trip byte-identically for
  all six perft positions". D-0008 stores the published Kiwipete FEN with FOUR fields, and
  forbids appending " 0 1" to it. A canonical six-field emitter therefore cannot return
  Kiwipete's stored bytes, and no reading of AC1 makes all six rows byte-identical.

Decision:     `to_fen` always emits six fields. `Board` carries no memory of how many
  fields its source text had. Every alternative considered -- a `layout` field, a
  `fullmove == 0` sentinel, a `FenText` newtype, a `(Board, FenShape)` return -- pays for
  AC1's literal wording by making `Board` worse: the field would be compared by `PartialEq`,
  copied on every `apply_move`, and would make two identical positions unequal over a
  formatting detail.

  AC1 is therefore restated as a LAW over the accepted language, and the restatement is
  declared here rather than applied silently:

      for every FEN f that `from_fen` accepts,
        to_fen(from_fen(f)) == f                 when f has six fields
        to_fen(from_fen(f)) == f + " 0 1"        when f has four fields
      and there is no third case.

  The four-field canonicalisation is not this project's invention. Stockfish 16, fed the
  published four-field Kiwipete FEN, echoes it with " 0 1" appended (measured 2026-08-16),
  so the elision's expansion is externally corroborated rather than chosen here.

Rule:         For every FEN `from_fen` accepts, `to_fen(from_fen(f))` equals `f` when `f`
  has six fields and `f` followed by " 0 1" when it has four; there is no third case.
  `tests/fixtures/perft_oracle.txt` is not edited to make this easier.

Evidence:     `fen_roundtrip::six_field_fens_round_trip_byte_identically`,
  `fen_roundtrip::the_four_field_kiwipete_fen_round_trips_to_its_canonical_six_field_form`,
  and an empty `git diff origin/main -- tests/fixtures/perft_oracle.txt`.

Consequences: AC1 is reported as satisfied under a declared narrowing, and the narrowing is
  a stronger statement than the AC's wording -- it quantifies over the whole accepted
  language rather than over seven rows.

---

## D-0021 — What `from_fen` deliberately does not enforce, and who owns each rule

Status:       Accepted
Date:         2026-08-16

Context:      A FEN reader can be arbitrarily strict. Strictness that is not written down
  is indistinguishable from strictness that was not considered, and a rule that is silently
  absent is discovered in issue #6 as a phantom move-generation bug.

Decision:     `Board::from_fen` enforces: field count (4 or 6); ASCII only; eight ranks;
  no consecutive skip digits; exactly eight files per rank; exactly one king per side; no
  pawn on rank 1 or 8; a castling right only when its king and rook stand on their home
  squares; an en-passant target on the rank implied by the side to move, with the target
  square empty, the square behind it empty, and the double-pushed pawn present; decimal
  clocks with no leading zeros, halfmove <= 65535 and fullmove in 1..=65535.

  It deliberately does NOT enforce, with owners:

    - side-not-to-move is in check          -- needs attack tables; issue #5.
    - the position is reachable from the initial array (promotion budgets, bishop parity)
                                            -- not decidable cheaply; no owner, and no
                                               engine needs it.
    - Shredder / X-FEN castling notation (`HAha`) -- Chess960 is out of scope per issue #5;
                                               rejected by name so a wider EPD suite in #6
                                               gets a useful error rather than a confusing
                                               one.
    - halfmove clock consistency with the fullmove number -- not a chess rule.

Rule:         Every rule `from_fen` deliberately does not enforce must have a test
  asserting that a FEN violating it is ACCEPTED, so that the boundary is a choice on
  record rather than an omission.

Evidence:     `crates/boid-board/tests/fen_language.rs`.

Consequences: Issue #5 inherits a written list of what it must add, and issue #6 knows
  which rejections to expect when it feeds a wider suite in.

---

## D-0022 — `FenError` replaces `Result<(), String>`, and there is one parser

Status:       Accepted
Date:         2026-08-16

Context:      D-0014 deferred to this issue: "`validate_fen` returns `Result<(), String>`.
  An unmatchable error ... Correct for a fixture validator, wrong for the FEN reader issue
  #4 will build on it: a caller that wants to branch on *why* a FEN was rejected cannot.
  Deferred to #4, which should introduce a typed `FenError` and have this function return
  it."

Decision:     The deferral is closed. `fen::FenError` is a flat, `Copy`, scalar-payload
  enum, and `oracle::validate_fen` becomes a delegate to `Board::from_fen`, so the
  repository holds ONE FEN parser rather than two that can drift.

  The delegation is only sound because the two callers want the same strictness. They do:
  every rule in D-0021 is a rule a fixture row must also satisfy. The one property that
  must survive is the four-field form -- the fixture stores Kiwipete that way and D-0008
  forbids changing it.

Rule:         `oracle::validate_fen` must accept a four-field FEN forever, and every
  negative oracle test must name the specific `FenError` it expects rather than matching
  `MalformedFen { .. }`.

Evidence:     `crates/boid-board/tests/oracle_validation.rs`; collapsing every `FenError`
  variant to one value reddens the negative tests rather than leaving them green.

Consequences: A single parser means a strictness change is a single edit with a single
  blast radius, and the oracle's rejections gain the diagnosis they lacked.

---

## D-0023 — The seam between issue #4 and issue #5, and the overlap this issue takes on

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4's body promises "an incrementally maintained zobrist key" and its
  AC5 requires two positions "reached by different move orders". Neither is deliverable
  without applying moves. Issue #5's body, however, says in as many words that "`apply_move`
  is immutable copy-on-write and maintains the zobrist key incrementally, with a
  `#[cfg(debug_assertions)]` recompute-from-scratch assertion at the end of every call",
  and its AC3 names en-passant, castling-rights and promotion application tests.

  The two issues therefore overlap, and the overlap has to be resolved somewhere. Resolving
  it in #4's favour is a RE-SCOPE, and re-scoping silently is the failure this log exists
  to prevent.

Decision:     Issue #4 ships `Move`, `MoveKind`, UCI conversion, `Board::apply_move` for
  all move kinds, `try_apply_move`, and `check_invariants`. The dividing sentence is:

      move APPLICATION needs no attack tables; move GENERATION and LEGALITY need nothing
      else.

  Issue #4 therefore contains no attack table, no `attackers_to`, no `is_square_attacked`,
  no check detection, no move generation, and no null move. Issue #5 retains all of those,
  plus `cargo bench` and its no-`unsafe` criterion.

  The alternative -- discharging AC5 with two `from_fen` calls and shipping no applier --
  was rejected because it makes "reached by different move orders" a fiction: it asserts
  that two identical FEN strings hash identically, which tests the parser, not the key.

  Issue #5 AC2 (a `#[should_panic]` test that corrupts one XOR and proves the debug
  assertion fires) becomes harder, because #4 lands the assertion rather than #5. #4
  mitigates by shipping `check_invariants()` as a PUBLIC `Result`-returning method, so #5
  can build a corrupted board by hand and assert on it without a corruption hook inside
  `apply_move` -- a test-only mutation path is exactly what issue #6 AC6 forbids elsewhere.

Rule:         Issue #4 ships no attack table, no move generation and no legality decision;
  issue #5 ships no second `apply_move`. The overlap is reported on issue #5, not absorbed
  in silence.

Evidence:     A `repo-invariants` step asserts no function under `crates/` matches
  `attack|attacker|is_check|square_attacked` until issue #5 lands.

Consequences: D-0014's second deferral closes: issue #6's `PerftEngine::divide` now has an
  expressible return type, `Result<Vec<(Move, u64)>, EngineError>`, because `Move` exists.

---

## D-0024 — Zobrist keys: splitmix64, published constants, a hardcoded seed, no seed search

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4: "Zobrist keys are generated by splitmix64 from a hardcoded seed in
  a `const fn` -- never from entropy, because reproducible bug repros and reproducible
  hashed-perft results are the whole point."

Decision:     splitmix64 with Vigna's published constants -- increment
  `0x9E3779B97F4A7C15`, multipliers `0xBF58476D1CE4E5B9` and `0x94D049BB133111EB`, shifts
  30 / 27 / 31. Published constants rather than project-chosen ones, because they are what
  makes the table regenerable by an outsider from the algorithm's name alone.

  The seed is `u64::from_be_bytes(*b"boidbord")`, written as that expression rather than as
  a hex literal so that its provenance is visible in the source. NO SEED SEARCH WAS
  PERFORMED: no seed was tried, measured, and kept. A searched seed would make every
  structural assertion about the key set (all distinct, none zero, no low-weight GF(2)
  dependency) a fitted result rather than a property of the generator.

  "Never from entropy" is proven structurally rather than behaviourally. The load-bearing
  mechanism is `const _: [ZobristKey; KEY_COUNT] = build_table();` at item scope, which
  forces compile-time evaluation of the whole table: entropy inside a `const fn` is a
  COMPILE ERROR, not a test failure. A test that builds the table twice in one process and
  compares is explicitly NOT relied upon -- it is green for a `OnceLock` seeded from
  `getrandom`, which is the exact defect it appears to exclude.

Rule:         Zobrist keys are produced by a `const fn` from a hardcoded seed; no seed may
  be chosen by inspecting its output, and no runtime initialisation is permitted.

Evidence:     `zobrist_table::splitmix64_matches_the_published_vectors` (vectors typed from
  outside this project), the `const _` forcing item, `scripts/zobrist-reference.py` as an
  independent derivation diffed in CI, and
  `zobrist_digest::zobrist_keys_are_baked_into_the_binary`.

Consequences: A bug repro from issue #6 quotes a key and the key means the same thing on
  every machine, forever, including a machine that has never run this repository's tests.

---

## D-0025 — proptest is a dev-dependency, and the corpus is not a "legal position" corpus

Status:       Accepted
Date:         2026-08-16

Context:      Issue #4 AC1 requires the round trip to hold "for all six perft positions plus
  200 randomly generated legal positions (proptest)". Two things in that sentence needed
  resolving: whether to take on a dependency in the crate whose manifest says it has none,
  and whether this issue can produce a *legal* position at all.

Decision:     **proptest is used**, because the customer named it. Replacing a named tool
  with a hand-rolled loop would be this project choosing its own acceptance criteria, which
  is precisely what D-0002 exists to prevent. It is a DEV-dependency, so:

    - `cargo build --workspace` compiles none of it; the "zero external dependencies"
      property the manifest claims is about `[dependencies]` and remains exactly true.
    - the `repo-invariants` dependency-DAG diff filters to normal dependencies
      (`select(.kind == null)`), so the declared DAG is unchanged.

  Default features are OFF. They pull `rusty-fork`, `tempfile`, `rustix` and `wait-timeout`
  for a fork-and-timeout harness this crate does not use; measured, turning them off takes
  the transitive tree from 39 crates to 18.

  The runner is seeded with `TestRng::deterministic_rng(RngAlgorithm::ChaCha)` rather than
  from entropy, for the same reason the issue gives for the zobrist seed: a failure must
  reproduce. `failure_persistence` is therefore `None` -- with a fixed seed the failing case
  is regenerated on the next run rather than remembered in an untracked file.

  **The corpus is not described as "legal".** Whether the side not to move is in check
  cannot be decided without attack tables, which are issue #5's. The generated positions are
  positions in the ACCEPTED LANGUAGE: structurally valid, two kings that do not touch, no
  back-rank pawns, castling rights only where the king and rook are home, an en-passant file
  only where a real double push could have left one. Reporting them as "200 random legal
  positions" would be a claim this project cannot support.

  The generator emits TEXT and never constructs a `Board`. A corpus produced by calling
  `to_fen` would be by construction the set the parser accepts, and round-tripping it would
  assert nothing about either half. That this matters is not hypothetical: on its first run
  the generator and the parser disagreed, and the parser was right -- the generator had put
  the square a double-pushed pawn came FROM one rank too far away.

Rule:         `boid-board` takes no normal dependencies. Any dev-dependency must be named by
  an acceptance criterion, declared with `default-features = false`, and seeded
  deterministically. The generated corpus must not be reused as a perft corpus until issue
  #5 can filter positions by check.

Evidence:     `crates/boid-board/tests/fen_roundtrip.rs` asserts 200 cases and the coverage
  buckets; `cargo tree` shows 18 crates; `cargo build --workspace` compiles none of them.

Consequences: `cargo test` pays a one-off compile for 18 crates. `cargo build` pays nothing.

---

## D-0026 — The position type is `Board`, and `apply_move` may not produce an unrepresentable one

Status:       Accepted
Date:         2026-08-16

Context:      D-0010's prose named the position type `Position`; issue #4 names it `Board`.
  Separately, the random-walk guard found that `try_apply_move` accepted a pawn moving onto
  the last rank without promoting, producing a board that `to_fen` would emit and
  `from_fen` would reject.

Decision:     The type is `Board`. D-0010's prose is superseded by number rather than
  edited, since this log is append-only.

  And the round trip is an INVARIANT of move application, not merely a property of parsing:
  every board `apply_move` can produce must be readable back from its own FEN.
  `MoveNotApplicable::PawnWouldNotPromote` is the missing precondition that makes it true.

Rule:         The position type is `Board`. Every board `apply_move` can produce must
  satisfy `from_fen(&b.to_fen()) == Ok(b)`.

Evidence:     `board_apply::try_apply_move_rejects_structurally_impossible_moves`, and the
  random walk, which asserts the FEN round trip at every ply of 500+ applied moves.

Consequences: Issue #5's move generator cannot produce a non-promoting pawn move to the
  last rank without `try_apply_move` rejecting it, which is a free correctness check on the
  generator it is about to write.

---

## D-0027 — The measured register for issue #4

Status:       Accepted
Date:         2026-08-16

Context:      D-0016 requires that a numeric claim about this project, made anywhere, be
  asserted by a test if it is asserted at all. These are issue #4's numbers, each with the
  test that holds it.

Decision:     The register:

    size_of::<Board>()          152 bytes, align 8, no padding
                                board_layout::board_is_exactly_one_hundred_and_fifty_two_bytes
                                and a const array-length item that fails at COMPILE time
    zobrist key count           781 = 768 + 1 + 4 + 8
                                zobrist_table::key_count_is_the_issues_arithmetic
    zobrist table digest        31e98d78da31b5d7439ed0601a698f7ab6dc54499d3e0be77d3be645dee2a314
                                zobrist_digest::the_key_digest_is_pinned, plus
                                scripts/zobrist-reference.py diffed by CI
    mean key population count   31.881, within a 28..=36 sanity band
                                zobrist_table::the_mean_population_count_is_close_to_half_the_word
    size_of::<Move>()           2 bytes
    castling revocation squares six non-zero RIGHTS_LOST entries
    generated corpus            exactly 200 cases, 190+ distinct, 8+ castling masks,
                                4+ en-passant states, both sides, one clock >= 100
                                fen_roundtrip::the_generated_corpus_round_trips_byte_identically
    proptest transitive tree    18 crates with default features off (39 with them on)
    Stockfish differential      179 legal root moves over 6 positions, agreeing on five of
                                the six FEN fields
                                apply_move_differential::applying_every_legal_root_move_agrees_with_stockfish

  Deferrals recorded rather than skipped, each with the issue that revisits it:

    - side-not-to-move-in-check is not detected                         issue #5
    - `PerftEngine::divide` is still not on the trait, but its return type is now
      expressible as `Result<Vec<(Move, u64)>, EngineError>` because `Move` exists  issue #6
    - repetition and fifty-move detection need a zobrist history threaded through the
      search stack; `Board::key()` is public so that history can hold it     issues #8, #9
    - Chess960 / Shredder castling notation is rejected by name             no owner yet
    - the en-passant convention that records a square only when a capture is available is
      implementable here and is deliberately not implemented, because the issue mandates
      the other one and it is the conservative choice for hashed perft       issue #8

Rule:         Every number in this register must remain asserted by the test named beside
  it; a number that loses its test is deleted from the register in the same change.

Evidence:     `cargo test --workspace` -- 167 tests.

Consequences: Issue #5 inherits a written list of what it must add rather than a guess.
