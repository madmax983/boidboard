//! Geometry: worlds, flocks, ribbons and sparklines, as inline SVG.
//!
//! Everything in here turns numbers into shapes. It is the half of `views` that
//! has arithmetic in it — coordinate scaling, heading angles, seam detection,
//! series normalisation — and it is separated from `pages` for that reason: the
//! bugs live in the arithmetic, and the arithmetic is what the degenerate-case
//! tests are aimed at.
//!
//! Every function here is still pure `fn(data) -> Markup`; see the module docs
//! on [`crate::views`] for what that rules out.
//!
//! **The rendering budgets live here too** (`FlockOpts::max_agents`,
//! `TrajectoryOpts::max_agents` / `max_frames`), because a budget is a drawing
//! decision: a 10 000-tick run must not put 10 000 marks on a page whatever the
//! handler hands over.

use autumn_web::prelude::*;
use boids_core::{Agent, Obstacle, World};

// ─────────────────────────── numeric formatting ───────────────────────────

/// Format a coordinate for an SVG attribute.
///
/// `pub(super)` because the preset cards in [`super::pages`] print world
/// dimensions with it — same rounding, same trimming, one implementation.
///
/// Three decimals is far below one device pixel at any plausible zoom, and
/// trimming the trailing zeros keeps the emitted document readable and small —
/// a 400-agent frame writes several thousand numbers.
///
/// Non-finite input renders as `0` rather than `NaN`: a `NaN` in a `points`
/// attribute silently drops the whole shape in every renderer, which is far
/// harder to diagnose than a mark at the origin.
pub(super) fn num(v: f64) -> String {
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
