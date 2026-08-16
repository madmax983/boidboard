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
