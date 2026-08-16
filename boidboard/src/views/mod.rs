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
//!
//! # Layout of this module
//!
//! Split along the seam the file's own section dividers already showed, once it
//! passed a thousand lines:
//!
//! * [`svg`] — the geometry. `num`, `fit`, `flock_svg`, `agent_mark`,
//!   `trajectory_svg`, `seam_split`, `sparkline`, `points_attr`, and the
//!   rendering budgets.
//! * [`pages`] — the structure. Layout, tables, panels, forms, fragments.
//! * [`style`] — the embedded stylesheet, which is CSS and not Rust.
//!
//! Everything public is re-exported here, so `views::flock_svg` and
//! `views::run_detail_page` are spelled exactly as they were and no caller —
//! handler or test — has to know which half a view lives in.

pub mod pages;
pub mod style;
pub mod svg;

pub use pages::{
    ConfigDiff, Csrf, DEFAULT_MAX_TICKS, DEFAULT_SEED, RunDetail, RunSummary, cancel_form,
    compare_view, config_diff, layout, metrics_panel, new_run_form, progress_fragment,
    provenance_panel, run_detail_page, run_row, runs_table, status_badge, stuck_badge,
};
pub use svg::{FlockOpts, SparkOpts, TrajectoryOpts, flock_svg, sparkline, trajectory_svg};
