//! Post-hoc analysis of a stored run.
//!
//! **Pure, and deliberately so.** Every function here takes rows that have
//! already been loaded and returns a plain answer — no connection, no pool,
//! nothing async at all. That is what lets the interesting question ("is this
//! flock stuck?")
//! be unit tested with a hand-built fixture and no database at all, exactly as
//! `views` is (AC-50), while leaving the handler with nothing to do but fetch
//! and pass the answer on.
//!
//! The module exists because [`boids_core::metrics::is_stuck`] was correct,
//! well-tested and **had no caller**: the kernel could detect a stuck flock and
//! the product could not tell you about one (**AC-27**). This is the seam
//! between the two.

use boids_core::metrics::{is_stuck, toroidal_centroid};
use boids_core::sim::SimState;
use boids_core::{Agent, Vec2, World};
use serde_json::Value;

use crate::models::Frame;

/// How many consecutive stored frames the stuck test looks back over.
///
/// The judgement is a *tortuosity over a window*, so the window is the whole
/// question: too short and a flock that turns a corner reads as trapped, too
/// long and a run that genuinely wedged itself takes forever to say so. Twenty
/// samples of the frame budget the detail page loads
/// (`routes::DETAIL_FRAME_BUDGET`, currently 120, spread evenly across the run)
/// is about the last sixth of a run — long enough that a single manoeuvre
/// cannot dominate it, short enough that a run stuck since halfway is reported
/// while it is still running.
pub const STUCK_WINDOW: usize = 20;

/// Straightness below which the flock counts as stuck.
///
/// Straightness is `net displacement / path length` in `[0,1]`: `1.0` is a
/// dead-straight run and `0.0` is a flock that walked a long way to end up
/// where it started. `0.2` is the middle of the useful `0.1`–`0.3` band
/// [`is_stuck`] documents — it tolerates a flock milling as it turns, and
/// catches one orbiting an obstacle it cannot get past.
pub const STUCK_STRAIGHTNESS: f64 = 0.2;

/// True when the flock's centroid has stopped making progress (**AC-27**).
///
/// Pure — takes already-loaded frames so it can be tested with no database.
///
/// Builds the centroid path with [`toroidal_centroid`] over the decoded frames
/// and hands it to [`is_stuck`], which measures **tortuosity**: net displacement
/// against total path walked, both under the minimum-image convention, so a
/// flock cruising once round the torus counts as the progress it is rather than
/// as a return to the start.
///
/// `frames` is expected in ascending tick order — which is what
/// [`frames_for_run`](crate::repositories::frames_for_run) returns — because the
/// path is walked in slice order. Frames that do not decode are **skipped**, not
/// substituted with an empty flock: a run whose frames are unreadable has
/// missing evidence, and `is_stuck` answering `false` on a path too short to
/// judge is the same "never claim stuckness on absent evidence" rule the kernel
/// already applies.
///
/// Note that the centroid is the kernel's one piece of non-bit-portable
/// arithmetic (it is the only function that uses trigonometry), which is exactly
/// why the answer is a **displayed badge** and is never hashed, persisted or
/// compared across machines.
#[must_use]
pub fn run_is_stuck(frames: &[Frame], world: &World) -> bool {
    let path = centroid_path(frames, world);
    is_stuck(&path, world, STUCK_WINDOW, STUCK_STRAIGHTNESS)
}

/// The centroid of every decodable frame, in the order the frames arrive.
///
/// Split out from [`run_is_stuck`] so the path itself is inspectable: "the
/// stuck test said no" and "the stuck test had two points to work with" are
/// different findings, and only the second is a bug.
#[must_use]
pub fn centroid_path(frames: &[Frame], world: &World) -> Vec<Vec2> {
    frames
        .iter()
        .filter_map(|frame| decode_agents(&frame.agents))
        .map(|agents| toroidal_centroid(&agents, world))
        .collect()
}

/// Decode a stored frame's `agents` column back into a flock.
///
/// # Why this reads `SimState`'s wire form first
///
/// `simulate_batch` does not store a serialized `Vec<Agent>`. It stores
/// [`SimState`]'s *own* array — flat records whose coordinates are exact decimal
/// **strings** — because a JSON float can come back one ULP away from what was
/// written and one ULP changes the canonical state hash. Reading such a frame as
/// `Vec<Agent>` yields an empty flock, silently, because the field names differ
/// (`px`/`py` rather than `pos`).
///
/// The direct `Vec<Agent>` form is accepted as a fallback: the column is opaque
/// `JSONB` by design and may hold a frame written by hand or by an older kernel.
///
/// Returns `None` — rather than an empty flock — when neither form parses, so a
/// caller can tell "this frame has no agents" from "this frame is unreadable".
/// [`centroid_path`] leans on that distinction: an unreadable frame drops out of
/// the path instead of contributing a centroid of `(0,0)` that would fake a huge
/// jump and make a healthy run look stuck.
#[must_use]
pub fn decode_agents(value: &Value) -> Option<Vec<Agent>> {
    serde_json::from_value::<SimState>(serde_json::json!({ "tick": 0, "agents": value }))
        .map(|state| state.agents)
        .or_else(|_| serde_json::from_value::<Vec<Agent>>(value.clone()))
        .ok()
}
