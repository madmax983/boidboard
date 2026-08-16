//! Pure rendering functions.
//!
//! **Every function in this module is `fn(data) -> Markup`.** None of them
//! touches a database, a request, the clock or the filesystem, which is the
//! whole point: the visual behaviour of Boidboard is unit-testable with plain
//! structs and no infrastructure at all (AC-50), and route handlers are left
//! with nothing to do but fetch and delegate.
//!
//! Two consequences worth stating because they are easy to erode:
//!
//! * A view never *queries* for what it needs. If a panel needs the latest
//!   metrics, the metrics are a parameter.
//! * A view never decides how much data exists — but it always decides how much
//!   it will *draw*. Rendering budgets (`FlockOpts::max_agents`,
//!   `TrajectoryOpts::max_agents` / `max_frames`) live here, so a 10 000-tick
//!   run cannot put 10 000 marks on a page no matter what the handler hands
//!   over.
//!
//! Elements carry stable semantic classes (`table.runs`, `tr.run-row`,
//! `svg.flock`, `.agent`, `.trajectory`, `.sparkline`, `.provenance`,
//! `.preset-card`, `.compare`). Those class names are the contract the tests
//! and the stylesheet are both written against.

use autumn_web::prelude::*;
use boids_core::metrics::FrameMetrics;
use boids_core::{Agent, Obstacle, World};

// ─────────────────────────── numeric formatting ───────────────────────────

/// Format a coordinate for an SVG attribute.
///
/// Three decimals is far below one device pixel at any plausible zoom, and
/// trimming the trailing zeros keeps the emitted document readable and small —
/// a 400-agent frame writes several thousand numbers.
///
/// Non-finite input renders as `0` rather than `NaN`: a `NaN` in a `points`
/// attribute silently drops the whole shape in every renderer, which is far
/// harder to diagnose than a mark at the origin.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    let mut s = format!("{v:.3}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if s == "-0" {
        s = "0".to_owned();
    }
    s
}

/// Format a metric for display, with a fixed number of decimals.
fn metric(v: f64, places: usize) -> String {
    if v.is_finite() {
        format!("{v:.places$}")
    } else {
        "—".to_owned()
    }
}

/// Evenly spaced indices into a collection of `len` items, at most `budget` of
/// them, always including the first and last.
///
/// This is the rendering budget primitive: subsampling *evenly* rather than
/// truncating means a trajectory still spans the whole run, and pinning the
/// endpoints means the start and the current state are never the frames that
/// get dropped.
fn even_indices(len: usize, budget: usize) -> Vec<usize> {
    if len == 0 || budget == 0 {
        return Vec::new();
    }
    if budget >= len {
        return (0..len).collect();
    }
    if budget == 1 {
        return vec![0];
    }
    let last = len - 1;
    let divisor = budget - 1;
    (0..budget)
        .map(|slot| (2 * slot * last + divisor) / (2 * divisor))
        .collect()
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

/// Embedded stylesheet. Deliberately small and dependency-free.
const STYLESHEET: &str = r"
:root { color-scheme: light dark; --ink:#16181d; --dim:#666e7a; --line:#d8dce3;
        --bg:#fbfbfd; --panel:#fff; --accent:#2f6fed; }
@media (prefers-color-scheme: dark) {
  :root { --ink:#e8eaf0; --dim:#9aa3b2; --line:#2b3140; --bg:#101319; --panel:#171b23; }
}
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink);
       font:15px/1.5 ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif; }
.masthead { display:flex; gap:1.5rem; align-items:baseline;
            padding:.9rem 1.4rem; border-bottom:1px solid var(--line); }
.brand { font-weight:700; letter-spacing:-.02em; text-decoration:none; color:var(--ink); }
.masthead-nav a { margin-right:1rem; color:var(--dim); text-decoration:none; }
.page { max-width:76rem; margin:0 auto; padding:1.4rem; }
h1 { font-size:1.4rem; margin:0 0 1rem; letter-spacing:-.02em; }
h2 { font-size:1rem; margin:0 0 .6rem; letter-spacing:-.01em; }
table.runs { width:100%; border-collapse:collapse; font-variant-numeric:tabular-nums; }
table.runs th, table.runs td { text-align:left; padding:.5rem .6rem;
                               border-bottom:1px solid var(--line); }
table.runs th { font-size:.78rem; text-transform:uppercase; letter-spacing:.06em;
                color:var(--dim); }
.status-badge { display:inline-block; padding:.1rem .5rem; border-radius:99px;
                font-size:.78rem; border:1px solid var(--line); }
.status-badge[data-status='running'] { border-color:var(--accent); color:var(--accent); }
.status-badge[data-status='failed'], .status-badge[data-status='budget_exceeded']
                { border-color:#c0392b; color:#c0392b; }
.status-badge[data-status='completed'] { border-color:#2e7d4f; color:#2e7d4f; }
.panels { display:grid; gap:1.2rem; grid-template-columns:repeat(auto-fit,minmax(20rem,1fr)); }
.panel { background:var(--panel); border:1px solid var(--line); border-radius:.6rem;
         padding:1rem; }
svg.flock, svg.trajectories { width:100%; height:auto; display:block;
                              background:var(--panel); border-radius:.4rem; }
.world-bounds { fill:none; stroke:var(--line); }
.agent { fill:var(--accent); stroke:none; }
.obstacle { fill:rgba(192,57,43,.15); stroke:#c0392b; }
.trail { fill:none; stroke:var(--accent); stroke-width:.6; opacity:.65; }
.trail-dot { fill:var(--accent); opacity:.65; }
.goal { fill:none; stroke:#2e7d4f; stroke-dasharray:3 3; }
.metrics { display:grid; gap:.9rem; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); }
.metric-card { margin:0; }
.metric-card figcaption { font-size:.78rem; color:var(--dim); text-transform:uppercase;
                          letter-spacing:.06em; }
svg.sparkline { width:100%; height:2.4rem; display:block; }
.spark-line { fill:none; stroke:var(--accent); stroke-width:1.2; }
.spark-baseline { stroke:var(--line); stroke-width:.5; }
.metric-latest { font-variant-numeric:tabular-nums; font-size:1.1rem; }
.provenance dl { display:grid; grid-template-columns:auto 1fr; gap:.3rem .9rem; margin:0; }
.provenance dt { color:var(--dim); font-size:.82rem; }
.provenance dd { margin:0; font-family:ui-monospace, SFMono-Regular, Menlo, monospace;
                 font-size:.82rem; word-break:break-all; }
.prov-pending { color:var(--dim); font-style:italic; }
.presets { display:grid; gap:.9rem; list-style:none; margin:0 0 1.2rem; padding:0;
           grid-template-columns:repeat(auto-fit,minmax(16rem,1fr)); }
.preset-card { background:var(--panel); border:1px solid var(--line); border-radius:.6rem;
               padding:.9rem; }
.preset-name { font-weight:600; }
.preset-description { color:var(--dim); font-size:.86rem; }
.preset-params { display:grid; grid-template-columns:auto 1fr; gap:.1rem .6rem;
                 font-size:.78rem; color:var(--dim); margin:.6rem 0 0; }
.preset-params dd { margin:0; font-variant-numeric:tabular-nums; }
.compare { display:grid; gap:1.2rem; }
.compare-sides { display:grid; gap:1.2rem; grid-template-columns:repeat(auto-fit,minmax(20rem,1fr)); }
table.config-diff { border-collapse:collapse; width:100%; font-variant-numeric:tabular-nums; }
table.config-diff th, table.config-diff td { text-align:left; padding:.35rem .6rem;
                                             border-bottom:1px solid var(--line); }
.run-progress { display:flex; gap:.8rem; align-items:center; }
progress.progress-bar { width:14rem; }
.empty { color:var(--dim); font-style:italic; }
";

// ────────────────────────────── flock rendering ──────────────────────────────

/// Rendering options for [`flock_svg`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlockOpts {
    /// Longest on-screen edge, in CSS pixels. The other edge follows the
    /// world's aspect ratio.
    pub px: u32,
    /// Hard cap on the number of agent marks drawn. Above this the flock is
    /// subsampled evenly; a frame of 5 000 agents is a page-weight problem
    /// long before it is a legibility problem.
    pub max_agents: usize,
    /// Length of an agent mark along its heading, in **world** units.
    pub mark: f64,
}

impl Default for FlockOpts {
    fn default() -> Self {
        Self {
            px: 560,
            max_agents: 400,
            mark: 3.5,
        }
    }
}

/// On-screen pixel dimensions for a world drawn into an `px`-sized box.
fn fit(world: &World, px: u32) -> (f64, f64) {
    let longest = world.width.max(world.height);
    if !longest.is_finite() || longest <= 0.0 {
        return (f64::from(px), f64::from(px));
    }
    let scale = f64::from(px) / longest;
    (world.width * scale, world.height * scale)
}

/// The flock as inline SVG: one **oriented** mark per agent, plus obstacles
/// and the world bounds (AC-43).
///
/// Each agent is a triangle translated to its position and rotated to its
/// heading, so a still frame shows which way the flock is going — a dot
/// scatter does not, and heading is most of what makes a boids run readable.
/// The rotation is `atan2(vy, vx)` in degrees, measured in SVG's screen frame
/// where **y grows downward**, so a velocity of `(0, 1)` renders as `90°`.
///
/// A stationary agent has no heading; it renders unrotated rather than
/// disappearing or producing `NaN`.
#[must_use]
pub fn flock_svg(
    agents: &[Agent],
    world: &World,
    obstacles: &[Obstacle],
    opts: FlockOpts,
) -> Markup {
    let (w_px, h_px) = fit(world, opts.px);
    let shown = even_indices(agents.len(), opts.max_agents);
    let label = format!(
        "Flock of {} agents in a {} by {} toroidal world",
        agents.len(),
        num(world.width),
        num(world.height)
    );

    html! {
        svg class="flock"
            role="img"
            aria-label=(label)
            viewBox=(format!("0 0 {} {}", num(world.width), num(world.height)))
            width=(num(w_px))
            height=(num(h_px))
            preserveAspectRatio="xMidYMid meet"
        {
            rect class="world-bounds" x="0" y="0"
                 width=(num(world.width)) height=(num(world.height)) {}
            g class="obstacles" {
                @for o in obstacles {
                    circle class="obstacle"
                           cx=(num(o.center.x)) cy=(num(o.center.y)) r=(num(o.radius)) {}
                }
            }
            g class="agents" {
                @for i in shown {
                    @if let Some(a) = agents.get(i) {
                        (agent_mark(a, opts.mark))
                    }
                }
            }
        }
    }
}

/// One agent as an oriented triangle.
fn agent_mark(a: &Agent, mark: f64) -> Markup {
    let heading = if a.vel.x == 0.0 && a.vel.y == 0.0 {
        0.0
    } else {
        a.vel.y.atan2(a.vel.x).to_degrees()
    };
    let nose = mark;
    let tail = -mark * 0.55;
    let half = mark * 0.45;
    let points = format!(
        "{},0 {},{} {},{}",
        num(nose),
        num(tail),
        num(half),
        num(tail),
        num(-half)
    );
    let transform = format!(
        "translate({} {}) rotate({})",
        num(a.pos.x),
        num(a.pos.y),
        num(heading)
    );
    html! {
        polygon class="agent" data-agent-id=(a.id) points=(points) transform=(transform) {}
    }
}

// ──────────────────────────── trajectory ribbons ────────────────────────────

/// Rendering options for [`trajectory_svg`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrajectoryOpts {
    /// Longest on-screen edge, in CSS pixels.
    pub px: u32,
    /// How many agents get a ribbon. Thirty distinguishable paths is already
    /// past what a reader can follow; a hundred and twenty is a grey smear
    /// that also costs a megabyte.
    pub max_agents: usize,
    /// How many frames each ribbon samples. Sixty points draws a smooth path
    /// at any realistic figure size.
    pub max_frames: usize,
}

impl Default for TrajectoryOpts {
    fn default() -> Self {
        Self {
            px: 560,
            max_agents: 30,
            max_frames: 60,
        }
    }
}

/// Trajectory ribbons: each sampled agent's path across the run, as one or
/// more polylines (AC-43).
///
/// **Ribbons break at the toroidal seam.** An agent that leaves the right edge
/// and re-enters at the left is at `x = 199` on one frame and `x = 1` on the
/// next; joining those with a line segment paints a horizontal streak across
/// the entire world — a wrong picture that reads as a real behaviour. Any step
/// longer than half a world dimension is therefore a wrap, and the ribbon is
/// cut there and resumed on the far side.
///
/// **Budgeted.** Frames are subsampled to `max_frames` and agents to
/// `max_agents`, both evenly and both keeping the endpoints, so the ribbon
/// still spans the whole run. A run of 10 000 ticks and 5 000 agents renders
/// the same size document as a run of 60 ticks and 30 agents.
#[must_use]
pub fn trajectory_svg(frames: &[(i32, Vec<Agent>)], world: &World, opts: TrajectoryOpts) -> Markup {
    let (w_px, h_px) = fit(world, opts.px);
    let sampled: Vec<&(i32, Vec<Agent>)> = even_indices(frames.len(), opts.max_frames)
        .into_iter()
        .filter_map(|i| frames.get(i))
        .collect();

    let mut ids: Vec<u32> = sampled
        .iter()
        .flat_map(|(_, agents)| agents.iter().map(|a| a.id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let ids: Vec<u32> = even_indices(ids.len(), opts.max_agents)
        .into_iter()
        .filter_map(|i| ids.get(i).copied())
        .collect();

    let dot = (world.width.max(world.height) / 400.0).max(0.4);
    let label = format!(
        "Trajectories of {} agents over {} sampled frames",
        ids.len(),
        sampled.len()
    );

    html! {
        svg class="trajectories"
            role="img"
            aria-label=(label)
            viewBox=(format!("0 0 {} {}", num(world.width), num(world.height)))
            width=(num(w_px))
            height=(num(h_px))
            preserveAspectRatio="xMidYMid meet"
        {
            rect class="world-bounds" x="0" y="0"
                 width=(num(world.width)) height=(num(world.height)) {}
            @for id in ids {
                g class="trajectory" data-agent-id=(id) {
                    @for segment in seam_split(&path_of(&sampled, id), world) {
                        @if segment.len() >= 2 {
                            polyline class="trail" points=(points_attr(&segment)) {}
                        } @else if let Some(p) = segment.first() {
                            circle class="trail-dot"
                                   cx=(num(p.0)) cy=(num(p.1)) r=(num(dot)) {}
                        }
                    }
                }
            }
        }
    }
}

/// One agent's positions across the sampled frames, in frame order. Frames
/// where the agent is absent are simply skipped.
fn path_of(sampled: &[&(i32, Vec<Agent>)], id: u32) -> Vec<(f64, f64)> {
    sampled
        .iter()
        .filter_map(|(_, agents)| agents.iter().find(|a| a.id == id))
        .map(|a| (a.pos.x, a.pos.y))
        .collect()
}

/// Cut a path wherever it crosses a seam of the torus.
///
/// A step longer than half a world dimension cannot be a real step — the
/// minimum-image convention makes half a world the largest distance the torus
/// admits on that axis — so it is a wrap, and the ribbon must not be drawn
/// through it.
fn seam_split(path: &[(f64, f64)], world: &World) -> Vec<Vec<(f64, f64)>> {
    let mut segments: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut current: Vec<(f64, f64)> = Vec::new();
    for (i, &p) in path.iter().enumerate() {
        if i > 0 {
            let prev = path[i - 1];
            if wraps(prev, p, world) {
                segments.push(std::mem::take(&mut current));
            }
        }
        current.push(p);
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Whether the step from `a` to `b` crossed a seam.
fn wraps(a: (f64, f64), b: (f64, f64), world: &World) -> bool {
    let crossed = |d: f64, size: f64| size.is_finite() && size > 0.0 && d.abs() > size / 2.0;
    crossed(b.0 - a.0, world.width) || crossed(b.1 - a.1, world.height)
}

/// Render a coordinate list as an SVG `points` attribute value.
fn points_attr(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{},{}", num(*x), num(*y)))
        .collect::<Vec<_>>()
        .join(" ")
}

// ──────────────────────────────── sparklines ────────────────────────────────

/// Rendering options for [`sparkline`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SparkOpts {
    /// `viewBox` width in user units.
    pub w: f64,
    /// `viewBox` height in user units.
    pub h: f64,
    /// Inset kept clear on every side so the stroke is not clipped at the
    /// extremes.
    pub pad: f64,
}

impl Default for SparkOpts {
    fn default() -> Self {
        Self {
            w: 100.0,
            h: 24.0,
            pad: 2.0,
        }
    }
}

/// A metric series as a small inline SVG polyline (AC-44).
///
/// The `viewBox` is **fixed** — it does not depend on the number of samples —
/// so a run's sparklines stay the same size and shape as it grows, and a
/// polling swap does not make the page jump.
///
/// Three degenerate cases are the whole difficulty, and each is handled
/// deliberately rather than by accident:
///
/// * **Empty** — no polyline at all, just a baseline and the
///   `sparkline-empty` marker class. A queued run's charts are empty, not
///   missing.
/// * **One sample** — drawn as a flat segment across the full width. A
///   one-point polyline is silently invisible.
/// * **Zero range** (every value equal) — the normalisation `(v - min) /
///   (max - min)` is `0/0` here; the series is pinned to the vertical middle
///   instead, so a flat metric renders as a flat line rather than as `NaN`,
///   which would drop the whole shape.
///
/// Non-finite samples are dropped before plotting: they are gaps in the data,
/// and letting one reach the `points` attribute would erase the chart.
#[must_use]
pub fn sparkline(values: &[f64], opts: SparkOpts) -> Markup {
    let vals: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    let view_box = format!("0 0 {} {}", num(opts.w), num(opts.h));
    let mid = opts.h / 2.0;

    if vals.is_empty() {
        return html! {
            svg class="sparkline sparkline-empty" role="img" aria-label="No data yet"
                viewBox=(view_box) preserveAspectRatio="none" {
                line class="spark-baseline" x1=(num(opts.pad)) y1=(num(mid))
                     x2=(num(opts.w - opts.pad)) y2=(num(mid)) {}
            }
        };
    }

    let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    let top = opts.pad;
    let bottom = opts.h - opts.pad;
    let left = opts.pad;
    let right = opts.w - opts.pad;

    let y_of = |v: f64| {
        if range <= 0.0 {
            mid
        } else {
            bottom - ((v - min) / range) * (bottom - top)
        }
    };

    let points: Vec<(f64, f64)> = if vals.len() == 1 {
        let y = y_of(vals[0]);
        vec![(left, y), (right, y)]
    } else {
        let last = (vals.len() - 1) as f64;
        vals.iter()
            .enumerate()
            .map(|(i, v)| (left + (i as f64 / last) * (right - left), y_of(*v)))
            .collect()
    };

    let label = format!("{} samples, from {} to {}", vals.len(), num(min), num(max));
    html! {
        svg class="sparkline" role="img" aria-label=(label)
            viewBox=(view_box) preserveAspectRatio="none" {
            polyline class="spark-line" points=(points_attr(&points)) {}
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
    let cards: [(&str, &str, Vec<f64>, usize); 4] = [
        (
            "polarization",
            "Polarization",
            series.iter().map(|m| m.polarization).collect(),
            3,
        ),
        (
            "mean_nearest_neighbor_distance",
            "Mean nearest-neighbour distance",
            series
                .iter()
                .map(|m| m.mean_nearest_neighbor_distance)
                .collect(),
            2,
        ),
        (
            "collisions",
            "Collisions",
            series.iter().map(|m| m.collisions as f64).collect(),
            0,
        ),
        (
            "mean_speed",
            "Mean speed",
            series.iter().map(|m| m.mean_speed).collect(),
            2,
        ),
    ];

    html! {
        section class="metrics" {
            @for (field, label, values, places) in cards {
                figure class="metric-card" data-metric=(field) {
                    figcaption { (label) }
                    (sparkline(&values, SparkOpts::default()))
                    span class="metric-latest" {
                        @match values.last() {
                            Some(v) => (metric(*v, places)),
                            None => "—",
                        }
                    }
                }
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

// ─────────────────────── the new-run form (AC-42) ───────────────────────

/// The new-run form, fronted by preset cards (AC-42).
///
/// The parameter space is fifteen coupled floats, most combinations of which
/// produce a static cloud or an explosion. Offering it blank is offering
/// nothing, so the form's primary control is a choice between named,
/// described starting points; seed and tick budget are the only overrides, and
/// both are optional.
#[must_use]
pub fn new_run_form(presets: &[crate::presets::Preset]) -> Markup {
    html! {
        form class="new-run" method="post" action=(crate::routes::paths::create_run()) {
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
}

/// The run detail page (AC-43, AC-44, AC-45, AC-47).
#[must_use]
pub fn run_detail_page(d: &RunDetail) -> Markup {
    html! {
        h1 { (d.scenario_name) " · run #" (d.run.id) }
        (progress_fragment(&d.run))
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
