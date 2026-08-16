//! Pages, panels, tables, forms and fragments.
//!
//! The half of `views` that has *structure* in it rather than geometry: what a
//! page is made of, in what order, and which parts of it are shown to whom.
//! Where [`super::svg`] decides where a mark goes, this decides whether a cancel
//! button exists at all.
//!
//! Every function here is pure `fn(data) -> Markup`; see the module docs on
//! [`crate::views`] for what that rules out.

use autumn_web::prelude::*;
use boids_core::metrics::FrameMetrics;
use boids_core::{Agent, Obstacle, World};

use super::style::STYLESHEET;
use super::svg::{FlockOpts, SparkOpts, TrajectoryOpts, flock_svg, num, sparkline, trajectory_svg};

/// Format a metric for display, with a fixed number of decimals.
fn metric(v: f64, places: usize) -> String {
    if v.is_finite() {
        format!("{v:.places$}")
    } else {
        "—".to_owned()
    }
}

// ───────────────────────────────── layout ─────────────────────────────────

/// The page chrome: doctype, head, styles, htmx, and the content slot.
///
/// The stylesheet is embedded rather than linked, and htmx is loaded from the
/// path Autumn serves it on. There is no external host anywhere in the
/// document — AC-43's "no SPA framework, no WASM" is enforced by there being
/// nothing to load.
#[must_use]
pub fn layout(title: &str, content: Markup) -> Markup {
    html! {
        (PreEscaped("<!DOCTYPE html>"))
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Boidboard" }
                style { (PreEscaped(STYLESHEET)) }
                script src=(autumn_web::htmx::HTMX_JS_PATH) defer {}
            }
            body {
                header class="masthead" {
                    a class="brand" href=(crate::routes::paths::run_list()) { "Boidboard" }
                    nav class="masthead-nav" {
                        a href=(crate::routes::paths::run_list()) { "Runs" }
                        a href=(crate::routes::paths::new_run_form()) { "New run" }
                    }
                }
                main class="page" { (content) }
            }
        }
    }
}

// ────────────────────────── status, provenance, rows ──────────────────────────

/// A run's lifecycle state as a selectable, styleable badge.
///
/// The raw status is kept verbatim in `data-status` — that is what CSS and
/// tests key on — while the visible text is humanised. Rewording the label can
/// then never break a selector.
#[must_use]
pub fn status_badge(status: &str) -> Markup {
    html! {
        span class="status-badge" data-status=(status) { (status.replace('_', " ")) }
    }
}

/// The reproducibility fingerprint of a run (AC-47).
///
/// Seed, config hash, kernel version and final state hash are the four values
/// that together answer "can I get this exact run back?". They are surfaced
/// as one labelled block rather than scattered through the page, because their
/// value is in being checked *together*: same seed and config against a
/// different kernel version is a different run, and a matching final state hash
/// is the proof that a re-run actually reproduced.
///
/// A run that has not finished has no final state hash yet. That is shown as
/// an explicit pending state, never as an empty cell — a blank reads as "this
/// run is not reproducible", which is a different and much worse claim.
#[must_use]
pub fn provenance_panel(run: &crate::models::Run) -> Markup {
    html! {
        section class="provenance panel" {
            h2 { "Reproducibility fingerprint" }
            p class="provenance-note" {
                "These four values are what it takes to reproduce this run: same seed, \
                 same config, same kernel — and a matching final state hash is the proof \
                 that the reproduction really matched."
            }
            dl {
                dt { "Seed" }
                dd class="prov-seed" { (run.seed) }
                dt { "Config hash" }
                dd class="prov-config-hash" { (run.config_hash) }
                dt { "Kernel version" }
                dd class="prov-kernel-version" { (run.kernel_version) }
                dt { "Final state hash" }
                @match run.final_state_hash.as_deref() {
                    Some(hash) => dd class="prov-final-state-hash" { (hash) },
                    None => dd class="prov-final-state-hash prov-pending" {
                        "pending — set when the run reaches a terminal state"
                    },
                }
            }
        }
    }
}

/// One run as the run list sees it: the row plus the two things the row shows
/// that are not on the `Run` itself.
///
/// Assembling this is the *handler's* job — it is the only part that needs a
/// database. Rendering it is this module's, and takes no IO at all.
#[derive(Debug, Clone)]
pub struct RunSummary {
    /// The run itself.
    pub run: crate::models::Run,
    /// Name of the scenario the run was created from.
    pub scenario_name: String,
    /// Metrics of the run's most recent stored frame, if it has one.
    pub headline: Option<FrameMetrics>,
}

/// One row of the run list (AC-41).
#[must_use]
pub fn run_row(summary: &RunSummary) -> Markup {
    let run = &summary.run;
    let m = summary.headline.as_ref();
    html! {
        tr class="run-row" data-run-id=(run.id) {
            td class="run-name" {
                a class="run-link" href=(crate::routes::paths::run_detail(run.id)) { (summary.scenario_name) }
            }
            td class="run-id" { "#" (run.id) }
            td class="run-status" { (status_badge(&run.status)) }
            td class="run-ticks" { (run.ticks_completed) " / " (run.max_ticks) }
            td class="metric metric-polarization" {
                (m.map_or_else(|| "—".to_owned(), |m| metric(m.polarization, 3)))
            }
            td class="metric metric-nnd" {
                (m.map_or_else(|| "—".to_owned(), |m| metric(m.mean_nearest_neighbor_distance, 2)))
            }
            td class="metric metric-collisions" {
                (m.map_or_else(|| "—".to_owned(), |m| m.collisions.to_string()))
            }
            td class="metric metric-speed" {
                (m.map_or_else(|| "—".to_owned(), |m| metric(m.mean_speed, 2)))
            }
            td class="run-seed" { (run.seed) }
        }
    }
}

/// The run list (AC-41): every run with its status and headline metrics.
///
/// An empty list renders as the table plus one explanatory row, not as a blank
/// page — "no runs yet" and "the page failed to load" must not look the same.
#[must_use]
pub fn runs_table(summaries: &[RunSummary]) -> Markup {
    html! {
        table class="runs" {
            thead {
                tr {
                    th { "Scenario" }
                    th { "Run" }
                    th { "Status" }
                    th { "Ticks" }
                    th { "Polarization" }
                    th { "Mean NND" }
                    th { "Collisions" }
                    th { "Mean speed" }
                    th { "Seed" }
                }
            }
            tbody {
                @if summaries.is_empty() {
                    tr class="empty-row" {
                        td class="empty" colspan="9" {
                            "No runs yet — start one from a preset."
                        }
                    }
                } @else {
                    @for s in summaries { (run_row(s)) }
                }
            }
        }
    }
}

/// The four headline metrics of a run, each as a labelled sparkline (AC-44).
///
/// The `data-metric` attribute carries the **persisted** `FrameMetrics` field
/// name, not the display label: the label is free to be reworded, the field
/// name is a storage contract and is what a test or a debugging session wants
/// to select on.
#[must_use]
pub fn metrics_panel(series: &[FrameMetrics]) -> Markup {
    let cards = [
        MetricCard {
            field: "polarization",
            label: "Polarization",
            values: series.iter().map(|m| m.polarization).collect(),
            places: 3,
        },
        MetricCard {
            field: "mean_nearest_neighbor_distance",
            label: "Mean nearest-neighbour distance",
            values: series
                .iter()
                .map(|m| m.mean_nearest_neighbor_distance)
                .collect(),
            places: 2,
        },
        MetricCard {
            field: "collisions",
            label: "Collisions",
            values: series.iter().map(|m| m.collisions as f64).collect(),
            places: 0,
        },
        MetricCard {
            field: "mean_speed",
            label: "Mean speed",
            values: series.iter().map(|m| m.mean_speed).collect(),
            places: 2,
        },
    ];

    html! {
        section class="metrics" {
            @for card in &cards {
                figure class="metric-card" data-metric=(card.field) {
                    figcaption { (card.label) }
                    (sparkline(&card.values, SparkOpts::default()))
                    span class="metric-latest" {
                        @match card.values.last() {
                            Some(v) => (metric(*v, card.places)),
                            None => "—",
                        }
                    }
                }
            }
        }
    }
}

/// One headline metric, ready to draw.
///
/// A named struct rather than a four-element tuple because three of the four
/// members are easy to confuse: `field` and `label` are both strings and are
/// deliberately *different* strings, and `places` is a bare `usize` sitting next
/// to a `Vec<f64>`. At the call site `places: 0` reads as "collisions are whole
/// numbers"; `0` in the fourth tuple slot reads as nothing at all.
struct MetricCard {
    /// The persisted [`FrameMetrics`] field name, emitted as `data-metric`.
    /// A storage contract, and what a test or a debugging session selects on.
    field: &'static str,
    /// The human-facing caption, free to be reworded without breaking a
    /// selector.
    label: &'static str,
    /// The series, oldest first.
    values: Vec<f64>,
    /// Decimal places for the latest-value readout.
    places: usize,
}

// ────────────────────────────── compare (AC-46) ──────────────────────────────

/// One config field whose value differs between two runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiff {
    /// Dotted path to the field, e.g. `world.height`.
    pub field: String,
    /// Rendered value on the left-hand run, or `—` when absent.
    pub a: String,
    /// Rendered value on the right-hand run, or `—` when absent.
    pub b: String,
}

/// Absent-on-this-side marker. A visible dash, not an empty string: an empty
/// cell reads as "the value is blank".
const ABSENT: &str = "—";

/// The fields in which two config snapshots differ, addressed by dotted path
/// and ordered stably.
///
/// Flattening to leaves is what makes the answer useful: "the configs differ"
/// is not a finding, "`w_cohesion` went from 1.0 to 0.02" is. Arrays are
/// compared whole rather than element-wise — an obstacle list is a single
/// design decision, and reporting `obstacles.2.radius` would be noise.
#[must_use]
pub fn config_diff(a: &serde_json::Value, b: &serde_json::Value) -> Vec<ConfigDiff> {
    let mut left = Vec::new();
    flatten(a, "", &mut left);
    let mut right = Vec::new();
    flatten(b, "", &mut right);

    let mut fields: Vec<String> = left
        .iter()
        .chain(right.iter())
        .map(|(k, _)| k.clone())
        .collect();
    fields.sort();
    fields.dedup();

    fields
        .into_iter()
        .filter_map(|field| {
            let lv = left
                .iter()
                .find(|(k, _)| *k == field)
                .map(|(_, v)| v.clone());
            let rv = right
                .iter()
                .find(|(k, _)| *k == field)
                .map(|(_, v)| v.clone());
            if lv == rv {
                return None;
            }
            Some(ConfigDiff {
                field,
                a: lv.unwrap_or_else(|| ABSENT.to_owned()),
                b: rv.unwrap_or_else(|| ABSENT.to_owned()),
            })
        })
        .collect()
}

/// Flatten a JSON value to `(dotted path, rendered leaf)` pairs.
fn flatten(value: &serde_json::Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(v, &path, out);
            }
        }
        serde_json::Value::String(s) => out.push((prefix.to_owned(), s.clone())),
        other => out.push((prefix.to_owned(), other.to_string())),
    }
}

/// Two runs side by side, with the config fields that differ called out
/// (AC-46).
///
/// Both flocks are drawn into the **same world**, at the same scale, so the
/// pictures are directly comparable — which is the entire point of the view.
/// The diff table below them answers "why do these look different?" without
/// making the reader compare two blobs of JSON by eye.
#[must_use]
pub fn compare_view(
    a: (&crate::models::Run, &[Agent]),
    b: (&crate::models::Run, &[Agent]),
    world: &World,
) -> Markup {
    let (run_a, agents_a) = a;
    let (run_b, agents_b) = b;
    let diffs = config_diff(&run_a.config_snapshot, &run_b.config_snapshot);

    html! {
        section class="compare" {
            div class="compare-sides" {
                (compare_side(run_a, agents_a, world))
                (compare_side(run_b, agents_b, world))
            }
            section class="panel" {
                h2 { "Config differences" }
                @if diffs.is_empty() {
                    p class="config-diff-empty" {
                        "These two runs used byte-identical configs — any difference \
                         between them comes from the seed or the kernel version, not \
                         from a parameter."
                    }
                } @else {
                    table class="config-diff" {
                        thead {
                            tr {
                                th { "Field" }
                                th { "Run #" (run_a.id) }
                                th { "Run #" (run_b.id) }
                            }
                        }
                        tbody {
                            @for d in &diffs {
                                tr class="config-diff-row" data-field=(d.field) {
                                    th scope="row" { (d.field) }
                                    td class="diff-a" { (d.a) }
                                    td class="diff-b" { (d.b) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One half of the compare view.
fn compare_side(run: &crate::models::Run, agents: &[Agent], world: &World) -> Markup {
    html! {
        section class="compare-side panel" data-run-id=(run.id) {
            h2 {
                a href=(crate::routes::paths::run_detail(run.id)) { "Run #" (run.id) }
                " " (status_badge(&run.status))
            }
            (flock_svg(agents, world, &[], FlockOpts::default()))
            (provenance_panel(run))
        }
    }
}

// ─────────────────────── progress fragment (AC-45) ───────────────────────

/// The htmx-polled progress fragment for one run (AC-45).
///
/// The fragment **is** the polling unit: it carries its own `hx-get` pointing
/// back at the endpoint that renders exactly this markup, and `hx-swap`
/// replaces itself, so the page needs no JavaScript of its own and the
/// endpoint needs no knowledge of where on the page it lands.
///
/// A terminal run emits **no** `hx-get` and **no** `hx-trigger`. That is the
/// whole stopping condition: the last swap a run ever receives is the one that
/// removes the polling attributes, so a finished run costs zero further
/// requests — for every open tab, forever.
#[must_use]
pub fn progress_fragment(run: &crate::models::Run) -> Markup {
    let live = !crate::models::run::status::is_terminal(&run.status);
    let poll_url = live.then(|| crate::routes::paths::run_progress(run.id));
    html! {
        div class="run-progress"
            id="run-progress"
            data-status=(run.status)
            hx-get=[poll_url]
            hx-trigger=[live.then_some("every 2s")]
            hx-swap=[live.then_some("outerHTML")]
        {
            (status_badge(&run.status))
            progress class="progress-bar"
                     value=(run.ticks_completed) max=(run.max_ticks.max(1)) {}
            span class="progress-ticks" {
                (run.ticks_completed) " / " (run.max_ticks) " ticks"
            }
            @if let Some(err) = run.error.as_deref() {
                p class="run-error" { (err) }
            }
        }
    }
}

// ───────────────────────────── CSRF for forms ─────────────────────────────

/// The CSRF token a state-changing form must carry, as a view sees it.
///
/// # Why the view is handed a token rather than fetching one
///
/// Autumn's `CsrfLayer` mints a per-request token and publishes it — plus the
/// **configured field name**, which `security.csrf.form_field` can rename — in
/// request extensions. Reaching into a request is precisely what a view may not
/// do (AC-50), so the handler extracts both and passes this across. That keeps
/// the view a pure function of data while still emitting the *framework's* field
/// name and the *framework's* token, never a value this crate invented.
///
/// The markup produced is the same hidden input `autumn_web::form::form_tag`
/// emits — this exists only because these forms need their own semantic classes,
/// which `form_tag` does not take.
///
/// # Absent is a first-class state
///
/// With no CSRF layer mounted there is no token, and [`absent`](Self::absent)
/// emits **nothing**. An invented or empty token would be worse than none: the
/// form would look protected, and would be rejected the moment the layer was
/// switched on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Csrf {
    field: String,
    token: Option<String>,
}

impl Csrf {
    /// The form-field name autumn-web uses unless `security.csrf.form_field`
    /// says otherwise.
    ///
    /// Mirrors the default in `autumn_web::form` (`ChangesetForm::blank`,
    /// `form_tag`). It is only a *fallback*: when the middleware is mounted it
    /// publishes the configured name, and [`from_request_parts`](Self::from_request_parts)
    /// prefers that.
    pub const DEFAULT_FIELD: &'static str = "_csrf";

    /// A token to emit under `field`.
    #[must_use]
    pub fn new(field: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            token: Some(token.into()),
        }
    }

    /// No token — the CSRF layer is not mounted, so there is nothing to emit.
    #[must_use]
    pub fn absent() -> Self {
        Self::default()
    }

    /// Build from what `CsrfLayer` put in the request.
    ///
    /// Both arguments are optional because both are absent when the layer is
    /// not mounted — which is the case in the pure-route tests, and on the `dev`
    /// profile before CSRF is switched on. A token with no accompanying field
    /// name falls back to [`DEFAULT_FIELD`](Self::DEFAULT_FIELD), which is the
    /// same fallback `autumn_web::form::form_tag` makes.
    #[must_use]
    pub fn from_request_parts(
        token: Option<&autumn_web::security::CsrfToken>,
        field: Option<&autumn_web::security::CsrfFormField>,
    ) -> Self {
        Self {
            field: field.map_or_else(|| Self::DEFAULT_FIELD.to_owned(), |field| field.0.clone()),
            token: token.map(|token| token.token().to_owned()),
        }
    }

    /// The hidden input, or nothing at all when there is no token.
    #[must_use]
    pub fn hidden_input(&self) -> Markup {
        html! {
            @if let Some(token) = self.token.as_deref() {
                input type="hidden" name=(self.field) value=(token);
            }
        }
    }
}

// ─────────────────────── stuck detection (AC-27) ───────────────────────

/// The "this flock has stopped getting anywhere" badge (**AC-27**).
///
/// Deliberately **not** a status: `runs.status` records what the *workflow* did,
/// and a stuck run is still perfectly healthy from the engine's point of view —
/// it is producing frames, advancing its cursor and heading for `completed`.
/// Stuckness is a reading of the *simulation*, so it is surfaced beside the
/// status rather than inside it, and it says what it measured so the reader can
/// disagree with it.
#[must_use]
pub fn stuck_badge() -> Markup {
    html! {
        p class="stuck-badge" role="status" {
            strong { "Stuck" }
            " — the flock's centroid has covered ground without getting anywhere \
              over the last "
            (crate::analysis::STUCK_WINDOW)
            " stored frames."
        }
    }
}

// ─────────────────────── cancelling a run (AC-33) ───────────────────────

/// The cancel control for one run (**AC-33**).
///
/// Rendered **only while the run is non-terminal**, because that is exactly the
/// set of runs `POST /runs/{id}/cancel` will act on: the handler redirects an
/// already-terminal run unchanged, so a cancel button on a finished run would
/// be a control that promises something and then quietly does nothing.
///
/// A `<form method="post">` and not a link: cancelling changes state, and a
/// `GET` control is one a crawler, a prefetcher or a browser's link preview can
/// fire on its own.
///
/// The button carries the run id in its own copy, so the page reads as a
/// sentence rather than as an unlabelled control — a detail page can be open in
/// several tabs at once.
#[must_use]
pub fn cancel_form(run_id: i64, csrf: &Csrf) -> Markup {
    html! {
        form class="cancel-run" method="post" action=(crate::routes::paths::cancel_run(run_id)) {
            (csrf.hidden_input())
            button class="cancel-run-button" type="submit" {
                "Cancel run #" (run_id)
            }
            p class="cancel-run-note" {
                "The batch already in flight finishes and keeps its frames; the run \
                 stops at the next batch boundary."
            }
        }
    }
}

// ─────────────────────── the new-run form (AC-42) ───────────────────────

/// The new-run form, fronted by preset cards (AC-42).
///
/// The parameter space is fifteen coupled floats, most combinations of which
/// produce a static cloud or an explosion. Offering it blank is offering
/// nothing, so the form's primary control is a choice between named,
/// described starting points; seed and tick budget are the only overrides, and
/// both are optional.
#[must_use]
pub fn new_run_form(presets: &[crate::presets::Preset], csrf: &Csrf) -> Markup {
    html! {
        form class="new-run" method="post" action=(crate::routes::paths::create_run()) {
            (csrf.hidden_input())
            h1 { "Start a run" }
            ul class="presets" {
                @for (i, p) in presets.iter().enumerate() {
                    li class="preset-card" data-slug=(p.slug) {
                        label for=(format!("preset-{}", p.slug)) {
                            input type="radio"
                                  id=(format!("preset-{}", p.slug))
                                  name="preset"
                                  value=(p.slug)
                                  checked[i == 0];
                            span class="preset-name" { (p.name) }
                        }
                        p class="preset-description" { (p.description) }
                        dl class="preset-params" {
                            dt { "Agents" }
                            dd { (p.params.agent_count) }
                            dt { "World" }
                            dd { (num(p.params.world.width)) " × " (num(p.params.world.height)) }
                            dt { "Sep / align / coh" }
                            dd {
                                (num(p.params.w_separation)) " / "
                                (num(p.params.w_alignment)) " / "
                                (num(p.params.w_cohesion))
                            }
                            dt { "Goal weight" }
                            dd { (num(p.params.w_goal)) }
                            dt { "Obstacles" }
                            dd { (p.params.obstacles.len()) }
                        }
                    }
                }
            }
            div class="new-run-overrides" {
                label for="seed" { "Seed" }
                input type="number" id="seed" name="seed"
                      value=(DEFAULT_SEED) min="0" step="1";
                label for="max_ticks" { "Tick budget" }
                input type="number" id="max_ticks" name="max_ticks"
                      value=(DEFAULT_MAX_TICKS) min="1" step="1";
            }
            button class="submit" type="submit" { "Queue run" }
        }
    }
}

/// Seed offered when the user does not choose one.
pub const DEFAULT_SEED: i64 = 1;
/// Tick budget offered when the user does not choose one (AC-35's guardrail
/// has to start somewhere).
pub const DEFAULT_MAX_TICKS: i32 = 300;

// ────────────────────── the assembled run detail page ──────────────────────

/// Everything the run detail page draws.
///
/// The handler's entire job is to fill this in; every field is data the page
/// needs and none of it is something the page could go and fetch.
#[derive(Debug, Clone)]
pub struct RunDetail {
    /// The run.
    pub run: crate::models::Run,
    /// Name of the scenario it came from.
    pub scenario_name: String,
    /// The world the run took place in, read from its config snapshot.
    pub world: World,
    /// Obstacles from the same snapshot.
    pub obstacles: Vec<Obstacle>,
    /// Agents of the most recent stored frame.
    pub latest_agents: Vec<Agent>,
    /// `(tick, agents)` for the frames the trajectory is drawn from — already
    /// subsampled by the handler's query budget, and subsampled again by
    /// [`trajectory_svg`] if it is still too many.
    pub trajectory: Vec<(i32, Vec<Agent>)>,
    /// Metric series across the run, oldest first.
    pub series: Vec<FrameMetrics>,
    /// Whether the flock has stopped making progress (**AC-27**), as decided by
    /// [`crate::analysis::run_is_stuck`].
    ///
    /// A **flag**, not a computation the page performs: the frames the answer is
    /// derived from are the handler's to load, and keeping the verdict in the
    /// data means the badge can be rendered — and tested — from a `bool`.
    pub stuck: bool,
    /// The CSRF token the page's cancel form must carry. [`Csrf::absent`] when
    /// no CSRF layer is mounted.
    pub csrf: Csrf,
}

/// The run detail page (AC-43, AC-44, AC-45, AC-47).
#[must_use]
pub fn run_detail_page(d: &RunDetail) -> Markup {
    html! {
        h1 { (d.scenario_name) " · run #" (d.run.id) }
        (progress_fragment(&d.run))
        @if d.stuck { (stuck_badge()) }
        @if !crate::models::run::status::is_terminal(&d.run.status) {
            (cancel_form(d.run.id, &d.csrf))
        }
        div class="panels" {
            figure class="panel" {
                figcaption { "Latest frame" }
                (flock_svg(&d.latest_agents, &d.world, &d.obstacles, FlockOpts::default()))
            }
            figure class="panel" {
                figcaption { "Trajectories" }
                (trajectory_svg(&d.trajectory, &d.world, TrajectoryOpts::default()))
            }
        }
        section class="panel" {
            h2 { "Metrics" }
            (metrics_panel(&d.series))
        }
        (provenance_panel(&d.run))
        form class="compare-cta" method="get" action=(crate::routes::paths::compare()) {
            input type="hidden" name="a" value=(d.run.id);
            label for="compare-b" { "Compare this run with run #" }
            input type="number" id="compare-b" name="b" min="1" step="1";
            button type="submit" { "Compare" }
        }
    }
}
