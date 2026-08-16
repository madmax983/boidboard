//! Web-interface acceptance tests (AC-41 … AC-48, AC-50).
//!
//! Two layers, deliberately separated:
//!
//! * **View unit tests** — plain data in, `Markup` out. No `TestApp`, no
//!   database, no runtime. This is the executable proof of AC-50: if a view
//!   needed a connection or a request, none of these could compile.
//! * **Route tests** — `autumn_web::test::TestApp`, asserting on *HTML
//!   structure* through the `assert_selector*` family rather than on raw
//!   substrings (AC-48).

use boidboard::presets;

// ───────────────────────────── AC-42 — presets ─────────────────────────────

#[test]
fn ac42_presets_offer_at_least_four_distinct_named_starting_points() {
    let all = presets::all();
    assert!(
        all.len() >= 4,
        "a user must never face a blank parameter form: expected >= 4 presets, got {}",
        all.len()
    );

    let mut slugs: Vec<&str> = all.iter().map(|p| p.slug).collect();
    slugs.sort_unstable();
    let unique = {
        let mut s = slugs.clone();
        s.dedup();
        s.len()
    };
    assert_eq!(
        unique,
        slugs.len(),
        "preset slugs must be unique: {slugs:?}"
    );

    for p in &all {
        assert!(!p.name.is_empty(), "preset `{}` needs a name", p.slug);
        assert!(
            !p.description.is_empty(),
            "preset `{}` needs a description",
            p.slug
        );
    }
}

#[test]
fn ac42_presets_have_genuinely_different_character() {
    let all = presets::all();
    // The whole point of a preset gallery is that the presets *behave*
    // differently. Two presets with identical parameters are one preset with
    // two names, so the weight triples must all be distinct.
    let mut shapes: Vec<String> = all
        .iter()
        .map(|p| {
            format!(
                "{:?}|{:?}|{:?}|{:?}",
                p.params.w_separation, p.params.w_alignment, p.params.w_cohesion, p.params.w_goal
            )
        })
        .collect();
    shapes.sort();
    let before = shapes.len();
    shapes.dedup();
    assert_eq!(
        shapes.len(),
        before,
        "every preset must have a distinct steering-weight profile"
    );
}

#[test]
fn ac42_by_slug_round_trips_and_rejects_unknown_slugs() {
    for p in presets::all() {
        let found = presets::by_slug(p.slug)
            .unwrap_or_else(|| panic!("by_slug must find the preset it advertises: {}", p.slug));
        assert_eq!(found.slug, p.slug);
        assert_eq!(found.name, p.name);
        assert_eq!(found.params, p.params);
    }
    assert!(presets::by_slug("no-such-preset").is_none());
    assert!(presets::by_slug("").is_none());
}

#[test]
fn ac42_every_preset_is_a_valid_simulation_config() {
    // The kernel's own validator is the authority — a preset the simulator
    // would refuse is worse than no preset at all, because the failure only
    // surfaces once the run has been queued.
    for p in presets::all() {
        if let Err(problems) = boids_core::sim::validate(&p.params) {
            panic!("preset `{}` is not a valid config: {problems:?}", p.slug);
        }
    }

    // The invariants that matter most for a *preset* specifically, asserted
    // directly so this test still says what it means if `validate` is ever
    // relaxed.
    for p in presets::all() {
        let s = p.slug;
        assert!(
            p.params.world.width > 0.0 && p.params.world.height > 0.0,
            "{s}: world must have positive dimensions"
        );
        assert!(
            p.params.agent_count > 0,
            "{s}: agent_count must be positive"
        );
        assert!(p.params.dt > 0.0, "{s}: dt must be positive");
        assert!(
            p.params.separation_radius <= p.params.neighbor_radius,
            "{s}: separation_radius must not exceed neighbor_radius"
        );
        assert!(
            p.params.neighbor_radius > 0.0,
            "{s}: neighbor_radius must be positive"
        );
        assert!(p.params.max_speed > 0.0, "{s}: max_speed must be positive");
        assert!(p.params.max_force > 0.0, "{s}: max_force must be positive");
        assert!(
            p.params.collision_radius > 0.0,
            "{s}: collision_radius must be positive"
        );
        assert!(
            p.params.goal_arrival_radius > 0.0,
            "{s}: goal_arrival_radius must be positive"
        );
        for o in &p.params.obstacles {
            assert!(o.radius > 0.0, "{s}: obstacle radius must be positive");
        }
    }
}

#[test]
fn ac42_presets_serialize_to_json_for_the_config_snapshot() {
    // A preset only earns its place if it can become a run's `config_snapshot`,
    // so round-tripping through JSON is part of the contract.
    for p in presets::all() {
        let json = serde_json::to_value(&p.params).expect("preset params serialize");
        let back: boids_core::SimParams =
            serde_json::from_value(json.clone()).expect("preset params round-trip");
        assert_eq!(
            back, p.params,
            "{}: JSON round-trip must be lossless",
            p.slug
        );
        assert!(json.is_object(), "{}: config must be a JSON object", p.slug);
    }
}

// ─────────────── tiny structural helpers for the pure-view layer ───────────────
//
// The route layer gets `assert_selector*` from `TestApp`; the view layer must
// prove it needs no `TestApp` at all (AC-50), so it gets these instead: a naive
// start-tag scanner that is entirely adequate for Maud output, which is always
// balanced and never puts `>` inside an attribute value.

mod dom {
    /// Every start tag in document order, as raw `<tag …>` text.
    pub fn start_tags(html: &str) -> Vec<&str> {
        let bytes = html.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<'
                && bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic)
                && let Some(end) = html[i..].find('>')
            {
                out.push(&html[i..=i + end]);
                i += end + 1;
                continue;
            }
            i += 1;
        }
        out
    }

    /// The value of a double-quoted attribute on a raw start tag.
    pub fn attr(tag: &str, name: &str) -> Option<String> {
        let key = format!(" {name}=\"");
        let start = tag.find(&key)? + key.len();
        let rest = &tag[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    fn has_class(tag: &str, class: &str) -> bool {
        attr(tag, "class").is_some_and(|c| c.split_whitespace().any(|x| x == class))
    }

    /// Every start tag carrying `class` in its class list.
    pub fn with_class<'a>(html: &'a str, class: &str) -> Vec<&'a str> {
        start_tags(html)
            .into_iter()
            .filter(|t| has_class(t, class))
            .collect()
    }

    /// How many start tags carry `class`.
    pub fn count_class(html: &str, class: &str) -> usize {
        with_class(html, class).len()
    }

    /// The markup from the start tag containing `start_needle` up to (not
    /// including) the next `end_needle`. Adequate because the groups this is
    /// used on never nest.
    pub fn section<'a>(html: &'a str, start_needle: &str, end_needle: &str) -> &'a str {
        let s = html
            .find(start_needle)
            .unwrap_or_else(|| panic!("markup does not contain `{start_needle}`:\n{html}"));
        let rest = &html[s..];
        let e = rest
            .find(end_needle)
            .unwrap_or_else(|| panic!("no `{end_needle}` after `{start_needle}`:\n{html}"));
        &rest[..e]
    }

    /// Parse a `points="x,y x,y …"` attribute into coordinate pairs.
    pub fn points(tag: &str) -> Vec<(f64, f64)> {
        attr(tag, "points")
            .unwrap_or_default()
            .split_whitespace()
            .map(|p| {
                let (x, y) = p
                    .split_once(',')
                    .unwrap_or_else(|| panic!("bad point `{p}`"));
                (
                    x.parse::<f64>().expect("x is a number"),
                    y.parse::<f64>().expect("y is a number"),
                )
            })
            .collect()
    }
}

// ──────────────────── AC-43 — inline SVG, oriented agent marks ────────────────────

use boidboard::views;
use boids_core::{Agent, Obstacle, Vec2, World};

fn agent(id: u32, pos: (f64, f64), vel: (f64, f64)) -> Agent {
    Agent {
        id,
        pos: Vec2::new(pos.0, pos.1),
        vel: Vec2::new(vel.0, vel.1),
    }
}

#[test]
fn ac43_flock_svg_is_inline_svg_with_a_world_sized_viewbox() {
    let world = World::new(400.0, 300.0);
    let html = views::flock_svg(
        &[agent(0, (10.0, 20.0), (1.0, 0.0))],
        &world,
        &[],
        views::FlockOpts::default(),
    )
    .into_string();

    let svg = *dom::with_class(&html, "flock")
        .first()
        .expect("an svg.flock element");
    assert!(
        svg.starts_with("<svg"),
        "the flock mark must be an <svg>: {svg}"
    );
    assert_eq!(dom::attr(svg, "viewBox").as_deref(), Some("0 0 400 300"));
    assert!(
        dom::attr(svg, "role").is_some() && dom::attr(svg, "aria-label").is_some(),
        "the flock svg must be labelled for assistive tech: {svg}"
    );
    assert!(
        !html.contains("<script") && !html.contains("<canvas") && !html.contains(".wasm"),
        "AC-43: the flock is server-rendered SVG — no script, no canvas, no WASM"
    );
}

#[test]
fn ac43_every_agent_is_an_oriented_mark_rotated_to_its_heading() {
    let world = World::new(100.0, 100.0);
    let agents = [
        agent(0, (10.0, 10.0), (1.0, 0.0)),  // east  → 0°
        agent(1, (20.0, 10.0), (0.0, 1.0)),  // south → 90° (SVG y grows downward)
        agent(2, (30.0, 10.0), (-1.0, 0.0)), // west  → 180°
        agent(3, (40.0, 10.0), (0.0, -1.0)), // north → -90°
    ];
    let html = views::flock_svg(&agents, &world, &[], views::FlockOpts::default()).into_string();

    let marks = dom::with_class(&html, "agent");
    assert_eq!(marks.len(), 4, "one mark per agent");

    let expected = ["rotate(0)", "rotate(90)", "rotate(180)", "rotate(-90)"];
    for (mark, want) in marks.iter().zip(expected) {
        let t = dom::attr(mark, "transform").expect("agent marks carry a transform");
        assert!(
            t.starts_with("translate("),
            "a mark must be placed before it is rotated: {t}"
        );
        assert!(
            t.contains(want),
            "expected transform to contain `{want}`, got `{t}`"
        );
    }

    // An oriented mark, not a dot: the shape must have three distinct vertices
    // so the heading is actually readable.
    let pts = dom::points(marks[0]);
    assert!(
        pts.len() >= 3,
        "an oriented mark needs at least three vertices, got {pts:?}"
    );
    assert!(
        marks
            .iter()
            .all(|m| dom::attr(m, "data-agent-id").is_some()),
        "each mark must carry its agent id"
    );
}

#[test]
fn ac43_flock_svg_renders_obstacles_and_survives_degenerate_input() {
    let world = World::new(200.0, 200.0);
    let obstacles = [
        Obstacle {
            center: Vec2::new(50.0, 60.0),
            radius: 12.0,
        },
        Obstacle {
            center: Vec2::new(150.0, 40.0),
            radius: 8.0,
        },
    ];
    let html = views::flock_svg(
        &[agent(0, (1.0, 1.0), (0.0, 0.0))],
        &world,
        &obstacles,
        views::FlockOpts::default(),
    )
    .into_string();

    let circles = dom::with_class(&html, "obstacle");
    assert_eq!(circles.len(), 2, "every obstacle is drawn");
    assert_eq!(dom::attr(circles[0], "cx").as_deref(), Some("50"));
    assert_eq!(dom::attr(circles[0], "cy").as_deref(), Some("60"));
    assert_eq!(dom::attr(circles[0], "r").as_deref(), Some("12"));
    assert!(
        !html.contains("NaN"),
        "a stationary agent has no heading, and must not produce NaN:\n{html}"
    );

    // An empty frame still renders a frame, not nothing.
    let empty = views::flock_svg(&[], &world, &[], views::FlockOpts::default()).into_string();
    assert_eq!(dom::count_class(&empty, "flock"), 1);
    assert_eq!(dom::count_class(&empty, "agent"), 0);
}

#[test]
fn ac43_flock_svg_respects_its_agent_budget() {
    let world = World::new(1000.0, 1000.0);
    let agents: Vec<Agent> = (0..900)
        .map(|i| agent(i, (f64::from(i) * 0.5, 10.0), (1.0, 0.0)))
        .collect();
    let opts = views::FlockOpts {
        max_agents: 50,
        ..views::FlockOpts::default()
    };
    let html = views::flock_svg(&agents, &world, &[], opts).into_string();
    assert_eq!(
        dom::count_class(&html, "agent"),
        50,
        "the page must not carry 900 marks when the budget is 50"
    );
}

// ───────────── AC-43 — trajectory ribbons, budgeted and seam-aware ─────────────

/// A straight-line walk that wraps around the right-hand seam of a 200-wide
/// world: x = 190, 195, 200→0, 5, 10.
fn wrapping_frames() -> Vec<(i32, Vec<Agent>)> {
    let xs = [190.0, 195.0, 0.0, 5.0, 10.0];
    xs.iter()
        .enumerate()
        .map(|(t, x)| {
            (
                i32::try_from(t).expect("tick fits"),
                vec![agent(0, (*x, 50.0), (1.0, 0.0))],
            )
        })
        .collect()
}

#[test]
fn ac43_trajectory_svg_draws_one_ribbon_group_per_agent() {
    let world = World::new(200.0, 200.0);
    let frames: Vec<(i32, Vec<Agent>)> = (0..5)
        .map(|t| {
            (
                t,
                vec![
                    agent(0, (10.0 + f64::from(t), 20.0), (1.0, 0.0)),
                    agent(1, (30.0 + f64::from(t), 60.0), (1.0, 0.0)),
                ],
            )
        })
        .collect();

    let html =
        views::trajectory_svg(&frames, &world, views::TrajectoryOpts::default()).into_string();

    let svg = *dom::with_class(&html, "trajectories")
        .first()
        .expect("an svg.trajectories element");
    assert_eq!(dom::attr(svg, "viewBox").as_deref(), Some("0 0 200 200"));
    assert_eq!(
        dom::count_class(&html, "trajectory"),
        2,
        "one ribbon group per agent"
    );
    assert_eq!(
        dom::count_class(&html, "trail"),
        2,
        "neither agent crosses a seam, so neither ribbon is broken"
    );

    let trail = dom::with_class(&html, "trail")[0];
    assert_eq!(
        dom::points(trail).len(),
        5,
        "a ribbon carries one point per sampled frame"
    );
}

#[test]
fn ac43_trajectory_ribbons_break_at_the_toroidal_seam() {
    // The bug this guards: a naive polyline joins x=195 to x=0 and paints a
    // horizontal streak straight across the whole world.
    let world = World::new(200.0, 200.0);
    let html = views::trajectory_svg(&wrapping_frames(), &world, views::TrajectoryOpts::default())
        .into_string();

    let group = dom::section(&html, r#"<g class="trajectory" data-agent-id="0""#, "</g>");
    let trails = dom::with_class(group, "trail");
    assert_eq!(
        trails.len(),
        2,
        "a wrapping path must be drawn as two ribbons, not one:\n{group}"
    );

    for trail in &trails {
        let pts = dom::points(trail);
        for pair in pts.windows(2) {
            let dx = (pair[1].0 - pair[0].0).abs();
            let dy = (pair[1].1 - pair[0].1).abs();
            assert!(
                dx <= world.width / 2.0 && dy <= world.height / 2.0,
                "no ribbon segment may span more than half the world \
                 (that is the seam streak): {pair:?}"
            );
        }
    }
}

#[test]
fn ac43_trajectory_svg_enforces_its_rendering_budget() {
    let world = World::new(1000.0, 1000.0);
    let frames: Vec<(i32, Vec<Agent>)> = (0..400)
        .map(|t| {
            (
                t,
                (0..120)
                    .map(|id| {
                        agent(
                            id,
                            (f64::from(id) * 2.0, f64::from(t) * 0.5 + f64::from(id)),
                            (0.0, 1.0),
                        )
                    })
                    .collect(),
            )
        })
        .collect();

    let opts = views::TrajectoryOpts::default();
    let html = views::trajectory_svg(&frames, &world, opts).into_string();

    let groups = dom::count_class(&html, "trajectory");
    assert!(
        groups <= opts.max_agents && groups > 0,
        "expected at most {} ribbon groups, got {groups}",
        opts.max_agents
    );
    for trail in dom::with_class(&html, "trail") {
        assert!(
            dom::points(trail).len() <= opts.max_frames,
            "a ribbon must not carry more than {} points",
            opts.max_frames
        );
    }
    assert!(
        html.len() < 400_000,
        "a 400-frame, 120-agent run must not emit an unbounded document ({} bytes)",
        html.len()
    );
}

#[test]
fn ac43_trajectory_svg_handles_empty_and_single_frame_input() {
    let world = World::new(100.0, 100.0);
    let opts = views::TrajectoryOpts::default();

    let empty = views::trajectory_svg(&[], &world, opts).into_string();
    assert_eq!(dom::count_class(&empty, "trajectories"), 1);
    assert_eq!(dom::count_class(&empty, "trajectory"), 0);
    assert!(!empty.contains("NaN"));

    // One frame is one point: there is no line to draw, so it renders as a dot
    // rather than an invisible one-point polyline.
    let one = views::trajectory_svg(&[(0, vec![agent(0, (5.0, 5.0), (1.0, 0.0))])], &world, opts)
        .into_string();
    assert_eq!(dom::count_class(&one, "trajectory"), 1);
    assert_eq!(dom::count_class(&one, "trail"), 0);
    assert_eq!(dom::count_class(&one, "trail-dot"), 1);
}

// ───────────────────────── AC-44 — metric sparklines ─────────────────────────

use boids_core::metrics::FrameMetrics;

fn metrics(polarization: f64, nnd: f64, collisions: usize, speed: f64) -> FrameMetrics {
    FrameMetrics {
        polarization,
        mean_nearest_neighbor_distance: nnd,
        collisions,
        mean_speed: speed,
        fraction_arrived: 0.0,
    }
}

#[test]
fn ac44_sparkline_maps_the_series_across_a_stable_viewbox() {
    let opts = views::SparkOpts::default();
    let html = views::sparkline(&[0.0, 5.0, 10.0], opts).into_string();

    let svg = *dom::with_class(&html, "sparkline")
        .first()
        .expect("an svg.sparkline element");
    let short = views::sparkline(&[1.0, 2.0], opts).into_string();
    let short_svg = *dom::with_class(&short, "sparkline").first().expect("svg");
    assert_eq!(
        dom::attr(svg, "viewBox"),
        dom::attr(short_svg, "viewBox"),
        "the viewBox must not depend on how many points the series has"
    );

    let line = *dom::with_class(&html, "spark-line")
        .first()
        .expect("a polyline.spark-line");
    let pts = dom::points(line);
    assert_eq!(pts.len(), 3, "one point per value");
    assert!(
        pts[0].0 < pts[1].0 && pts[1].0 < pts[2].0,
        "x must advance monotonically: {pts:?}"
    );
    assert!(
        pts[0].1 > pts[2].1,
        "the largest value must sit highest, i.e. at the smallest y: {pts:?}"
    );
}

#[test]
fn ac44_sparkline_survives_empty_single_and_flat_series() {
    let opts = views::SparkOpts::default();

    let empty = views::sparkline(&[], opts).into_string();
    assert_eq!(dom::count_class(&empty, "sparkline"), 1, "still an svg");
    assert_eq!(dom::count_class(&empty, "spark-line"), 0, "nothing to draw");
    assert_eq!(dom::count_class(&empty, "sparkline-empty"), 1);

    let single = views::sparkline(&[7.0], opts).into_string();
    let line = *dom::with_class(&single, "spark-line")
        .first()
        .expect("a single value still draws a line");
    let pts = dom::points(line);
    assert_eq!(pts.len(), 2, "one value is drawn as a flat segment");
    assert!((pts[0].1 - pts[1].1).abs() < f64::EPSILON);

    // Zero range: the naive (v - min) / (max - min) is 0/0 here.
    let flat = views::sparkline(&[3.0, 3.0, 3.0, 3.0], opts).into_string();
    assert!(
        !flat.contains("NaN") && !flat.contains("inf"),
        "a zero-range series must not divide by zero:\n{flat}"
    );
    let flat_pts = dom::points(dom::with_class(&flat, "spark-line")[0]);
    let ys: Vec<f64> = flat_pts.iter().map(|p| p.1).collect();
    assert!(
        ys.windows(2).all(|w| (w[0] - w[1]).abs() < f64::EPSILON),
        "a flat series must render flat: {ys:?}"
    );

    // Non-finite values must not poison the whole chart.
    let dirty = views::sparkline(&[1.0, f64::NAN, 3.0], opts).into_string();
    assert!(
        !dirty.contains("NaN"),
        "NaN must not reach the markup:\n{dirty}"
    );
}

#[test]
fn ac44_metrics_panel_gives_every_headline_metric_its_own_sparkline() {
    let series = vec![
        metrics(0.1, 5.0, 0, 1.0),
        metrics(0.5, 4.0, 2, 1.4),
        metrics(0.9, 3.0, 1, 1.8),
    ];
    let html = views::metrics_panel(&series).into_string();

    assert_eq!(
        dom::count_class(&html, "sparkline"),
        4,
        "polarization, mean NND, collisions and mean speed each get a sparkline"
    );
    for metric in [
        "polarization",
        "mean_nearest_neighbor_distance",
        "collisions",
        "mean_speed",
    ] {
        assert!(
            html.contains(&format!(r#"data-metric="{metric}""#)),
            "the panel must label its `{metric}` card with the persisted field name"
        );
    }
    let latest = dom::with_class(&html, "metric-latest");
    assert_eq!(latest.len(), 4, "each card shows its latest value");
}

#[test]
fn ac44_metrics_panel_renders_for_a_run_with_no_frames_yet() {
    let html = views::metrics_panel(&[]).into_string();
    assert_eq!(
        dom::count_class(&html, "sparkline"),
        4,
        "a queued run still shows its metric slots, empty"
    );
    assert!(!html.contains("NaN"));
}

// ─────────────── AC-47 — the reproducibility fingerprint is visible ───────────────

use boidboard::models::Run;
use boidboard::models::run::status as run_status;

fn run_fixture(id: i64, status: &str) -> Run {
    let epoch = chrono::NaiveDate::from_ymd_opt(2026, 8, 16)
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .expect("valid fixture timestamp");
    Run {
        id,
        scenario_id: 1,
        seed: 4242,
        status: status.to_owned(),
        max_ticks: 300,
        ticks_completed: 120,
        config_snapshot: serde_json::json!({ "agent_count": 120, "w_cohesion": 1.0 }),
        config_hash: "cfg-hash-aaaa".to_owned(),
        kernel_version: "boids-core@0.1.0".to_owned(),
        final_state_hash: Some("state-hash-bbbb".to_owned()),
        error: None,
        workflow_execution_id: None,
        created_at: epoch,
        updated_at: epoch,
    }
}

#[test]
fn ac47_provenance_panel_shows_every_field_needed_to_reproduce_a_run() {
    let run = run_fixture(7, run_status::COMPLETED);
    let html = views::provenance_panel(&run).into_string();

    assert_eq!(dom::count_class(&html, "provenance"), 1);
    for (class, expected) in [
        ("prov-final-state-hash", "state-hash-bbbb"),
        ("prov-config-hash", "cfg-hash-aaaa"),
        ("prov-seed", "4242"),
        ("prov-kernel-version", "boids-core@0.1.0"),
    ] {
        assert_eq!(
            dom::count_class(&html, class),
            1,
            "the provenance panel must carry a `.{class}` field"
        );
        assert!(
            html.contains(expected),
            "`.{class}` must surface `{expected}`:\n{html}"
        );
    }
    assert!(
        html.to_lowercase().contains("reproduc"),
        "the panel must say what it is — a reproducibility fingerprint:\n{html}"
    );
}

#[test]
fn ac47_provenance_panel_marks_a_missing_final_state_hash_as_pending() {
    let mut run = run_fixture(7, run_status::RUNNING);
    run.final_state_hash = None;
    let html = views::provenance_panel(&run).into_string();

    assert_eq!(dom::count_class(&html, "prov-final-state-hash"), 1);
    assert_eq!(
        dom::count_class(&html, "prov-pending"),
        1,
        "an unfinished run must say the fingerprint is not final yet, \
         not show a blank that reads as 'no hash':\n{html}"
    );
    assert!(
        !html.contains("None"),
        "Rust Option debug must not leak into the UI"
    );
}

// ───────────── AC-41 — run list with status and headline metrics ─────────────

#[test]
fn ac41_status_badge_carries_the_status_as_data_and_text() {
    for status in [
        run_status::QUEUED,
        run_status::RUNNING,
        run_status::COMPLETED,
        run_status::CANCELLED,
        run_status::FAILED,
        run_status::BUDGET_EXCEEDED,
    ] {
        let html = views::status_badge(status).into_string();
        let badge = *dom::with_class(&html, "status-badge")
            .first()
            .expect("a .status-badge");
        assert_eq!(
            dom::attr(badge, "data-status").as_deref(),
            Some(status),
            "the badge must be styleable and selectable by status"
        );
        assert!(
            html.contains(status),
            "the badge must name the status: {html}"
        );
    }
}

#[test]
fn ac41_runs_table_shows_one_row_per_run_with_status_and_headline_metrics() {
    let summaries = vec![
        views::RunSummary {
            run: run_fixture(1, run_status::RUNNING),
            scenario_name: "Classic Flock".to_owned(),
            headline: Some(metrics(0.812, 4.1, 3, 1.9)),
        },
        views::RunSummary {
            run: run_fixture(2, run_status::QUEUED),
            scenario_name: "Scatter".to_owned(),
            headline: None,
        },
    ];
    let html = views::runs_table(&summaries).into_string();

    assert_eq!(dom::count_class(&html, "runs"), 1, "one table.runs");
    assert_eq!(dom::count_class(&html, "run-row"), 2, "one row per run");

    let first = dom::section(&html, r#"<tr class="run-row" data-run-id="1""#, "</tr>");
    let link = *dom::with_class(first, "run-link")
        .first()
        .expect("a run link");
    assert_eq!(dom::attr(link, "href").as_deref(), Some("/runs/1"));
    assert!(
        first.contains("Classic Flock"),
        "the row names its scenario"
    );
    assert!(
        first.contains(r#"data-status="running""#),
        "AC-41: every row shows the run's status:\n{first}"
    );
    assert!(
        first.contains("0.812"),
        "AC-41: every row shows headline metrics:\n{first}"
    );

    // A run with no frames yet still gets a complete row.
    let second = dom::section(&html, r#"<tr class="run-row" data-run-id="2""#, "</tr>");
    assert_eq!(
        dom::count_class(second, "metric"),
        dom::count_class(first, "metric"),
        "a metric-less run must keep the table rectangular"
    );
    assert!(!second.contains("NaN") && !second.contains("None"));
}

#[test]
fn ac41_runs_table_says_so_when_there_are_no_runs() {
    let html = views::runs_table(&[]).into_string();
    assert_eq!(dom::count_class(&html, "run-row"), 0);
    assert_eq!(
        dom::count_class(&html, "empty"),
        1,
        "an empty list must be a message, not a blank page:\n{html}"
    );
}

// ────────────── AC-46 — compare two runs and show what differs ──────────────

#[test]
fn ac46_config_diff_lists_only_the_fields_that_actually_differ() {
    let a = serde_json::json!({
        "agent_count": 120,
        "w_cohesion": 1.0,
        "w_separation": 1.5,
        "world": { "width": 400.0, "height": 300.0 },
        "obstacles": [],
    });
    let b = serde_json::json!({
        "agent_count": 120,
        "w_cohesion": 0.02,
        "w_separation": 2.6,
        "world": { "width": 400.0, "height": 360.0 },
        "obstacles": [],
    });

    let diff = views::config_diff(&a, &b);
    let fields: Vec<&str> = diff.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(
        fields,
        vec!["w_cohesion", "w_separation", "world.height"],
        "only differing leaves, addressed by dotted path, in a stable order"
    );
    assert_eq!(diff[0].a, "1.0");
    assert_eq!(diff[0].b, "0.02");

    assert!(
        views::config_diff(&a, &a).is_empty(),
        "identical configs differ in nothing"
    );
}

#[test]
fn ac46_config_diff_reports_fields_present_on_only_one_side() {
    let a = serde_json::json!({ "goal": null, "backend": "naive" });
    let b = serde_json::json!({ "goal": { "x": 1.0, "y": 2.0 }, "backend": "naive" });
    let diff = views::config_diff(&a, &b);
    let fields: Vec<&str> = diff.iter().map(|d| d.field.as_str()).collect();
    assert!(
        fields.contains(&"goal") || fields.contains(&"goal.x"),
        "a field that exists on only one side is a difference: {fields:?}"
    );
    assert!(
        !fields.contains(&"backend"),
        "matching fields must not be listed: {fields:?}"
    );
}

#[test]
fn ac46_compare_view_renders_both_runs_and_highlights_the_differences() {
    let world = World::new(200.0, 200.0);
    let mut a = run_fixture(1, run_status::COMPLETED);
    a.config_snapshot = serde_json::json!({ "w_cohesion": 1.0, "agent_count": 120 });
    a.config_hash = "hash-a".to_owned();
    let mut b = run_fixture(2, run_status::COMPLETED);
    b.config_snapshot = serde_json::json!({ "w_cohesion": 0.02, "agent_count": 120 });
    b.config_hash = "hash-b".to_owned();

    let agents_a = vec![agent(0, (10.0, 10.0), (1.0, 0.0))];
    let agents_b = vec![
        agent(0, (100.0, 100.0), (0.0, 1.0)),
        agent(1, (120.0, 130.0), (0.0, 1.0)),
    ];

    let html = views::compare_view((&a, &agents_a), (&b, &agents_b), &world).into_string();

    assert_eq!(dom::count_class(&html, "compare"), 1);
    assert_eq!(
        dom::count_class(&html, "compare-side"),
        2,
        "both runs are rendered together"
    );
    assert_eq!(
        dom::count_class(&html, "flock"),
        2,
        "AC-46: the effect of the parameter change must be *visible*, \
         so each side draws its own flock"
    );
    assert_eq!(
        dom::count_class(&html, "provenance"),
        2,
        "each side carries its own fingerprint"
    );
    assert_eq!(dom::count_class(&html, "agent"), 3, "1 agent + 2 agents");

    assert_eq!(dom::count_class(&html, "config-diff"), 1);
    assert_eq!(
        dom::count_class(&html, "config-diff-row"),
        1,
        "exactly the one field that differs is listed"
    );
    let row = dom::with_class(&html, "config-diff-row")[0];
    assert_eq!(dom::attr(row, "data-field").as_deref(), Some("w_cohesion"));
    assert!(html.contains("hash-a") && html.contains("hash-b"));
}

#[test]
fn ac46_compare_view_says_so_when_two_runs_share_a_config() {
    let world = World::new(200.0, 200.0);
    let a = run_fixture(1, run_status::COMPLETED);
    let b = run_fixture(2, run_status::COMPLETED);
    let html = views::compare_view((&a, &[]), (&b, &[]), &world).into_string();

    assert_eq!(dom::count_class(&html, "config-diff-row"), 0);
    assert_eq!(
        dom::count_class(&html, "config-diff-empty"),
        1,
        "'these two runs used the same config' is a finding, not an empty table:\n{html}"
    );
}

// ────────── AC-45 — the htmx polling fragment (pure-view half) ──────────

#[test]
fn ac45_progress_fragment_polls_itself_while_a_run_is_not_terminal() {
    for status in [run_status::QUEUED, run_status::RUNNING] {
        let run = run_fixture(9, status);
        let html = views::progress_fragment(&run).into_string();

        let frag = *dom::with_class(&html, "run-progress")
            .first()
            .expect("a .run-progress fragment");
        assert_eq!(
            dom::attr(frag, "id").as_deref(),
            Some("run-progress"),
            "the fragment must swap itself out by a stable id"
        );
        assert_eq!(
            dom::attr(frag, "hx-get").as_deref(),
            Some("/runs/9/progress"),
            "a live run polls its own fragment endpoint"
        );
        assert_eq!(dom::attr(frag, "hx-trigger").as_deref(), Some("every 2s"));
        assert_eq!(dom::attr(frag, "hx-swap").as_deref(), Some("outerHTML"));
        assert!(
            html.contains("120") && html.contains("300"),
            "the fragment shows progress against the budget:\n{html}"
        );
    }
}

#[test]
fn ac45_progress_fragment_stops_polling_once_the_run_is_terminal() {
    for status in [
        run_status::COMPLETED,
        run_status::CANCELLED,
        run_status::FAILED,
        run_status::BUDGET_EXCEEDED,
    ] {
        let run = run_fixture(9, status);
        let html = views::progress_fragment(&run).into_string();
        let frag = *dom::with_class(&html, "run-progress")
            .first()
            .expect("a .run-progress fragment");

        assert!(
            dom::attr(frag, "hx-trigger").is_none() && dom::attr(frag, "hx-get").is_none(),
            "a `{status}` run is finished — polling it forever is a bug: {frag}"
        );
        assert!(
            html.contains(&format!(r#"data-status="{status}""#)),
            "the terminal fragment still reports the final status:\n{html}"
        );
    }
}

#[test]
fn ac45_progress_fragment_shows_the_error_of_a_failed_run() {
    let mut run = run_fixture(9, run_status::FAILED);
    run.error = Some("kernel panicked at tick 41".to_owned());
    let html = views::progress_fragment(&run).into_string();
    assert_eq!(dom::count_class(&html, "run-error"), 1);
    assert!(html.contains("kernel panicked at tick 41"));
}

// ─────────── AC-42 — the preset-fronted new-run form (pure-view half) ───────────

#[test]
fn ac42_new_run_form_is_fronted_by_preset_cards() {
    let all = presets::all();
    let html = views::new_run_form(&all, &views::Csrf::absent()).into_string();

    let form = *dom::start_tags(&html)
        .iter()
        .find(|t| t.starts_with("<form"))
        .expect("a form");
    assert_eq!(dom::attr(form, "method").as_deref(), Some("post"));
    assert_eq!(dom::attr(form, "action").as_deref(), Some("/runs"));

    assert_eq!(
        dom::count_class(&html, "preset-card"),
        all.len(),
        "every preset is offered as a card"
    );
    for p in &all {
        assert!(
            html.contains(&format!(r#"data-slug="{}""#, p.slug)),
            "preset `{}` must have a card",
            p.slug
        );
        assert!(html.contains(p.name), "card must name `{}`", p.slug);
        assert!(
            html.contains(p.description),
            "card must describe `{}` — a slug alone is still a blank form",
            p.slug
        );
    }

    // Exactly one preset is preselected, so submitting the form untouched works.
    let checked = dom::start_tags(&html)
        .iter()
        .filter(|t| t.starts_with("<input") && t.contains("checked"))
        .count();
    assert_eq!(checked, 1, "exactly one preset must be preselected");

    // Seed and tick budget are overridable, but never required.
    for name in ["seed", "max_ticks"] {
        let input = dom::start_tags(&html)
            .into_iter()
            .find(|t| t.starts_with("<input") && dom::attr(t, "name").as_deref() == Some(name))
            .unwrap_or_else(|| panic!("a `{name}` input"));
        assert!(
            !input.contains("required"),
            "`{name}` must be optional — presets are the point"
        );
    }
}

// ─────────────── CSRF — every state-changing form carries a token ───────────────
//
// The hand-written forms emitted no `_csrf` field at all, which made the app
// either CSRF-vulnerable (token check off) or broken (token check on, every
// submission a 403) depending on one config toggle. These tests pin the markup
// half: the field name and token come from the framework's own request
// extensions, never from a value this crate invents.

/// The hidden token input on a form, if it has one.
fn csrf_input<'a>(html: &'a str, field: &str) -> Option<&'a str> {
    dom::start_tags(html).into_iter().find(|t| {
        t.starts_with("<input")
            && dom::attr(t, "type").as_deref() == Some("hidden")
            && dom::attr(t, "name").as_deref() == Some(field)
    })
}

#[test]
fn csrf_the_new_run_form_carries_the_token_the_middleware_supplied() {
    let all = presets::all();
    let csrf = views::Csrf::new(views::Csrf::DEFAULT_FIELD, "tok-abc123");
    let html = views::new_run_form(&all, &csrf).into_string();

    let input = csrf_input(&html, "_csrf").unwrap_or_else(|| {
        panic!("POST /runs must carry a hidden `_csrf` input or every submission 403s:\n{html}")
    });
    assert_eq!(dom::attr(input, "value").as_deref(), Some("tok-abc123"));
}

#[test]
fn csrf_the_cancel_form_carries_the_token_too() {
    let csrf = views::Csrf::new(views::Csrf::DEFAULT_FIELD, "tok-cancel");
    let html = views::cancel_form(9, &csrf).into_string();

    let input = csrf_input(&html, "_csrf")
        .unwrap_or_else(|| panic!("POST /runs/9/cancel is a state change too:\n{html}"));
    assert_eq!(dom::attr(input, "value").as_deref(), Some("tok-cancel"));
}

#[test]
fn csrf_the_configured_field_name_is_honoured_rather_than_hardcoded() {
    let all = presets::all();
    // `security.csrf.form_field` is configurable, and the middleware publishes
    // the configured name in request extensions precisely so a template can use
    // it. A view that hardcoded `_csrf` would silently 403 under a renamed field.
    let csrf = views::Csrf::new("authenticity_token", "tok-renamed");
    let html = views::new_run_form(&all, &csrf).into_string();

    assert!(
        csrf_input(&html, "_csrf").is_none(),
        "the default name must not be emitted when another one is configured:\n{html}"
    );
    let input = csrf_input(&html, "authenticity_token")
        .unwrap_or_else(|| panic!("the configured field name must be used:\n{html}"));
    assert_eq!(dom::attr(input, "value").as_deref(), Some("tok-renamed"));
}

#[test]
fn csrf_no_middleware_means_no_field_rather_than_an_invented_token() {
    let all = presets::all();
    let absent = views::Csrf::absent();
    assert_eq!(absent, views::Csrf::default());

    for html in [
        views::new_run_form(&all, &absent).into_string(),
        views::cancel_form(9, &absent).into_string(),
    ] {
        assert!(
            !html.contains("_csrf"),
            "with no CSRF layer mounted there is no token to emit, and a made-up \
             one would be worse than none:\n{html}"
        );
    }
}

#[test]
fn csrf_the_compare_form_stays_a_get_and_needs_no_token() {
    // A `GET` is in `security.csrf.safe_methods` by default, so the compare
    // form neither needs nor should carry a token — it would land in the query
    // string, be bookmarked, and end up in logs and referrers.
    let mut d = detail_fixture(run_status::COMPLETED);
    d.csrf = views::Csrf::new(views::Csrf::DEFAULT_FIELD, "tok-detail");
    let html = views::run_detail_page(&d).into_string();

    let compare = dom::with_class(&html, "compare-cta");
    assert_eq!(compare.len(), 1);
    assert_eq!(dom::attr(compare[0], "method").as_deref(), Some("get"));
    assert!(
        !html.contains("tok-detail"),
        "a terminal run has no POST form on its page, so no token belongs in it:\n{html}"
    );
}

#[test]
fn csrf_the_detail_page_hands_its_token_to_the_cancel_form() {
    let mut d = detail_fixture(run_status::RUNNING);
    d.csrf = views::Csrf::new(views::Csrf::DEFAULT_FIELD, "tok-detail");
    let html = views::run_detail_page(&d).into_string();

    let input = csrf_input(&html, "_csrf")
        .unwrap_or_else(|| panic!("the page's cancel form must be submittable:\n{html}"));
    assert_eq!(dom::attr(input, "value").as_deref(), Some("tok-detail"));
}

// ───────── AC-43 / AC-44 / AC-47 — the assembled run detail page ─────────

fn detail_fixture(status: &str) -> views::RunDetail {
    let frames: Vec<(i32, Vec<Agent>)> = (0..6)
        .map(|t| {
            (
                t,
                vec![
                    agent(0, (10.0 + f64::from(t), 20.0), (1.0, 0.0)),
                    agent(1, (40.0 + f64::from(t), 50.0), (0.0, 1.0)),
                ],
            )
        })
        .collect();
    views::RunDetail {
        run: run_fixture(5, status),
        scenario_name: "Highway".to_owned(),
        world: World::new(200.0, 150.0),
        obstacles: vec![Obstacle {
            center: Vec2::new(80.0, 70.0),
            radius: 15.0,
        }],
        latest_agents: frames.last().map(|(_, a)| a.clone()).unwrap_or_default(),
        trajectory: frames,
        series: vec![
            metrics(0.2, 6.0, 0, 1.1),
            metrics(0.6, 5.0, 1, 1.5),
            metrics(0.9, 4.0, 2, 1.8),
        ],
        stuck: false,
        csrf: views::Csrf::absent(),
    }
}

#[test]
fn ac43_run_detail_page_assembles_flock_trajectory_metrics_and_provenance() {
    let html = views::run_detail_page(&detail_fixture(run_status::RUNNING)).into_string();

    assert_eq!(dom::count_class(&html, "flock"), 1, "AC-43: the flock");
    assert_eq!(
        dom::count_class(&html, "trajectories"),
        1,
        "AC-43: trajectory ribbons"
    );
    assert_eq!(dom::count_class(&html, "agent"), 2);
    assert_eq!(dom::count_class(&html, "obstacle"), 1);
    assert_eq!(dom::count_class(&html, "trajectory"), 2, "one per agent");
    assert_eq!(dom::count_class(&html, "sparkline"), 4, "AC-44");
    assert_eq!(dom::count_class(&html, "provenance"), 1, "AC-47");
    assert_eq!(dom::count_class(&html, "run-progress"), 1, "AC-45");
    assert!(html.contains("Highway"), "the page names its scenario");
    assert!(
        !html.contains("NaN"),
        "no NaN may reach the page:\n{}",
        &html[..html.len().min(2000)]
    );
}

#[test]
fn ac43_run_detail_page_renders_a_run_that_has_no_frames_yet() {
    let mut d = detail_fixture(run_status::QUEUED);
    d.latest_agents.clear();
    d.trajectory.clear();
    d.series.clear();

    let html = views::run_detail_page(&d).into_string();
    assert_eq!(
        dom::count_class(&html, "flock"),
        1,
        "an empty world, not a hole"
    );
    assert_eq!(dom::count_class(&html, "agent"), 0);
    assert_eq!(dom::count_class(&html, "sparkline"), 4);
    assert_eq!(dom::count_class(&html, "provenance"), 1);
    assert!(!html.contains("NaN"));
}

// ───────── AC-33 — cancel is reachable from the product, not just the API ─────────
//
// `POST /runs/{id}/cancel` was mounted, documented and covered by an
// integration test that posted to it directly — and had no button anywhere in
// the interface, so no user could ever reach it. These are the tests that would
// have caught that.

#[test]
fn ac33_a_live_run_offers_a_cancel_button_that_posts_to_the_cancel_route() {
    let html = views::run_detail_page(&detail_fixture(run_status::RUNNING)).into_string();

    let forms = dom::with_class(&html, "cancel-run");
    assert_eq!(
        forms.len(),
        1,
        "a running run must offer exactly one cancel control:\n{html}"
    );
    assert_eq!(
        dom::attr(forms[0], "method").as_deref(),
        Some("post"),
        "cancelling is a state change, so it must be a POST:\n{}",
        forms[0]
    );
    assert_eq!(
        dom::attr(forms[0], "action"),
        Some(boidboard::routes::paths::cancel_run(5)),
        "the form must target the route that actually cancels the run:\n{}",
        forms[0]
    );
    assert!(
        html.contains("Cancel run"),
        "the button must say what it does:\n{html}"
    );
}

#[test]
fn ac33_a_queued_run_can_be_cancelled_but_a_terminal_one_cannot() {
    for live in [run_status::QUEUED, run_status::RUNNING] {
        let html = views::run_detail_page(&detail_fixture(live)).into_string();
        assert_eq!(
            dom::count_class(&html, "cancel-run"),
            1,
            "`{live}` is not terminal, so the run is still cancellable:\n{html}"
        );
    }
    for done in [
        run_status::COMPLETED,
        run_status::CANCELLED,
        run_status::FAILED,
        run_status::BUDGET_EXCEEDED,
    ] {
        let html = views::run_detail_page(&detail_fixture(done)).into_string();
        assert_eq!(
            dom::count_class(&html, "cancel-run"),
            0,
            "`{done}` is terminal: offering a cancel button promises something \
             the route will not do:\n{html}"
        );
    }
}

// ───────── AC-27 — stuck detection, reachable from the product ─────────
//
// `boids_core::metrics::is_stuck` was correct and well tested and had no
// caller, so the application's answer to "does it detect stuck flocks?" was
// "only in a unit test". `analysis::run_is_stuck` is the caller, and it is pure
// — it takes already-loaded frames, so these tests need no database.

/// One stored frame whose `agents` column is in the **wire form the workflow
/// actually writes**: `SimState`'s own array of exact-decimal strings, not a
/// directly-serialized `Vec<Agent>`.
fn stored_frame(run_id: i64, tick: i32, agents: Vec<Agent>) -> boidboard::models::Frame {
    let state = boids_core::sim::SimState {
        tick: u32::try_from(tick).expect("non-negative fixture tick"),
        agents,
    };
    let mut wire = serde_json::to_value(&state).expect("SimState serializes");
    let agents = wire
        .get_mut("agents")
        .map(serde_json::Value::take)
        .expect("the wire form has an agents array");
    boidboard::models::Frame {
        id: i64::from(tick) + 1,
        run_id,
        tick,
        agents,
        state_hash: state.state_hash_hex(),
        metrics: serde_json::Value::Null,
    }
}

/// A run whose single agent walks steadily east: net displacement equals path
/// length, so straightness is 1 and the flock is plainly making progress.
fn cruising_frames(count: i32) -> Vec<boidboard::models::Frame> {
    (0..count)
        .map(|t| {
            let x = 10.0 + f64::from(t) * 2.0;
            stored_frame(1, t, vec![agent(0, (x, 40.0), (2.0, 0.0))])
        })
        .collect()
}

/// A run whose flock ping-pongs between two nearby points: it walks a long path
/// and ends up where it started, which is exactly the tortuosity `is_stuck`
/// measures.
fn oscillating_frames(count: i32) -> Vec<boidboard::models::Frame> {
    (0..count)
        .map(|t| {
            let x = if t % 2 == 0 { 100.0 } else { 106.0 };
            stored_frame(1, t, vec![agent(0, (x, 40.0), (6.0, 0.0))])
        })
        .collect()
}

#[test]
fn ac27_a_flock_that_keeps_making_progress_is_not_stuck() {
    let world = World::new(200.0, 150.0);
    let frames = cruising_frames(40);
    assert!(
        frames.len() > boidboard::analysis::STUCK_WINDOW,
        "the fixture must carry enough evidence to judge on"
    );
    assert!(
        !boidboard::analysis::run_is_stuck(&frames, &world),
        "a flock cruising in a straight line is the definition of not stuck"
    );
}

#[test]
fn ac27_a_flock_oscillating_in_place_is_reported_stuck() {
    let world = World::new(200.0, 150.0);
    assert!(
        boidboard::analysis::run_is_stuck(&oscillating_frames(40), &world),
        "a centroid that walks a long path back to where it started is stuck"
    );
}

#[test]
fn ac27_stuckness_is_never_claimed_on_absent_evidence() {
    let world = World::new(200.0, 150.0);
    assert!(
        !boidboard::analysis::run_is_stuck(&[], &world),
        "a run with no frames has not been shown to be stuck"
    );

    let short = i32::try_from(boidboard::analysis::STUCK_WINDOW).expect("small window") - 1;
    assert!(
        !boidboard::analysis::run_is_stuck(&oscillating_frames(short), &world),
        "fewer samples than the window is not enough to call a run stuck"
    );
}

#[test]
fn ac27_stuckness_is_read_from_the_wire_form_the_workflow_actually_writes() {
    let world = World::new(200.0, 150.0);
    let mut frames = oscillating_frames(40);

    // Every frame is unreadable now, so no centroid path can be built at all —
    // and an undecodable run must read as "no evidence", not as "stuck".
    for frame in &mut frames {
        frame.agents = serde_json::json!("not a flock");
    }
    assert!(
        !boidboard::analysis::run_is_stuck(&frames, &world),
        "frames that do not decode are missing evidence, not proof of stuckness"
    );
}

#[test]
fn ac27_the_run_detail_page_badges_a_stuck_run_and_only_a_stuck_run() {
    let mut d = detail_fixture(run_status::RUNNING);

    d.stuck = false;
    let html = views::run_detail_page(&d).into_string();
    assert_eq!(
        dom::count_class(&html, "stuck-badge"),
        0,
        "a run that is making progress must not be labelled stuck:\n{html}"
    );

    d.stuck = true;
    let html = views::run_detail_page(&d).into_string();
    assert_eq!(
        dom::count_class(&html, "stuck-badge"),
        1,
        "AC-27: a stuck run must say so on its own page:\n{html}"
    );
    assert!(
        html.to_lowercase().contains("stuck"),
        "the badge must be readable, not just selectable:\n{html}"
    );
}

// ═══════════════════════ route layer (AC-48: TestApp + selectors) ═══════════════════════

use autumn_web::prelude::routes;
use autumn_web::test::{TestApp, TestClient};
use boidboard::models::{NewFrame, NewRun, NewScenario, Scenario};
use boidboard::repositories::{
    FrameRepository as _, PgFrameRepository, PgRunRepository, PgScenarioRepository,
    RunRepository as _, ScenarioRepository as _,
};
use boidboard::routes as app_routes;

const TEST_DB_URL: &str = "postgres://boid:boid@127.0.0.1:5432/boidboard_test";

type TestPool = diesel_async::pooled_connection::deadpool::Pool<diesel_async::AsyncPgConnection>;

/// Apply the embedded migrations once per test binary. Idempotent — Diesel
/// tracks applied versions — which it has to be, because this commits while
/// every test's own writes stay inside a transaction that is rolled back.
fn migrate_once() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        autumn_web::migrate::run_pending(TEST_DB_URL, boidboard::MIGRATIONS)
            .expect("embedded migrations apply to the test database");
    });
}

fn all_routes() -> Vec<autumn_web::Route> {
    routes![
        app_routes::run_list,
        app_routes::new_run_form,
        app_routes::create_run,
        app_routes::run_detail,
        app_routes::run_progress,
        app_routes::compare
    ]
}

/// A transactionally-isolated client with every Boidboard route mounted.
fn web_client() -> TestClient {
    migrate_once();
    TestApp::new()
        .routes(all_routes())
        .with_transactional_db(TEST_DB_URL)
        .build()
}

async fn seed_scenario(pool: &TestPool, name: &str, config: &serde_json::Value) -> Scenario {
    PgScenarioRepository::with_pool_untracked(pool.clone())
        .save(&NewScenario {
            name: name.to_owned(),
            config: config.clone(),
            config_hash: boidboard::models::canonical_config_hash(config),
        })
        .await
        .expect("scenario saves")
}

#[allow(clippy::too_many_arguments)]
async fn seed_run(
    pool: &TestPool,
    scenario: &Scenario,
    status: &str,
    ticks_completed: i32,
    final_state_hash: Option<&str>,
    frames: &[(i32, Vec<Agent>, FrameMetrics)],
) -> Run {
    let run = PgRunRepository::with_pool_untracked(pool.clone())
        .save(&NewRun {
            scenario_id: scenario.id,
            seed: 4242,
            status: status.to_owned(),
            max_ticks: 300,
            ticks_completed,
            config_snapshot: scenario.config.clone(),
            config_hash: scenario.config_hash.clone(),
            kernel_version: boidboard::KERNEL_VERSION.to_owned(),
            final_state_hash: final_state_hash.map(str::to_owned),
            error: None,
            workflow_execution_id: None,
        })
        .await
        .expect("run saves");

    let frame_repo = PgFrameRepository::with_pool_untracked(pool.clone());
    for (tick, agents, m) in frames {
        frame_repo
            .save(&NewFrame {
                run_id: run.id,
                tick: *tick,
                agents: serde_json::to_value(agents).expect("agents serialize"),
                state_hash: format!("state-{tick}"),
                metrics: serde_json::to_value(m).expect("metrics serialize"),
            })
            .await
            .expect("frame saves");
    }
    run
}

/// A short run in the `classic-flock` world (400 × 300): agent 0 walks east
/// and wraps the x seam once (380, 385, 390, 395, 0, 5) while agent 1 stays
/// well inside the world — so exactly one of the two ribbons must be broken.
fn sample_frames() -> Vec<(i32, Vec<Agent>, FrameMetrics)> {
    (0..6)
        .map(|t| {
            let wrapping_x = (380.0 + f64::from(t) * 5.0) % 400.0;
            (
                t,
                vec![
                    agent(0, (wrapping_x, 50.0), (1.0, 0.0)),
                    agent(1, (40.0 + f64::from(t) * 2.0, 90.0), (0.0, 1.0)),
                ],
                metrics(
                    0.1 * f64::from(t),
                    5.0 - f64::from(t) * 0.2,
                    t as usize,
                    1.5,
                ),
            )
        })
        .collect()
}

fn sample_config() -> serde_json::Value {
    serde_json::to_value(presets::by_slug("classic-flock").expect("preset").params)
        .expect("config serializes")
}

// ───────────────────────────── AC-41 (route) ─────────────────────────────

#[tokio::test]
async fn ac41_run_list_route_renders_a_row_per_run_with_status_and_metrics() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let scatter = seed_scenario(&pool, "Scatter", &serde_json::json!({ "w_cohesion": 0.02 })).await;
    seed_run(&pool, &scatter, run_status::QUEUED, 0, None, &[]).await;
    let classic = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let newest = seed_run(
        &pool,
        &classic,
        run_status::RUNNING,
        5,
        None,
        &sample_frames(),
    )
    .await;

    client
        .get("/runs")
        .send()
        .await
        .assert_ok()
        .assert_selector("table.runs")
        .assert_selector_count("tbody tr.run-row", 2)
        .assert_text("tr.run-row a", "Classic Flock")
        .assert_attr("tr.run-row a", "href", &format!("/runs/{}", newest.id))
        .assert_selector(r#"tr.run-row .status-badge[data-status="running"]"#)
        .assert_selector(r#"tr.run-row .status-badge[data-status="queued"]"#)
        // Headline metrics come from the run's most recent frame: tick 5 has
        // polarization 0.5 and 5 collisions.
        .assert_text("tr.run-row .metric-polarization", "0.500")
        .assert_text("tr.run-row .metric-collisions", "5");
}

#[tokio::test]
async fn ac41_run_list_route_renders_an_empty_state_rather_than_a_blank_page() {
    web_client()
        .get("/runs")
        .send()
        .await
        .assert_ok()
        .assert_selector("table.runs")
        .assert_no_selector("tr.run-row")
        .assert_selector("td.empty");
}

// ───────────────────────────── AC-42 (route) ─────────────────────────────

#[tokio::test]
async fn ac42_new_run_route_is_fronted_by_preset_cards() {
    // No database at all: the form is presets and nothing else.
    let client = TestApp::new()
        .routes(routes![app_routes::new_run_form])
        .build();

    client
        .get("/runs/new")
        .send()
        .await
        .assert_ok()
        .assert_selector(r#"form[action="/runs"][method="post"]"#)
        .assert_selector_count("li.preset-card", presets::all().len())
        .assert_selector(r#"li.preset-card[data-slug="classic-flock"]"#)
        .assert_selector(r#"li.preset-card[data-slug="nervous-swarm"]"#)
        .assert_selector(r#"li.preset-card[data-slug="highway"]"#)
        .assert_selector(r#"li.preset-card[data-slug="scatter"]"#)
        .assert_selector_count(
            r#"input[type="radio"][name="preset"]"#,
            presets::all().len(),
        );
}

/// The route table with CSRF **actually switched on**, which is the
/// configuration `prod` runs and the one the hand-written forms were broken
/// under: every POST was a 403 because no form emitted a `_csrf` field.
fn csrf_web_client() -> TestClient {
    migrate_once();
    TestApp::new()
        .routes(all_routes())
        .with_transactional_db(TEST_DB_URL)
        .layer(autumn_web::security::CsrfLayer::from_config(
            &autumn_web::security::CsrfConfig {
                enabled: true,
                ..autumn_web::security::CsrfConfig::default()
            },
        ))
        .build()
}

#[tokio::test]
async fn csrf_the_rendered_new_run_form_submits_successfully_under_an_enabled_csrf_layer() {
    let client = csrf_web_client();

    let page = client.get("/runs/new").send().await;
    page.assert_ok()
        .assert_selector(r#"form.new-run input[type="hidden"][name="_csrf"]"#);
    let token = csrf_input(&page.text(), "_csrf")
        .and_then(|input| dom::attr(input, "value"))
        .expect("the form carries a token");

    // The exact submission the rendered form produces — plus the cookie the
    // same response set, which `TestClient`'s jar replays automatically.
    let created = client
        .post("/runs")
        .form(&format!("preset=nervous-swarm&_csrf={token}"))
        .send()
        .await;
    assert_eq!(
        created.status,
        303,
        "the form as rendered must be accepted, not rejected as a forgery: {}",
        created.text()
    );

    // And the protection is real rather than absent: the same POST without the
    // field the form emits is refused.
    client
        .post("/runs")
        .form("preset=nervous-swarm")
        .send()
        .await
        .assert_status(403);
}

#[tokio::test]
async fn ac42_posting_a_preset_slug_creates_a_run_carrying_that_presets_config() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let response = client
        .post("/runs")
        .form("preset=nervous-swarm&seed=99&max_ticks=25")
        .send()
        .await;
    response.assert_status(303);
    let location = response.header("location").expect("a redirect target");
    assert!(
        location.starts_with("/runs/"),
        "a created run must redirect to its own page, got `{location}`"
    );

    let id: i64 = location
        .trim_start_matches("/runs/")
        .parse()
        .expect("the redirect names the new run");
    let run = PgRunRepository::with_pool_untracked(pool.clone())
        .find_by_id(id)
        .await
        .expect("run lookup")
        .expect("the run exists");

    let preset = presets::by_slug("nervous-swarm").expect("preset");
    let expected = serde_json::to_value(&preset.params).expect("params serialize");
    assert_eq!(
        run.config_snapshot, expected,
        "AC-39/AC-42: the run must snapshot the preset's config, \
         not merely point at a scenario row"
    );
    assert_eq!(
        run.config_hash,
        boidboard::models::canonical_config_hash(&expected)
    );
    assert_eq!(run.seed, 99);
    assert_eq!(run.max_ticks, 25);
    assert_eq!(run.status, run_status::QUEUED);
    assert_eq!(run.ticks_completed, 0);
    assert_eq!(run.kernel_version, boidboard::KERNEL_VERSION);

    let scenario = PgScenarioRepository::with_pool_untracked(pool)
        .find_by_id(run.scenario_id)
        .await
        .expect("scenario lookup")
        .expect("the scenario exists");
    assert_eq!(scenario.name, preset.name);
}

#[tokio::test]
async fn ac42_posting_a_preset_slug_alone_uses_the_offered_defaults() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let response = client.post("/runs").form("preset=highway").send().await;
    response.assert_status(303);
    let id: i64 = response
        .header("location")
        .expect("redirect")
        .trim_start_matches("/runs/")
        .parse()
        .expect("run id");

    let run = PgRunRepository::with_pool_untracked(pool)
        .find_by_id(id)
        .await
        .expect("lookup")
        .expect("run exists");
    assert_eq!(run.seed, boidboard::views::DEFAULT_SEED);
    assert_eq!(run.max_ticks, boidboard::views::DEFAULT_MAX_TICKS);
}

#[tokio::test]
async fn ac42_posting_an_unknown_preset_is_rejected_rather_than_silently_substituted() {
    let client = web_client();
    client
        .post("/runs")
        .form("preset=not-a-preset")
        .send()
        .await
        .assert_status(400);

    assert_eq!(
        PgRunRepository::with_pool_untracked(client.state().pool().expect("pool").clone())
            .count()
            .await
            .expect("count"),
        0,
        "a rejected submission must not leave a run behind"
    );
}

// ─────────────────────── AC-43 / AC-44 / AC-47 (route) ───────────────────────

#[tokio::test]
async fn ac43_run_detail_route_renders_inline_svg_with_oriented_marks_and_ribbons() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let run = seed_run(
        &pool,
        &scenario,
        run_status::RUNNING,
        5,
        None,
        &sample_frames(),
    )
    .await;

    let response = client.get(&format!("/runs/{}", run.id)).send().await;
    response
        .assert_ok()
        .assert_selector("svg.flock")
        .assert_selector_count("svg.flock polygon.agent", 2)
        .assert_selector("svg.flock rect.world-bounds")
        .assert_selector("svg.trajectories")
        .assert_selector_count("svg.trajectories g.trajectory", 2)
        .assert_selector("svg.trajectories g.trajectory polyline.trail")
        .assert_no_selector("canvas")
        .assert_no_selector("iframe");

    for transform in response.selector_attr("polygon.agent", "transform") {
        let t = transform.expect("every agent mark is placed and oriented");
        assert!(
            t.contains("rotate("),
            "AC-43: agent marks must be *oriented*, got `{t}`"
        );
    }

    // AC-43: no SPA framework, no WASM — every script is same-origin and local.
    for src in response
        .selector_attr("script", "src")
        .into_iter()
        .flatten()
    {
        assert!(
            src.starts_with('/') && !src.starts_with("//"),
            "no script may come from an external host: `{src}`"
        );
        assert!(!src.ends_with(".wasm"), "no WASM: `{src}`");
    }
    response.assert_selector(r#"script[src="/static/js/htmx.min.js"]"#);
}

#[tokio::test]
async fn ac43_trajectory_ribbons_are_broken_at_the_seam_on_the_rendered_page() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    // Agent 0 wraps the x seam between tick 1 (x=195) and tick 2 (x=0).
    let run = seed_run(
        &pool,
        &scenario,
        run_status::COMPLETED,
        5,
        Some("final-hash"),
        &sample_frames(),
    )
    .await;

    client
        .get(&format!("/runs/{}", run.id))
        .send()
        .await
        .assert_ok()
        .assert_selector_count(r#"g.trajectory[data-agent-id="0"] polyline.trail"#, 2)
        .assert_selector_count(r#"g.trajectory[data-agent-id="1"] polyline.trail"#, 1);
}

#[tokio::test]
async fn ac44_run_detail_route_renders_one_sparkline_per_headline_metric() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let run = seed_run(
        &pool,
        &scenario,
        run_status::RUNNING,
        5,
        None,
        &sample_frames(),
    )
    .await;

    client
        .get(&format!("/runs/{}", run.id))
        .send()
        .await
        .assert_ok()
        .assert_selector_count("svg.sparkline", 4)
        .assert_selector(r#"figure.metric-card[data-metric="polarization"] svg.sparkline"#)
        .assert_selector(
            r#"figure.metric-card[data-metric="mean_nearest_neighbor_distance"] svg.sparkline"#,
        )
        .assert_selector(r#"figure.metric-card[data-metric="collisions"] svg.sparkline"#)
        .assert_selector(r#"figure.metric-card[data-metric="mean_speed"] svg.sparkline"#)
        .assert_no_selector("svg.sparkline-empty");
}

#[tokio::test]
async fn ac44_run_detail_route_survives_a_run_with_no_frames_at_all() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let run = seed_run(&pool, &scenario, run_status::QUEUED, 0, None, &[]).await;

    client
        .get(&format!("/runs/{}", run.id))
        .send()
        .await
        .assert_ok()
        .assert_selector("svg.flock")
        .assert_no_selector("polygon.agent")
        .assert_selector_count("svg.sparkline", 4)
        .assert_selector_count("svg.sparkline-empty", 4)
        .assert_selector("section.provenance");
}

#[tokio::test]
async fn ac47_run_detail_route_surfaces_the_reproducibility_hash() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let run = seed_run(
        &pool,
        &scenario,
        run_status::COMPLETED,
        6,
        Some("deadbeefcafe"),
        &sample_frames(),
    )
    .await;

    client
        .get(&format!("/runs/{}", run.id))
        .send()
        .await
        .assert_ok()
        .assert_selector("section.provenance")
        .assert_text(".prov-final-state-hash", "deadbeefcafe")
        .assert_text(".prov-config-hash", &scenario.config_hash)
        .assert_text(".prov-seed", "4242")
        .assert_text(".prov-kernel-version", boidboard::KERNEL_VERSION);
}

#[tokio::test]
async fn ac33_the_rendered_detail_page_offers_a_cancel_button_only_while_the_run_is_live() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;

    let live = seed_run(
        &pool,
        &scenario,
        run_status::RUNNING,
        6,
        None,
        &sample_frames(),
    )
    .await;
    client
        .get(&format!("/runs/{}", live.id))
        .send()
        .await
        .assert_ok()
        .assert_selector(&format!(
            r#"form.cancel-run[method="post"][action="/runs/{}/cancel"]"#,
            live.id
        ))
        .assert_selector("form.cancel-run button[type=\"submit\"]");

    let done = seed_run(
        &pool,
        &scenario,
        run_status::COMPLETED,
        6,
        Some("deadbeefcafe"),
        &sample_frames(),
    )
    .await;
    client
        .get(&format!("/runs/{}", done.id))
        .send()
        .await
        .assert_ok()
        .assert_no_selector("form.cancel-run");
}

#[tokio::test]
async fn run_detail_route_404s_for_a_run_that_does_not_exist() {
    web_client()
        .get("/runs/987654321")
        .send()
        .await
        .assert_status(404);
}

// ───────────────────────────── AC-45 (route) ─────────────────────────────

#[tokio::test]
async fn ac45_progress_endpoint_returns_the_fragment_and_nothing_else() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let run = seed_run(
        &pool,
        &scenario,
        run_status::RUNNING,
        5,
        None,
        &sample_frames(),
    )
    .await;

    client
        .get(&format!("/runs/{}/progress", run.id))
        .send()
        .await
        .assert_ok()
        // It is a *fragment*: no document chrome, so htmx can swap it in place.
        .assert_no_selector("html")
        .assert_no_selector("head")
        .assert_no_selector("body")
        .assert_no_selector("svg.flock")
        .assert_selector_count("#run-progress", 1)
        .assert_attr(
            "#run-progress",
            "hx-get",
            &format!("/runs/{}/progress", run.id),
        )
        .assert_attr("#run-progress", "hx-trigger", "every 2s")
        .assert_selector(r#".status-badge[data-status="running"]"#);
}

#[tokio::test]
async fn ac45_the_page_polls_while_running_and_stops_once_terminal() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let running = seed_run(&pool, &scenario, run_status::RUNNING, 5, None, &[]).await;
    let done = seed_run(
        &pool,
        &scenario,
        run_status::COMPLETED,
        300,
        Some("final"),
        &[],
    )
    .await;

    client
        .get(&format!("/runs/{}", running.id))
        .send()
        .await
        .assert_ok()
        .assert_selector(r#"#run-progress[hx-trigger="every 2s"]"#)
        .assert_attr(
            "#run-progress",
            "hx-get",
            &format!("/runs/{}/progress", running.id),
        );

    client
        .get(&format!("/runs/{}", done.id))
        .send()
        .await
        .assert_ok()
        .assert_selector("#run-progress")
        .assert_no_selector("#run-progress[hx-trigger]")
        .assert_no_selector("#run-progress[hx-get]");

    // …and the fragment endpoint agrees, so the final swap is what stops it.
    client
        .get(&format!("/runs/{}/progress", done.id))
        .send()
        .await
        .assert_ok()
        .assert_no_selector("[hx-trigger]");
}

// ───────────────────────────── AC-46 (route) ─────────────────────────────

#[tokio::test]
async fn ac46_compare_route_renders_both_runs_and_the_config_difference() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();

    let mut config_a = sample_config();
    config_a["w_cohesion"] = serde_json::json!(1.0);
    let mut config_b = sample_config();
    config_b["w_cohesion"] = serde_json::json!(0.02);

    let sa = seed_scenario(&pool, "Classic Flock", &config_a).await;
    let sb = seed_scenario(&pool, "Scatter", &config_b).await;
    let a = seed_run(
        &pool,
        &sa,
        run_status::COMPLETED,
        5,
        Some("hash-a"),
        &sample_frames(),
    )
    .await;
    let b = seed_run(
        &pool,
        &sb,
        run_status::COMPLETED,
        5,
        Some("hash-b"),
        &sample_frames(),
    )
    .await;

    client
        .get(&format!("/compare?a={}&b={}", a.id, b.id))
        .send()
        .await
        .assert_ok()
        .assert_selector("section.compare")
        .assert_selector_count(".compare-side", 2)
        .assert_selector_count("svg.flock", 2)
        .assert_selector_count("section.provenance", 2)
        .assert_selector_count("tr.config-diff-row", 1)
        .assert_attr("tr.config-diff-row", "data-field", "w_cohesion")
        .assert_text("tr.config-diff-row .diff-a", "1.0")
        .assert_text("tr.config-diff-row .diff-b", "0.02")
        .assert_text(".compare-side .prov-final-state-hash", "hash-a");
}

#[tokio::test]
async fn ac46_compare_route_404s_when_a_run_is_missing() {
    let client = web_client();
    let pool = client.state().pool().expect("transactional pool").clone();
    let scenario = seed_scenario(&pool, "Classic Flock", &sample_config()).await;
    let a = seed_run(&pool, &scenario, run_status::COMPLETED, 5, None, &[]).await;

    client
        .get(&format!("/compare?a={}&b=987654321", a.id))
        .send()
        .await
        .assert_status(404);
}

// ────────── AC-46 — getting *to* the compare view from a run detail page ──────────

#[test]
fn ac46_detail_page_offers_a_compare_form_prefilled_with_this_run() {
    let html = views::run_detail_page(&detail_fixture(run_status::COMPLETED)).into_string();
    let form = *dom::start_tags(&html)
        .iter()
        .find(|t| t.starts_with("<form") && dom::attr(t, "class").as_deref() == Some("compare-cta"))
        .expect("a compare form on the detail page");

    assert_eq!(dom::attr(form, "method").as_deref(), Some("get"));
    assert_eq!(dom::attr(form, "action").as_deref(), Some("/compare"));

    let hidden = dom::start_tags(&html)
        .into_iter()
        .find(|t| t.starts_with("<input") && dom::attr(t, "name").as_deref() == Some("a"))
        .expect("the current run is prefilled as side `a`");
    assert_eq!(dom::attr(hidden, "type").as_deref(), Some("hidden"));
    assert_eq!(
        dom::attr(hidden, "value").as_deref(),
        Some("5"),
        "the form must not make the user retype the run they are looking at"
    );

    assert!(
        dom::start_tags(&html)
            .into_iter()
            .any(|t| t.starts_with("<input") && dom::attr(t, "name").as_deref() == Some("b")),
        "…and must ask for the other run:\n{html}"
    );
}

// ───────────── AC-50 — no domain logic in handlers, views stay pure ─────────────

/// Every `views::` test above is already an AC-50 assertion: they are plain
/// `#[test]`s with no `TestApp`, no runtime and no connection, so a view that
/// reached for the database could not appear in that layer at all. This is the
/// guard that keeps the property from eroding in the other direction — a
/// handler quietly growing a `html!` block, or a view quietly growing an
/// `.await`.
#[test]
fn ac50_handlers_do_no_rendering_and_views_do_no_io() {
    const ROUTES_SRC: &str = include_str!("../src/routes.rs");
    const ANALYSIS_SRC: &str = include_str!("../src/analysis.rs");
    // `views` is a directory now; the purity rule applies to every file in it,
    // so the guard reads all three rather than whichever one it was written
    // against. A fourth file added without being listed here is caught by the
    // `views/mod.rs` check below.
    const VIEWS_MOD_SRC: &str = include_str!("../src/views/mod.rs");
    const VIEWS_SRC: &str = concat!(
        include_str!("../src/views/mod.rs"),
        include_str!("../src/views/pages.rs"),
        include_str!("../src/views/svg.rs"),
        include_str!("../src/views/style.rs"),
    );

    for declared in ["pub mod pages;", "pub mod style;", "pub mod svg;"] {
        assert!(
            VIEWS_MOD_SRC.contains(declared),
            "`views/mod.rs` must declare exactly the files this guard reads — \
             `{declared}` is missing, so a view file may be escaping the purity check"
        );
    }
    assert_eq!(
        VIEWS_MOD_SRC.matches("pub mod ").count(),
        3,
        "a new `views/` submodule must be added to the AC-50 purity guard too"
    );

    // `analysis.rs` is held to the same purity rule as `views.rs` and for the
    // same reason: it answers a question *about* loaded data, so it must be
    // callable — and testable — with a fixture and no infrastructure. The moment
    // it grows a query, "is this run stuck?" stops being a unit test.
    for forbidden in [".await", "diesel", "AsyncPgConnection", "AutumnResult"] {
        assert!(
            !ANALYSIS_SRC.contains(forbidden),
            "AC-50: `analysis.rs` must stay pure `fn(data) -> answer` — found `{forbidden}`"
        );
    }

    for forbidden in [
        "html!",
        "<svg",
        "viewBox",
        "boids_core::sim",
        "frame_metrics(",
    ] {
        assert!(
            !ROUTES_SRC.contains(forbidden),
            "AC-50: `routes.rs` must delegate, not render or simulate — found `{forbidden}`"
        );
    }
    for forbidden in [
        ".await",
        "diesel",
        "AsyncPgConnection",
        "PgRunRepository",
        "AutumnResult",
    ] {
        assert!(
            !VIEWS_SRC.contains(forbidden),
            "AC-50: `views.rs` must be pure `fn(data) -> Markup` — found `{forbidden}`"
        );
    }

    // And the positive half: every public view is reachable from plain data.
    let world = World::new(120.0, 90.0);
    let run = run_fixture(3, run_status::RUNNING);
    let agents = [agent(0, (1.0, 2.0), (1.0, 1.0))];
    let series = [metrics(0.4, 3.0, 1, 1.2)];

    let rendered = [
        views::layout("t", views::runs_table(&[])).into_string(),
        views::status_badge(run_status::QUEUED).into_string(),
        views::flock_svg(&agents, &world, &[], views::FlockOpts::default()).into_string(),
        views::trajectory_svg(
            &[(0, agents.to_vec())],
            &world,
            views::TrajectoryOpts::default(),
        )
        .into_string(),
        views::sparkline(&[1.0, 2.0], views::SparkOpts::default()).into_string(),
        views::metrics_panel(&series).into_string(),
        views::provenance_panel(&run).into_string(),
        views::progress_fragment(&run).into_string(),
        views::new_run_form(&presets::all(), &views::Csrf::absent()).into_string(),
        views::cancel_form(run.id, &views::Csrf::new("_csrf", "t")).into_string(),
        views::stuck_badge().into_string(),
        views::run_row(&views::RunSummary {
            run: run.clone(),
            scenario_name: "s".to_owned(),
            headline: Some(series[0]),
        })
        .into_string(),
        views::compare_view((&run, &agents), (&run, &agents), &world).into_string(),
        views::run_detail_page(&detail_fixture(run_status::QUEUED)).into_string(),
    ];
    for html in &rendered {
        assert!(!html.is_empty(), "every view must render something");
        assert!(!html.contains("NaN"), "no view may emit NaN:\n{html}");
    }
}
