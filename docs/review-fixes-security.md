# Adversarial review fixes — security and resource exhaustion

Every finding below was demonstrated live by a security reviewer against a
running instance. Each one was **reproduced again here as a failing test**
before it was fixed, the failure captured, and the fix then shown to turn it
green. Where the fix landed as configuration rather than code, the fix was
temporarily undone to capture the red, then restored.

All terminal output in this document is real, pasted unedited.

**Scope.** `boidboard/src/lib.rs`, `boidboard/src/routes.rs`,
`boidboard/src/repositories/frame_queries.rs`, `boidboard/autumn.toml`,
`boidboard/autumn-dev.toml`, `boidboard/tests/integration.rs`,
`boidboard/migrations/20260816000006_scenarios_config_hash_unique/`, and the
repository-root `.gitignore`. No new dependencies. No `unwrap` / `expect` /
`panic!` outside tests.

## Result

| | before | after |
|---|---|---|
| `cargo test --workspace` | 325 passed | **392 passed** |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean | **clean** |
| `boidboard` integration tests | 5 | **18** |

Not all 67 of those are this change's: `boids-core` and `views` were being
worked on in the same tree. This change adds 13 integration tests and 5 unit
tests. Two consecutive full-suite runs, back to back:

```
$ cargo test --workspace -- --test-threads=1        # run A
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 38.72s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.93s
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.73s
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.09s
test result: ok. 238 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.35s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.75s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test --workspace -- --test-threads=1        # run B
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 52.24s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.90s
test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.79s
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.07s
test result: ok. 238 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.29s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.95s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --workspace --all-targets -- -D warnings
    Checking boidboard v0.1.0 (/home/user/boidboard/boidboard)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.70s
```

Run B is byte-identical to run A apart from timings — the integration file
commits to a shared dev database, so "passes twice consecutively" is a real
property to check rather than a formality. It passes because every new test
removes the `runs`, `frames` and `scenarios` rows it created *before* it
asserts, so a failing assertion cannot poison the tests after it. (The two
placeholder scenario rows the fixtures hang off are `NOT EXISTS`-guarded and
deliberately reused rather than re-created.)

## Summary

| ID | Finding | Verdict | Proved by |
|---|---|---|---|
| C1 | Harvest management API + admin UI mounted with no authentication | **FIXED** | `the_harvest_management_api_is_not_reachable_anonymously`, `the_opted_in_management_api_refuses_callers_without_the_token`, `the_opted_in_management_api_admits_the_configured_token`, `the_management_api_is_opt_in_and_off_by_default`, `the_token_comparison_accepts_only_the_whole_token` |
| H1 | `max_ticks` has a floor but no ceiling; malformed input silently defaulted | **FIXED** | `an_out_of_range_tick_budget_is_refused_with_an_explanation`, `an_unreadable_tick_budget_is_refused_rather_than_silently_defaulted`, `the_ceiling_itself_and_an_omitted_budget_are_both_accepted` |
| H1b | Unbounded concurrent runs | **FIXED** | `submissions_are_refused_once_the_bench_is_full_and_accepted_again_after` |
| H2 | `frames_for_run` loads every frame, then discards most | **FIXED** | `the_detail_query_asks_for_at_most_the_budget_and_only_the_ticks_it_named`, `the_detail_page_of_a_long_run_still_renders_from_the_budgeted_query`, 3 unit tests in `frame_queries.rs` |
| M1 | No CSRF token on any form; posture depends on the profile name | **FIXED** | `under_a_csrf_enabled_profile_a_forged_cross_origin_post_is_refused`, `under_a_csrf_enabled_profile_the_apps_own_form_still_submits` |
| M3 | `Access-Control-Allow-Origin: *`; `autumn.toml` overrides prod defaults | **FIXED** | config split, `autumn.toml` + `autumn-dev.toml` |
| L2 | `.gitignore` covers nothing that would hold a secret | **FIXED** | `git check-ignore` transcript below |
| — | `scenarios` find-or-create TOCTOU (the "if time allows" item) | **FIXED** | `the_schema_refuses_two_scenarios_with_the_same_config_hash`, `concurrent_submissions_of_one_preset_all_land_on_one_scenario` |

---

## C1 — the Harvest management API was public

**FIXED.**

### What was wrong

`lib.rs` mounted the plugin with `.api("/api/harvest")`, which nests the full
operator surface behind **no middleware**: start, cancel, pause, resume, signal,
a read endpoint that dumps the input and output of every execution, and an HTML
admin UI. The reviewer drove it anonymously and produced two outcomes the app
cannot recover from:

* **Provenance forgery.** An anonymous `steer` signal made run 128 simulate from
  tick 100 onward at `max_speed: 40.0` while `runs.config_snapshot` still said
  `2.0`. The detail page's "Reproducibility fingerprint" panel then asserts the
  run reproduces from its stored config. It does not.
* **Permanent stuck runs.** An anonymous engine-level cancel killed run 127's
  execution without running `finalize_run`, so `runs.status` stayed `running`
  forever. The app's own `POST /runs/127/cancel` then returns 422 and the
  progress fragment polls every two seconds in every open tab, indefinitely.
  One anonymous POST per run.

### Reproduced

`tests/integration.rs` replays the reviewer's three requests against the plugin
the application actually mounts, with the pre-fix `.api(HARVEST_API_PATH)`
restored:

```
$ cargo test -p boidboard --test integration -- --test-threads=1 harvest
test the_harvest_management_api_is_not_reachable_anonymously ... FAILED

---- the_harvest_management_api_is_not_reachable_anonymously stdout ----
thread 'the_harvest_management_api_is_not_reachable_anonymously' panicked at boidboard/tests/integration.rs:475:9:
assertion `left == right` failed: anonymous POST /api/harvest/workflows/simulation_workflow/start must not reach Harvest's management API: engine-level start/cancel/signal over every execution is not something an app with no login flow can expose. Got 201 Created
  left: 201
 right: 404

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.52s
```

### The decision: do not mount it by default

The plugin offers `api_with_auth` (embedder-supplied tower layer),
`api_with_role_auth` (the same plus a read-only tier keyed off an autumn-web
`Session` role) and `enable_api_tokens` (scoped `hvst_…` bearers). Reading
`autumn-harvest-plugin-0.5.0/src/plugin.rs`:

* `api_with_role_auth` is **not** usable here. Its read-only tier is driven by a
  `role` / `is_harvest_readonly` marker on an autumn-web `Session`, and Boidboard
  has no accounts, no login flow and no session to carry a role. It still needs
  an embedder authentication layer underneath; the role part would be decoration.
* `enable_api_tokens` alone is **not a gate**. Its own doc comment is explicit:
  *"A bearer that does not begin with `hvst_` (or an absent bearer) is passed
  through untouched, so token auth composes with — it never replaces — an
  embedder's own `api_with_auth` middleware."* An anonymous request would still
  sail through. It also wants a `harvest_api_tokens` table this app has no
  workflow for minting into.

So the honest options were (a) a single-credential token gate via
`api_with_auth`, or (b) not mounting it. **Both were taken, in that order:**

* **`harvest_plugin()` no longer calls `.api(..)` at all.** Nothing in the
  application needs it. Runs are started in-process through the
  `WorkflowHandleClient` the plugin installs, never over HTTP, and the only
  operation a user performs is `POST /runs/{id}/cancel`, which sends the
  *graceful* `cancel` signal the workflow finalizes on — precisely the operation
  the engine-level cancel got wrong. An unmounted route cannot be misconfigured.
* **`harvest_plugin_with_admin_api(token)`** mounts it at `/api/harvest` behind
  `api_with_auth` with an `Authorization: Bearer` gate. The comparison is
  whole-string and constant-time, so a caller cannot recover the token from
  response latency and a value that merely *starts* with the token is refused.
* **`run()` picks between them** from `BOIDBOARD_HARVEST_ADMIN_TOKEN`, read from
  the process environment with autumn-web's `.env` overlay as a fallback. Unset
  or blank means "not mounted". The token is a secret, so it lives in the
  environment or an ignored `.env` file — not in the tracked `autumn.toml`,
  which is also why L2 below matters.

The reasoning is recorded as a `# Why the management API is *not* mounted here`
section on `harvest_plugin`'s doc comment, at the mount site.

The integration tests keep working unchanged: they exercise the app through its
own routes and the in-process client, never over the management API.

### Green

```
$ cargo test -p boidboard --test integration -- --test-threads=1 management_api
running 3 tests
test the_harvest_management_api_is_not_reachable_anonymously ... ok
test the_opted_in_management_api_admits_the_configured_token ... ok
test the_opted_in_management_api_refuses_callers_without_the_token ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.82s
```

The anonymous test asserts **404** on all three of the reviewer's PoC requests
(`POST …/workflows/simulation_workflow/start`, `GET …/workflows`,
`POST …/ui/workflows/{exec}/cancel`). The token tests assert 401 for no bearer,
401 for a wrong bearer, 401 for `s3cret-operator-token-and-more` (a prefix
match must not pass), and not-401/not-404 for the configured token.

Two unit tests in `lib.rs` pin the two properties the whole gate rests on:

```
$ cargo test -p boidboard --lib
test tests::the_management_api_is_opt_in_and_off_by_default ... ok
test tests::the_token_comparison_accepts_only_the_whole_token ... ok
```

The first asserts that with nothing configured `harvest_admin_api_token()` is
`None` — a change that made an absent variable resolve to `Some("")` would
silently mount the console behind an empty bearer. The second covers the cases
a naive comparison gets wrong: a prefix, a truncation, a same-length mismatch,
and a case difference.

---

## H1 — `max_ticks` had a floor but no ceiling

**FIXED.**

### What was wrong

`max_ticks_or_default` was `.parse().ok().unwrap_or(DEFAULT).max(1)`. Two bugs
of the same class:

* `max_ticks=2147483647` was accepted — run 125 was created and started. At the
  measured ~40 ticks/s and ~9 KB of `agents` JSONB per frame that is ~1.7 years
  of worker time and ~19 TB of frame rows, from one form POST. The form's
  `min="1"` is client-side only.
* `max_ticks=4294967295` fails `parse::<i32>()` and silently became 300, so the
  user got a different experiment from the one they asked for and was told
  nothing.

### Reproduced

Pre-fix body restored (`raw.parse().unwrap_or(DEFAULT).max(1)`):

```
$ cargo test -p boidboard --test integration -- --test-threads=1 tick_budget
test an_out_of_range_tick_budget_is_refused_with_an_explanation ... FAILED
test an_unreadable_tick_budget_is_refused_rather_than_silently_defaulted ... FAILED

assertion `left == right` failed: max_ticks=2147483647 must be refused, not accepted: one form POST must not be able to book years of worker time. Got 303 See Other
  left: 303
 right: 422

assertion `left == right` failed: max_ticks=4294967295 is unreadable and must be reported, not silently replaced by the default. Got 303 See Other
  left: 303
 right: 422

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.81s
```

### The fix

`routes.rs`:

* `MAX_TICKS_CEILING = 100_000`, chosen against the workflow rather than as a
  round number: at `BATCH_TICKS = 50` that is 2 000 batches ≈ 6 000 history
  events, comfortably under Harvest's 10 000-event `continue_as_new` threshold,
  so a full-budget run still completes as a single execution and neither the
  resume cursor nor the frame sequence needs to learn about rotation.
* Out-of-range and unreadable input are **422 with an explanatory message that
  names the ceiling**, not a silent clamp. Clamping would run a different
  experiment and report success — the same category of quiet lie as substituting
  a default preset for an unknown slug, which this handler already refuses to do.
* Omitted / empty stays the documented default path: a browser sends an emptied
  number field as `""`, and the form advertises the fallback.
* `seed` got the same treatment — present-but-unreadable is now a 422 rather than
  a silent `seed = 1`, because a run's recorded seed is provenance.

### Green

```
$ cargo test -p boidboard --test integration -- --test-threads=1 tick_budget
test an_out_of_range_tick_budget_is_refused_with_an_explanation ... ok
test an_unreadable_tick_budget_is_refused_rather_than_silently_defaulted ... ok
test the_ceiling_itself_and_an_omitted_budget_are_both_accepted ... ok
```

The first test drives `2147483647`, `100001`, `0` and `-5` and asserts each is a
422 whose body names the ceiling. The second drives `4294967295`, `sixty` and
`1e6`. The third pins the inclusive boundary (exactly `MAX_TICKS_CEILING` is
accepted and stored) and the empty-field default path, so the fix cannot drift
into an off-by-one or into rejecting the form's own default.

### H1b — unbounded concurrent runs

`MAX_TICKS_CEILING` bounds one run; nothing bounded the fleet, and *n* runs at
the ceiling cost the same worker-years spread out. `create_run` now counts
non-terminal runs before writing anything and refuses past `MAX_ACTIVE_RUNS =
24` with a `503` that says what to do. The non-terminal status set is *derived*
from `status::is_terminal` rather than listed again, so a status added later is
counted without anyone remembering.

`503` rather than `429`: the request is well-formed and will be fine later,
which is what a saturated bench means; `AutumnError` carries no 429 constructor
and hand-rolling a bare response would lose the uniform error rendering every
other refusal in the file gets.

```
$ cargo test -p boidboard --test integration -- --test-threads=1 bench_is_full
test submissions_are_refused_once_the_bench_is_full_and_accepted_again_after ... ok
```

The test fills the bench with cheap undispatched rows, asserts the 503 and its
message, drains the bench **before** asserting (so a failure does not leave the
database saturated for the rest of the file), then proves the same submission
succeeds once there is room — a capacity limit, not a kill switch.

`[security.rate_limit]` is also now enabled in `autumn.toml` (see M3); it
defaults to `false` and had to be asked for.

---

## H2 — the "query budget" was not a query budget

**FIXED.**

### What was wrong

`routes.rs` claimed `DETAIL_FRAME_BUDGET` "is the *query* budget, and it is the
one that matters for latency". `frames_for_run` then `.load()`ed **every** frame
for the run and subsampled in Rust. Measured: a 301-frame run transferred
2.66 MB to render a 57 KB page, linear and unbounded; combined with H1 one
`GET /runs/{id}` became a multi-gigabyte allocation.

### Reproduced

A contiguous run cannot tell "sampled by tick in SQL" from "sampled by index in
Rust" — tick equals index there, which is exactly why the bug survived. A run
with a **hole** can. Pre-fix body restored:

```
$ cargo test -p boidboard --test integration -- --test-threads=1 the_detail_query_asks
test the_detail_query_asks_for_at_most_the_budget_and_only_the_ticks_it_named ... FAILED

thread '...' panicked at boidboard/tests/integration.rs:890:5:
every returned frame must be one the query named ([0, 25, 50, 75, 100]); got [0, 63, 75, 88, 100]. A tick outside that list can only have arrived by loading every frame and thinning afterwards.

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.52s
```

Ticks 63 and 88 are frames the query never asked for. They can only exist in the
result because all 51 rows were transferred first.

### The fix

`frame_queries.rs`, two round trips instead of one unbounded one:

1. `tick_bounds` — `SELECT min(tick), max(tick) … WHERE run_id = $1`. One row,
   index-only, cost independent of run length.
2. `sampled_ticks(lowest, highest, budget)` — a pure function returning the
   ≤ `budget` evenly spaced ticks the page wants, endpoints pinned, ascending
   and duplicate-free.
3. The fetch is `WHERE run_id = $1 AND tick = ANY($ticks) ORDER BY tick LIMIT
   $budget`.

The bound is **structural**: the `ANY` list is itself at most `budget` long, so
the database cannot build more rows than that however long the run is. The
`LIMIT` is belt as well as braces.

An explicit tick list was chosen over the `(tick - min) % stride = 0` form: it
needs no raw SQL fragment, produces exactly the wanted count rather than
"however many the modulus happens to match", and — because the list is a value
the test can compute and compare against — it is what makes the boundedness
*testable* rather than merely asserted.

`subsample` is kept as a final tidy-up. On the healthy path it is a no-op; it
exists so "at most `n`, endpoints included" still holds for a run whose tick
sequence has gaps.

The misleading comment in `routes.rs` was not deleted — it was made true, and
now carries a note recording that it *was* false and what changed.

### Green

```
$ cargo test -p boidboard --test integration -- --test-threads=1 detail
test the_detail_page_of_a_completed_run_draws_the_flock_and_its_hash ... ok
test the_detail_page_of_a_long_run_still_renders_from_the_budgeted_query ... ok
test the_detail_query_asks_for_at_most_the_budget_and_only_the_ticks_it_named ... ok
```

* `the_detail_query_asks_for_at_most_the_budget_and_only_the_ticks_it_named`
  seeds a 3 001-frame run and asserts the budgeted read returns *exactly* the
  ≤120 ticks the query named, then seeds a run with ticks `0` and `51..=100` and
  asserts every returned tick is one the query asked for (`[0, 75, 100]`) — the
  assertion that goes red under load-then-thin.
* `the_detail_page_of_a_long_run_still_renders_from_the_budgeted_query` proves
  the page is unchanged: a real 401-frame run renders with 120 agents drawn.
* Three unit tests in `frame_queries.rs` bound `sampled_ticks` directly, up to
  `i32::MAX - 1`, and pin it to the positions index subsampling used to choose
  (`sampled_ticks(0, 100, 5) == [0, 25, 50, 75, 100]`) so the change did not
  quietly move which frames the page draws.

---

## M1 — CSRF

**FIXED** (this half: config + the prod-profile test; the hidden `_csrf` field
in `views.rs` and the `Csrf` view type landed concurrently).

### What was wrong

Autumn's `prod` profile enables CSRF, `dev` disables it, and a **custom profile
name gets no smart defaults at all** — so `AUTUMN_PROFILE=staging` shipped with
CSRF off. `autumn.toml` said nothing either way, so the app was forgeable or
broken depending on one string. The reviewer demonstrated both halves: on dev a
cross-origin `POST /runs` from `Origin: https://evil.example` returned 303 and
created a run; with CSRF enabled `POST /runs` returned 403 because the
hand-written form emitted no `_csrf` field.

### Reproduced

There was **no test at any layer running under a production profile**, which is
why three separate "would this work in production?" findings went unnoticed at
once. With `security.csrf.enabled` flipped back to `false` in the new
prod-profile harness:

```
$ cargo test -p boidboard --test integration -- --test-threads=1 csrf_enabled_profile
thread 'under_a_csrf_enabled_profile_a_forged_cross_origin_post_is_refused' panicked at boidboard/tests/integration.rs:993:5:
assertion `left == right` failed: a POST carrying no CSRF token must be refused; on the dev profile this same request returned 303 and created a run. Got 303 See Other
  left: 303
 right: 403

thread 'under_a_csrf_enabled_profile_the_apps_own_form_still_submits' panicked at boidboard/tests/integration.rs:1021:10:
with CSRF enabled the new-run form must embed a `_csrf` field, or every submission is a 403 and the app is simply broken in production
```

Both halves of the finding, in one run.

### The fix

* `autumn.toml` states `[security.csrf] enabled = true` **explicitly**, so the
  posture is a property of the application rather than of a profile name.
* `routes::new_run_form` and `routes::run_detail` take
  `Option<CsrfToken>` / `Option<CsrfFormField>` and hand them to the views as a
  `views::Csrf`. `Option` matters: the extractor 500s when the layer is not
  mounted, and the pure route tests deliberately boot without it.
* Writing the prod-profile harness immediately turned up a **third**
  production-only failure: under `prod` autumn-web stops trusting `localhost`
  implicitly, so every request without a configured `[security.trusted_hosts]`
  entry is a `400 Invalid Host header`. That is recorded in the deployment
  checklist below.

### Green

```
$ cargo test -p boidboard --test integration -- --test-threads=1 csrf_enabled_profile
test under_a_csrf_enabled_profile_a_forged_cross_origin_post_is_refused ... ok
test under_a_csrf_enabled_profile_the_apps_own_form_still_submits ... ok
```

The second test is the shape that was missing: it boots under `prod` with CSRF
on, fetches `/runs/new`, reads the `_csrf` value out of the rendered form,
submits it the way a browser would (the client's cookie jar replays the CSRF
cookie) and asserts the result is **not** a 403 — then deletes the run, since no
worker is mounted and an orphan would hold a slot against `MAX_ACTIVE_RUNS`.

---

## M3 — CORS and the config split

**FIXED.**

`autumn.toml` was a dev file wearing a profile-independent name. Autumn merges
per-profile smart defaults **first** and `autumn.toml` **second**, so every key
in it silently overrides the `prod` defaults. It carried:

| key | prod smart default | what `autumn.toml` forced |
|---|---|---|
| `database.url` | (none — operator supplies) | the developer's `boidboard_dev` |
| `log.level` | `info` | `debug` |
| `log.format` | `Json` | `Pretty` (unparseable by log shipping) |
| `server.host` | `0.0.0.0` | `127.0.0.1` (unreachable behind a proxy) |

All four moved to the new **`autumn-dev.toml`**, which is loaded only when the
profile resolves to `dev` — which is the default for a debug build and therefore
what `cargo test` runs under, so the Harvest plugin's `AutumnConfig::load()`
still finds the dev database.

`autumn.toml` now holds only what is true of every profile, and states the
security posture explicitly rather than inheriting it:

* `[cors] allowed_origins = []` — the reviewer saw `Access-Control-Allow-Origin: *`
  on every response, inherited from the `dev` smart default. Empty means the CORS
  middleware is not applied at all. The whole UI is server-rendered HTML on one
  origin; there is no JSON API for a browser to call.
* `[security.csrf] enabled = true` (M1).
* `[security.rate_limit] enabled = true`, 20 rps / burst 60. `RateLimitConfig::enabled`
  defaults to `false`. The numbers are generous for what the app does — a detail
  page plus a progress fragment every two seconds per tab is well under 1 rps —
  while bounding a script that submits in a loop.

Both files carry a header comment explaining the layering, so the next person to
add a key knows which file it belongs in and why.

## L2 — `.gitignore`

**FIXED.** autumn-web loads `.env`, `.env.local`, `.env.{profile}` and
`.env.{profile}.local` into the config layer, and `BOIDBOARD_HARVEST_ADMIN_TOKEN`
— the credential that unlocks the engine — is read from exactly those files.
None of them were ignored: the first person to set it locally would have
committed it.

Added `.env`, `.env.*`, `!.env.example`, `*.pem`, `*.key`, with a comment
recording that **`autumn-dev.toml` stays tracked on purpose** — it is
configuration, not a secret, and the `.env*` rules are where anything with a
real credential goes.

```
$ git check-ignore -v boidboard/autumn-dev.toml boidboard/autumn.toml .env .env.local .env.prod.local .env.example server.key cert.pem
.gitignore:26:.env	.env
.gitignore:27:.env.*	.env.local
.gitignore:27:.env.*	.env.prod.local
.gitignore:28:!.env.example	.env.example
.gitignore:30:*.key	server.key
.gitignore:29:*.pem	cert.pem

$ git status --short --untracked-files=all
?? boidboard/autumn-dev.toml
```

`autumn-dev.toml` and `autumn.toml` are absent from the `check-ignore` output —
neither is ignored — and `autumn-dev.toml` shows up as an ordinary untracked
file ready to be committed.

---

## `scenarios` find-or-create was a TOCTOU

**FIXED** (the "if time allows" item).

`create_run` read `scenarios` by `config_hash` and inserted when it found
nothing. Two submissions of the same preset arriving together both saw no row
and both inserted one, leaving two scenarios that are the same scenario — which
breaks the only question `config_hash` exists to answer.

The generated `find_or_create_by_<field>` was not usable: it must be **declared**
in the `#[repository]` trait, and `repositories/scenario_repository.rs` is
outside this change's file scope. The same `INSERT … ON CONFLICT DO NOTHING` +
re-read shape was written directly in `routes.rs` instead, which is what the
macro generates anyway.

A **new** migration adds the UNIQUE index the conflict target requires
(`20260816000006_scenarios_config_hash_unique`); no applied migration was
edited. It replaces the plain `scenarios_config_hash_idx` with a unique index of
the same name, which serves the same lookups. Both databases were checked for
duplicate hashes first — `4 scenarios / 4 distinct hashes` in dev, empty in test
— so the index applies cleanly.

```
$ cargo test -p boidboard --test integration -- --test-threads=1 scenario
test concurrent_submissions_of_one_preset_all_land_on_one_scenario ... ok
test the_schema_refuses_two_scenarios_with_the_same_config_hash ... ok
```

`the_schema_refuses_two_scenarios_with_the_same_config_hash` inserts the same
`config_hash` twice and requires the second to fail — it passes only with the
UNIQUE index, which is what makes `ON CONFLICT (config_hash)` legal and the race
impossible. The invariant now lives in the schema, where a future read-then-write
cannot get it wrong either.

---

## Deployment checklist

Things a `prod` boot needs that a `dev` boot does not. The first was already
known; the second and third were found by writing the prod-profile test.

1. **Run the migrations first.** `database.auto_migrate_in_production` defaults
   to `false`, so a `prod` boot does **not** apply pending migrations. Run
   `autumn migrate` against the target database before starting the server, or
   the first request hits tables that do not exist. This now includes
   `20260816000006_scenarios_config_hash_unique`.
2. **Name the hostnames.** Under `prod`, autumn-web stops implicitly trusting
   `localhost` / `127.0.0.1`, and a request whose `Host` is not listed under
   `[security.trusted_hosts] hosts` gets `400 Invalid Host header` — including a
   request with no `Host` at all. Set them for the real deployment.
3. **Set `[security.signing_secret]`.** Under `prod` session cookies are signed;
   with no configured secret an ephemeral key is generated per process, so every
   restart (and every replica) invalidates the others' cookies.
4. **Supply `database.url` and `log` settings from the environment or a
   `autumn-prod.toml`.** They deliberately no longer live in `autumn.toml`.
5. **Decide about the Harvest console.** It is unmounted unless
   `BOIDBOARD_HARVEST_ADMIN_TOKEN` is set. If you set it, treat the value as a
   root credential: it grants engine-level control over every execution, with no
   per-user attribution, and it belongs in a secret store rather than in any
   tracked file.

## Dev-database cleanup

The reviewer's leftovers were removed and the historic orphans finalized:

* Runs 125–129 deleted along with their frames and signals (2 056 frame rows).
  Run 127 was the one left stuck `running` by the anonymous engine cancel and
  could not be cleared through the UI.
* 32 non-terminal runs left behind by earlier development sessions — `queued`
  rows from before the dispatch seam existed, and `running` rows whose worker
  processes are long gone — were marked `failed` with an explanatory `error`
  rather than deleted, so their frames and history survive.
* The two 2 147 483 647-tick executions created while reproducing H1 were
  cancelled at the engine level as well as in `runs`, so no future worker
  resumes them.

Immediately after the cleanup: `cancelled 22 / completed 97 / failed 32`, no
non-terminal runs, and no `RUNNING` Harvest executions. After the two full-suite
runs above the totals are higher (`cancelled 55 / completed 202 / failed 32`),
but the shape is the one that matters and it holds: **zero** `queued` or
`running` rows, and **zero** `RUNNING` executions. A suite that left either
behind would be re-creating the condition H1b now guards against.
