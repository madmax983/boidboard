# TDD log — web interface

Every behaviour below was built **red → green → refactor**. The `RED` blocks
contain **real, unedited (only trimmed) output** captured from
`cargo test -p boidboard --test web` *before* the implementation existed.

Test file: `boidboard/tests/web.rs`
Implementation: `boidboard/src/presets.rs`, `boidboard/src/views.rs`,
`boidboard/src/routes.rs`

Two test layers, deliberately separated:

* **View unit tests** take plain data and return `Markup`. No `TestApp`, no
  database, no async runtime. That separation is the executable proof of
  **AC-50**: a view that reached for a connection could not appear in this
  layer at all.
* **Route tests** drive `autumn_web::test::TestApp` and assert on parsed HTML
  *structure* via `assert_selector` / `assert_selector_count` / `assert_attr` /
  `assert_text` rather than raw substrings (**AC-48**).

---

### AC-42 — named presets front the new-run form (data layer)

**RED** — `ac42_presets_offer_at_least_four_distinct_named_starting_points`,
`ac42_presets_have_genuinely_different_character`,
`ac42_by_slug_round_trips_and_rejects_unknown_slugs`,
`ac42_every_preset_is_a_valid_simulation_config`,
`ac42_presets_serialize_to_json_for_the_config_snapshot`

```
error[E0425]: cannot find function `all` in module `presets`
  --> boidboard/tests/web.rs:18:24
   |
18 |     let all = presets::all();
   |                        ^^^ not found in `presets`
   |
help: consider importing this function
   |
12 + use diesel::dsl::all;
   |
...
For more information about this error, try `rustc --explain E0425`.
error: could not compile `boidboard` (test "web") due to 8 previous errors
```

**GREEN** — `presets.rs` gained `Preset { slug, name, description, params }`
plus `all()` / `by_slug()`, with four presets whose steering-weight profiles are
all distinct: `classic-flock`, `nervous-swarm`, `highway` (goal + obstacles),
`scatter`.

**REFACTOR** — none needed; the four constructors are private `fn`s so `all()`
and `by_slug()` are the only surface, and `..SimParams::default()` keeps each
preset's declaration to just the fields that make it that preset.

> Note on `boids_core::sim::validate`: it did **not** exist when this cycle ran
> (a concurrent agent owns `sim.rs`), so
> `ac42_every_preset_is_a_valid_simulation_config` first asserted the invariants
> directly — positive world dimensions, `agent_count > 0`, `dt > 0`,
> `separation_radius <= neighbor_radius`, positive speed/force/radii — behind a
> `TODO` naming `validate` as the replacement.
>
> **Follow-up (later in this session):** `validate` landed, the `TODO` was
> honoured, and the test now calls
> `boids_core::sim::validate(&p.params)` for every preset — the kernel's own
> validator is the authority, because a preset the simulator would refuse only
> fails *after* the run is queued. The hand-rolled invariants were kept
> underneath it so the test still states its intent if `validate` is ever
> relaxed. All four presets passed on the first run against the real validator.

---

### AC-43 (a) — the flock renders as inline SVG with **oriented** agent marks

**RED** — `ac43_flock_svg_is_inline_svg_with_a_world_sized_viewbox`,
`ac43_every_agent_is_an_oriented_mark_rotated_to_its_heading`,
`ac43_flock_svg_renders_obstacles_and_survives_degenerate_input`,
`ac43_flock_svg_respects_its_agent_budget`

```
    |                       ^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `flock_svg` in module `views`
   --> boidboard/tests/web.rs:307:23
    |
307 |     let html = views::flock_svg(
    |                       ^^^^^^^^^ not found in `views`

error[E0422]: cannot find struct, variant or union type `FlockOpts` in module `views`
   --> boidboard/tests/web.rs:337:23
    |
337 |     let opts = views::FlockOpts {
    |                       ^^^^^^^^^ not found in `views`

Some errors have detailed explanations: E0422, E0425.
For more information about an error, try `rustc --explain E0422`.
error: could not compile `boidboard` (test "web") due to 11 previous errors
```

**GREEN** — `views::flock_svg(agents, world, obstacles, FlockOpts)` emits an
`svg.flock` with a world-sized `viewBox`, a `rect.world-bounds`, one
`circle.obstacle` per obstacle and one `polygon.agent` per agent, each placed
and rotated with `transform="translate(x y) rotate(deg)"` where
`deg = atan2(vy, vx)` in SVG's screen frame.

**REFACTOR** — pulled two helpers out of the first draft, both now shared with
every later view: `num()` (three-decimal, trailing-zero-trimmed, and
`NaN`/`inf` → `0`, because a `NaN` in a `points` attribute silently drops the
whole shape) and `even_indices()` (the endpoint-pinning subsample that all
three rendering budgets are built on).

---

### AC-43 (b) — trajectory ribbons, budgeted and broken at the toroidal seam

**RED** — `ac43_trajectory_svg_draws_one_ribbon_group_per_agent`,
`ac43_trajectory_ribbons_break_at_the_toroidal_seam`,
`ac43_trajectory_svg_enforces_its_rendering_budget`,
`ac43_trajectory_svg_handles_empty_and_single_frame_input`

```
error[E0433]: failed to resolve: could not find `TrajectoryOpts` in `views`
   --> boidboard/tests/web.rs:487:23
487 |     let opts = views::TrajectoryOpts::default();

error[E0425]: cannot find function `trajectory_svg` in module `views`
   --> boidboard/tests/web.rs:382:16
382 |         views::trajectory_svg(&frames, &world, views::TrajectoryOpts::default()).into_string();

error[E0425]: cannot find function `trajectory_svg` in module `views`
   --> boidboard/tests/web.rs:412:23
412 |     let html = views::trajectory_svg(

Some errors have detailed explanations: E0425, E0433.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `boidboard` (test "web") due to 9 previous errors
```

**GREEN** — `views::trajectory_svg(frames, world, TrajectoryOpts)` emits one
`g.trajectory[data-agent-id]` per sampled agent containing one
`polyline.trail` **per unbroken segment**. `seam_split` cuts the path wherever
a step exceeds half a world dimension — the minimum-image convention makes half
a world the longest step the torus admits, so a longer one is necessarily a
wrap, not motion. Budgets: `max_frames = 60`, `max_agents = 30`, both
subsampled with the endpoint-pinning `even_indices`.

**REFACTOR** — a one-point segment (an agent seen in exactly one sampled frame,
or a wrap on the very last step) was originally emitted as a one-point
`polyline`, which is silently invisible in every renderer. It now renders as
`circle.trail-dot`, so no sampled position is dropped from the picture; the
degenerate-input test pins that.

---

### AC-44 — metric sparklines, including the degenerate series

**RED** — `ac44_sparkline_maps_the_series_across_a_stable_viewbox`,
`ac44_sparkline_survives_empty_single_and_flat_series`,
`ac44_metrics_panel_gives_every_headline_metric_its_own_sparkline`,
`ac44_metrics_panel_renders_for_a_run_with_no_frames_yet`

```
help: consider importing this function
    |
 12 + use autumn_web::widgets::sparkline;
    |
help: if you import `sparkline`, refer to it directly
    |
583 -     let dirty = views::sparkline(&[1.0, f64::NAN, 3.0], opts).into_string();
583 +     let dirty = sparkline(&[1.0, f64::NAN, 3.0], opts).into_string();
    |

error[E0425]: cannot find function `metrics_panel` in module `views`
   --> boidboard/tests/web.rs:594:23
    |
594 |     let html = views::metrics_panel(&series).into_string();
    |                       ^^^^^^^^^^^^^ not found in `views`

Some errors have detailed explanations: E0425, E0433.
For more information about an error, try `rustc --explain E0425`.
error: could not compile `boidboard` (test "web") due to 10 previous errors
```

**GREEN** — `views::sparkline(values, SparkOpts)` with a **fixed** `viewBox`
(so a polling swap never resizes the page), and `views::metrics_panel(series)`
emitting one `figure.metric-card[data-metric]` per headline metric. The
`data-metric` value is the persisted `FrameMetrics` field name, not the display
label.

**REFACTOR** — the three degenerate cases were each a separate red on the first
run and are now handled at one place in `sparkline`: empty → baseline plus a
`sparkline-empty` marker and *no* polyline; one sample → a flat full-width
segment (a one-point polyline is invisible); zero range → pinned to the
vertical middle rather than evaluating `(v - min) / (max - min)` as `0/0`.
Non-finite samples are filtered out up front — one `NaN` in a `points`
attribute erases the entire shape, so a gap must never reach the markup.

---

### AC-47 — the reproducibility fingerprint is visible in the UI

**RED** — `ac47_provenance_panel_shows_every_field_needed_to_reproduce_a_run`,
`ac47_provenance_panel_marks_a_missing_final_state_hash_as_pending`

```
error[E0425]: cannot find function `provenance_panel` in module `views`
   --> boidboard/tests/web.rs:657:23
    |
657 |     let html = views::provenance_panel(&run).into_string();
    |                       ^^^^^^^^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `provenance_panel` in module `views`
   --> boidboard/tests/web.rs:686:23
    |
686 |     let html = views::provenance_panel(&run).into_string();
    |                       ^^^^^^^^^^^^^^^^ not found in `views`
```

**GREEN** — `views::provenance_panel(run)` renders `section.provenance` with
`.prov-seed`, `.prov-config-hash`, `.prov-kernel-version` and
`.prov-final-state-hash`, under a heading that names what the block is for.

**REFACTOR** — none needed. One decision worth recording: a run that has not
finished renders its final state hash as an explicit
`.prov-final-state-hash.prov-pending` rather than as an empty cell. A blank
there reads as "this run is not reproducible", which is a different and much
worse claim than "not finished yet"; the second test pins the distinction.

---

### AC-41 — run list shows status and headline metrics

**RED** — `ac41_status_badge_carries_the_status_as_data_and_text`,
`ac41_runs_table_shows_one_row_per_run_with_status_and_headline_metrics`,
`ac41_runs_table_says_so_when_there_are_no_runs`

```
error[E0425]: cannot find function `status_badge` in module `views`
    --> boidboard/tests/web.rs:710:27
     |
 710 |         let html = views::status_badge(status).into_string();
     |                           ^^^^^^^^^^^^
     |
    ::: autumn-web-0.6.0/src/widgets.rs:3081:1
     |
3081 | pub fn status_tag(label: &str) -> maud::Markup {
     | ---------------------------------------------- similarly named function `status_tag` defined here

error[E0422]: cannot find struct, variant or union type `RunSummary` in module `views`
   --> boidboard/tests/web.rs:731:16
    |
731 |         views::RunSummary {
    |                ^^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `runs_table` in module `views`
   --> boidboard/tests/web.rs:737:23
    |
737 |     let html = views::runs_table(&summaries).into_string();
    |                       ^^^^^^^^^^ not found in `views`
```

**GREEN** — `views::status_badge`, `views::RunSummary { run, scenario_name,
headline }`, `views::run_row` and `views::runs_table`. `RunSummary` is the seam
that keeps AC-50 true: assembling it needs a database, rendering it does not.

**REFACTOR** — the badge's visible text is humanised (`budget_exceeded` →
`budget exceeded`) while `data-status` keeps the raw value, so rewording a
label can never break a selector or a stylesheet rule. The empty list renders
the table with one `td.empty` row rather than nothing at all — "no runs yet"
and "the page failed to load" must not look the same.

---

### AC-46 — compare view renders both runs and highlights differing config fields

**RED** — `ac46_config_diff_lists_only_the_fields_that_actually_differ`,
`ac46_config_diff_reports_fields_present_on_only_one_side`,
`ac46_compare_view_renders_both_runs_and_highlights_the_differences`,
`ac46_compare_view_says_so_when_two_runs_share_a_config`

```
error[E0425]: cannot find function `config_diff` in module `views`
   --> boidboard/tests/web.rs:795:23
    |
795 |     let diff = views::config_diff(&a, &b);
    |                       ^^^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `compare_view` in module `views`
   --> boidboard/tests/web.rs:843:23
    |
843 |     let html = views::compare_view((&a, &agents_a), (&b, &agents_b), &world).into_string();
    |                       ^^^^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `compare_view` in module `views`
   --> boidboard/tests/web.rs:880:23
    |
880 |     let html = views::compare_view((&a, &[]), (&b, &[]), &world).into_string();
    |                       ^^^^^^^^^^^^ not found in `views`
```

**GREEN** — `views::config_diff(a, b)` flattens both `config_snapshot`s to
dotted leaf paths and returns only the leaves that differ;
`views::compare_view` draws both flocks **into the same world at the same
scale** (so the pictures are directly comparable, which is the whole point)
with each side's provenance panel, above a `table.config-diff` of
`tr.config-diff-row[data-field]`.

**REFACTOR** — arrays are compared whole rather than element-wise. Reporting
`obstacles.2.radius` as three separate differences would bury the finding; an
obstacle list is one design decision. Two runs that share a config get an
explicit `.config-diff-empty` statement — "same config, so the difference is
seed or kernel" is a finding, not an empty table.

---

### AC-45 (view half) — the progress fragment polls itself, and stops when terminal

**RED** — `ac45_progress_fragment_polls_itself_while_a_run_is_not_terminal`,
`ac45_progress_fragment_stops_polling_once_the_run_is_terminal`,
`ac45_progress_fragment_shows_the_error_of_a_failed_run`

```
error[E0425]: cannot find function `progress_fragment` in module `views`
   --> boidboard/tests/web.rs:896:27
    |
896 |         let html = views::progress_fragment(&run).into_string();
    |                           ^^^^^^^^^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `progress_fragment` in module `views`
   --> boidboard/tests/web.rs:929:27
    |
929 |         let html = views::progress_fragment(&run).into_string();
    |                           ^^^^^^^^^^^^^^^^^ not found in `views`
```

**GREEN** — `views::progress_fragment(run)` renders
`div#run-progress.run-progress[data-status]` carrying `hx-get`,
`hx-trigger="every 2s"` and `hx-swap="outerHTML"` **only while the run is
non-terminal**, using `models::run::status::is_terminal`.

**REFACTOR** — none needed. The design decision worth recording: the fragment
carries its own polling attributes rather than the page carrying them. The last
swap a run ever receives is therefore the one that *removes* those attributes,
so a finished run costs zero further requests in every open tab, forever — with
no client-side code to get that right.

---

### AC-42 (view half) — the form's primary control is a preset card

**RED** — `ac42_new_run_form_is_fronted_by_preset_cards`

```
error[E0425]: cannot find function `new_run_form` in module `views`
   --> boidboard/tests/web.rs:959:23
    |
959 |     let html = views::new_run_form(&all).into_string();
    |                       ^^^^^^^^^^^^ not found in `views`

For more information about this error, try `rustc --explain E0425`.
error: could not compile `boidboard` (test "web") due to 4 previous errors
```

**GREEN** — `views::new_run_form(presets)` emits `form[method=post][action=/runs]`
with one `li.preset-card[data-slug]` per preset — name, description and the
parameters that give it its character — exactly one radio preselected, and
optional `seed` / `max_ticks` overrides.

**REFACTOR** — none needed.

---

### AC-43 / AC-44 / AC-47 (composition) — the assembled detail page

**RED** — `ac43_run_detail_page_assembles_flock_trajectory_metrics_and_provenance`,
`ac43_run_detail_page_renders_a_run_that_has_no_frames_yet`

```
error[E0425]: cannot find type `RunDetail` in module `views`
    --> boidboard/tests/web.rs:1009:43
     |
1009 | fn detail_fixture(status: &str) -> views::RunDetail {
     |                                           ^^^^^^^^^ not found in `views`

error[E0425]: cannot find function `run_detail_page` in module `views`
    --> boidboard/tests/web.rs:1044:23
     |
1044 |     let html = views::run_detail_page(&detail_fixture(run_status::RUNNING)).into_string();
     |                       ^^^^^^^^^^^^^^^ not found in `views`
```

**GREEN** — `views::RunDetail` (the whole of what the page draws) and
`views::run_detail_page(&RunDetail)`, composing the progress fragment, flock
SVG, trajectory SVG, metrics panel and provenance panel.

**REFACTOR** — none needed. `RunDetail` is the AC-50 seam for the detail page
just as `RunSummary` is for the list: the handler fills it in, the page has
nothing left to fetch.

---

## The route layer

All six handlers were driven out by one red. `routes![…]` cannot even name a
handler that does not exist, so the whole route surface fails to compile until
every one of them is there:

**RED** — `ac41_run_list_route_renders_a_row_per_run_with_status_and_metrics`,
`ac41_run_list_route_renders_an_empty_state_rather_than_a_blank_page`,
`ac42_new_run_route_is_fronted_by_preset_cards`,
`ac42_posting_a_preset_slug_creates_a_run_carrying_that_presets_config`,
`ac42_posting_a_preset_slug_alone_uses_the_offered_defaults`,
`ac42_posting_an_unknown_preset_is_rejected_rather_than_silently_substituted`,
`ac43_run_detail_route_renders_inline_svg_with_oriented_marks_and_ribbons`,
`ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page`,
`ac44_run_detail_route_renders_one_sparkline_per_headline_metric`,
`ac44_run_detail_route_survives_a_run_with_no_frames_at_all`,
`ac45_progress_endpoint_returns_the_fragment_and_nothing_else`,
`ac45_the_page_polls_while_running_and_stops_once_terminal`,
`ac46_compare_route_renders_both_runs_and_the_config_difference`,
`ac46_compare_route_404s_when_a_run_is_missing`,
`ac47_run_detail_route_surfaces_the_reproducibility_hash`,
`run_detail_route_404s_for_a_run_that_does_not_exist`

```
error[E0425]: cannot find value `run_list` in module `app_routes`
    --> boidboard/tests/web.rs:1110:21
     |
1110 |         app_routes::run_list,
     |                     ^^^^^^^^ not found in `app_routes`

error[E0425]: cannot find function `__autumn_route_info_run_list` in module `app_routes`
    --> boidboard/tests/web.rs:1110:21
     |
1110 |         app_routes::run_list,
     |                     ^^^^^^^^ not found in `app_routes`

error[E0425]: cannot find value `new_run_form` in module `app_routes`
    --> boidboard/tests/web.rs:1111:21
     |
1111 |         app_routes::new_run_form,
     |                     ^^^^^^^^^^^^ not found in `app_routes`
     |
help: consider importing one of these functions
     |
  12 + use boidboard::views::new_run_form;
     |

error[E0425]: cannot find value `create_run` in module `app_routes`
    --> boidboard/tests/web.rs:1112:21
     |
```

**GREEN** — the six handlers in `routes.rs`, all `#[public]`, each doing only
fetch-and-delegate. Route tests run against the live Postgres through
`TestApp::new().routes(all_routes()).with_transactional_db(…)`, so every test
rolls back on drop and the suite is order-independent.

### A second, more interesting red inside the same cycle

The first full route run came back **48 tests, 1 failure** — and the failure was
a *test* bug, which is exactly the kind the seam assertion exists to catch:

```
running 48 tests
...
test ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page ... FAILED

---- ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page stdout ----
thread 'ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page'
panicked at boidboard/tests/web.rs:1443:10:
expected 2 element(s) matching selector `g.trajectory[data-agent-id="0"] polyline.trail`, found 1.
Parsed HTML:
<html lang="en">
  ...
          <svg class="flock" role="img" aria-label="Flock of 2 agents in a 400 by 300 toroidal world" …
```

The fixture walked an agent from `x = 195` to `x = 0`, which *is* a seam
crossing in a 200-wide world — but the `classic-flock` preset's world is
**400 × 300**, and a 195-unit step in a 400-wide world is an ordinary step, not
a wrap. The renderer was right and the fixture was wrong. Fixed by wrapping the
agent at the real seam (`380, 385, 390, 395, 0, 5`), after which agent 0 gets
two ribbons and agent 1, which never leaves the world, gets one.

**REFACTOR** —

1. Views hard-coded `format!("/runs/{}", id)` while the tests were being
   written. Once the handlers existed, `autumn_web::paths![…]` in `routes.rs`
   gave typed helpers, and every URL in `views.rs` now goes through
   `crate::routes::paths::…`, so a route path and the links to it cannot drift
   apart. Tests stayed green across the swap, which is what made it safe.

---

### AC-46 (follow-up) — reaching the compare view from a run

**RED** — `ac46_detail_page_offers_a_compare_form_prefilled_with_this_run`

```
running 1 test
test ac46_detail_page_offers_a_compare_form_prefilled_with_this_run ... FAILED

---- ac46_detail_page_offers_a_compare_form_prefilled_with_this_run stdout ----

thread 'ac46_detail_page_offers_a_compare_form_prefilled_with_this_run' (1799) panicked at boidboard/tests/web.rs:1692:10:
a compare form on the detail page
```

**GREEN** — the detail page's compare call-to-action became a real
`form.compare-cta[method=get][action=/compare]` with the current run prefilled
as the hidden `a` and a number field for `b`.

**REFACTOR** — none needed. It replaced a placeholder link that compared a run
with *itself*, which was never a useful destination.

---

### AC-48 — route tests assert HTML structure, not strings

Not a cycle of its own: it is a property of every route test above. All of them
go through `TestApp` and assert with `assert_selector`,
`assert_selector_count`, `assert_no_selector`, `assert_attr`, `assert_text` and
`selector_attr` — for example
`assert_selector_count(r#"g.trajectory[data-agent-id="0"] polyline.trail"#, 2)`
and `assert_selector(r#"#run-progress[hx-trigger="every 2s"]"#)`. The only
non-selector assertions in the route layer are on the `Location` header, on
HTTP status, and on rows read back out of the database after a `POST` — none
of which is HTML.

---

### AC-50 — no domain logic in handlers; views are pure

AC-50 has no separate red phase because **every `views::` red in this document
is one**. Those tests are plain `#[test]`s — no `#[tokio::test]`, no `TestApp`,
no pool — so a view that reached for a connection could not have appeared in
that layer at all; the criterion is enforced by where the tests live.

What was added at the end is the guard that stops the property eroding from
either side: `ac50_handlers_do_no_rendering_and_views_do_no_io` reads both
source files with `include_str!` and asserts that `routes.rs` contains no
`html!`, no `<svg`, no `viewBox` and no kernel call, and that `views.rs`
contains no `.await`, no `diesel`, no `AsyncPgConnection`, no repository and no
`AutumnResult`. It then renders every public view from plain structs. It passed
the moment it was written — which is the point of adding it during refactor
rather than pretending otherwise.

---

## Rendering and query budgets (for the reviewer)

| Budget | Value | Where | Why |
|---|---|---|---|
| Run list size | 200 runs | `routes::RUN_LIST_LIMIT` | The list is a launcher, not an archive. |
| Detail frames fetched | 120 frames, evenly spaced, endpoints pinned | `routes::DETAIL_FRAME_BUDGET` → `frames_for_run` | The latency budget: a 10 000-tick, 160-agent run would otherwise deserialize 1.6 M agent records per page view. |
| Flock marks drawn | 400 agents | `views::FlockOpts::max_agents` | Page weight, not legibility — a 5 000-agent frame is megabytes of `points` attributes. |
| Ribbons drawn | 30 agents × 60 frames | `views::TrajectoryOpts` | Thirty paths is already past what a reader can follow. |
| Sparkline geometry | fixed `0 0 100 24` viewBox | `views::SparkOpts` | A stable viewBox means an htmx swap never resizes the page. |

Every subsample goes through `views::even_indices`, which pins the first and
last item — so a trajectory always spans the whole run and the *current* state
is never the frame that gets dropped.

Two deliberate degradations, both preferring a usable page to a 500:

* A `config_snapshot` that does not parse as `SimParams` (an older kernel, or a
  hand-written config) falls back to a default 200 × 200 world with no
  obstacles. The provenance panel is exactly what a user needs to *see* while
  diagnosing such a run, so the page must still render.
* A frame whose `agents` or `metrics` JSON will not decode is skipped rather
  than failing the request.

---

## Route surface

Six handlers, all `#[public]`, all in `boidboard::routes`:

| Handler | Method + path |
|---|---|
| `run_list` | `GET /runs` |
| `new_run_form` | `GET /runs/new` |
| `create_run` | `POST /runs` |
| `run_detail` | `GET /runs/{id}` |
| `run_progress` | `GET /runs/{id}/progress` |
| `compare` | `GET /compare?a=…&b=…` |

Wire them in with:

```rust
.routes(routes![
    index,
    routes::run_list,
    routes::new_run_form,
    routes::create_run,
    routes::run_detail,
    routes::run_progress,
    routes::compare,
])
```

`routes.rs` also declares `autumn_web::paths![…]`, so `views.rs` links through
`crate::routes::paths::run_detail(id)` and friends rather than hand-built URL
strings.

## Which test proves which criterion

| AC | Proven by |
|---|---|
| **AC-41** | `ac41_run_list_route_renders_a_row_per_run_with_status_and_metrics` (+ `ac41_runs_table_shows_one_row_per_run_with_status_and_headline_metrics`, `ac41_status_badge_carries_the_status_as_data_and_text`, `ac41_run_list_route_renders_an_empty_state_rather_than_a_blank_page`) |
| **AC-42** | `ac42_new_run_route_is_fronted_by_preset_cards` and `ac42_posting_a_preset_slug_creates_a_run_carrying_that_presets_config` (+ `ac42_presets_offer_at_least_four_distinct_named_starting_points`, `ac42_presets_have_genuinely_different_character`, `ac42_by_slug_round_trips_and_rejects_unknown_slugs`, `ac42_every_preset_is_a_valid_simulation_config`, `ac42_presets_serialize_to_json_for_the_config_snapshot`, `ac42_new_run_form_is_fronted_by_preset_cards`, `ac42_posting_a_preset_slug_alone_uses_the_offered_defaults`, `ac42_posting_an_unknown_preset_is_rejected_rather_than_silently_substituted`) |
| **AC-43** | `ac43_run_detail_route_renders_inline_svg_with_oriented_marks_and_ribbons` and `ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page` (+ `ac43_every_agent_is_an_oriented_mark_rotated_to_its_heading`, `ac43_flock_svg_is_inline_svg_with_a_world_sized_viewbox`, `ac43_flock_svg_renders_obstacles_and_survives_degenerate_input`, `ac43_flock_svg_respects_its_agent_budget`, `ac43_trajectory_svg_draws_one_ribbon_group_per_agent`, `ac43_trajectory_ribbons_break_at_the_toroidal_seam`, `ac43_trajectory_svg_enforces_its_rendering_budget`, `ac43_trajectory_svg_handles_empty_and_single_frame_input`, `ac43_run_detail_page_assembles_flock_trajectory_metrics_and_provenance`, `ac43_run_detail_page_renders_a_run_that_has_no_frames_yet`) |
| **AC-44** | `ac44_run_detail_route_renders_one_sparkline_per_headline_metric` (+ `ac44_sparkline_maps_the_series_across_a_stable_viewbox`, `ac44_sparkline_survives_empty_single_and_flat_series`, `ac44_metrics_panel_gives_every_headline_metric_its_own_sparkline`, `ac44_metrics_panel_renders_for_a_run_with_no_frames_yet`, `ac44_run_detail_route_survives_a_run_with_no_frames_at_all`) |
| **AC-45** | `ac45_progress_endpoint_returns_the_fragment_and_nothing_else` (endpoint asserted **directly**) and `ac45_the_page_polls_while_running_and_stops_once_terminal` (+ `ac45_progress_fragment_polls_itself_while_a_run_is_not_terminal`, `ac45_progress_fragment_stops_polling_once_the_run_is_terminal`, `ac45_progress_fragment_shows_the_error_of_a_failed_run`) |
| **AC-46** | `ac46_compare_route_renders_both_runs_and_the_config_difference` (+ `ac46_config_diff_lists_only_the_fields_that_actually_differ`, `ac46_config_diff_reports_fields_present_on_only_one_side`, `ac46_compare_view_renders_both_runs_and_highlights_the_differences`, `ac46_compare_view_says_so_when_two_runs_share_a_config`, `ac46_compare_route_404s_when_a_run_is_missing`, `ac46_detail_page_offers_a_compare_form_prefilled_with_this_run`) |
| **AC-47** | `ac47_run_detail_route_surfaces_the_reproducibility_hash` (+ `ac47_provenance_panel_shows_every_field_needed_to_reproduce_a_run`, `ac47_provenance_panel_marks_a_missing_final_state_hash_as_pending`) |
| **AC-48** | Every route test above. `boidboard/tests/web.rs` contains **72** selector-family assertions and **zero** `assert_body_contains` / `assert_body_eq`. |
| **AC-50** | `ac50_handlers_do_no_rendering_and_views_do_no_io`, plus the fact that all 26 view tests are plain `#[test]`s with no `TestApp`, no runtime and no pool. |

## Final state

`cargo clippy -p boidboard --all-targets -- -D warnings` — clean.

`cargo test -p boidboard`, twice consecutively:

```
===== RUN 1 =====
     Running unittests src/lib.rs
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running unittests src/main.rs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/persistence.rs
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
     Running tests/web.rs
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
     Running tests/workflow.rs
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.35s
   Doc-tests boidboard
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s

===== RUN 2 =====
     Running unittests src/lib.rs
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running unittests src/main.rs
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/persistence.rs
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.35s
     Running tests/web.rs
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
     Running tests/workflow.rs
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.37s
   Doc-tests boidboard
test result: ok. 0 passed; 0 failed; 24 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

> Both full-suite runs above were captured before a concurrent agent's
> in-progress AC-33 cancellation work landed in `tests/workflow.rs`. That
> binary is owned by another agent and is not part of this deliverable; the web
> layer's own binary was re-run twice consecutively afterwards and is green
> both times:
>
> ```
> == WEB RUN 1 ==
> test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
> == WEB RUN 2 ==
> test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
> == LIB + PERSISTENCE ==
> test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
> test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
> ```

Passing twice consecutively against a live Postgres is the isolation proof: the
route tests write scenarios, runs and frames on every run and every one of
those writes is rolled back when the `TestClient` drops, so the second run sees
exactly the database the first one did.
