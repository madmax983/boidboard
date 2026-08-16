//! Connection-level frame queries used by the simulation workflow's activities.
//!
//! These take `&mut AsyncPgConnection` rather than a pool: an activity already
//! holds a connection, usually needs several operations inside one transaction,
//! and taking the connection is what makes these directly unit testable.

use autumn_web::AutumnResult;
// Deliberately not `diesel::prelude::*`: that pulls in the *synchronous*
// `RunQueryDsl`, which collides with `diesel_async`'s on every `.load`/
// `.execute` call.
use diesel::{ExpressionMethods as _, QueryDsl as _, SelectableHelper as _};
use diesel_async::{AsyncPgConnection, RunQueryDsl as _};

use crate::models::{Frame, NewFrame};
use crate::schema::frames;

/// Insert frames idempotently.
///
/// Re-inserting a `(run_id, tick)` that already exists is a **no-op** — not an
/// error and not a duplicate — implemented with `ON CONFLICT (run_id, tick) DO
/// NOTHING`. This is what makes at-least-once activity retries safe by
/// construction (AC-32): the activity can be delivered any number of times and
/// the frame table converges to the same content.
///
/// Existing rows are left exactly as they were; a conflicting frame does not
/// overwrite. `run_id` is applied to every frame, so a caller cannot smear one
/// batch across two runs by mistake.
///
/// Returns the number of rows **actually** inserted.
///
/// # Errors
/// Returns an error if the insert fails (for example the run does not exist).
pub async fn insert_frames_idempotent(
    conn: &mut AsyncPgConnection,
    run_id: i64,
    new_frames: &[NewFrame],
) -> AutumnResult<usize> {
    if new_frames.is_empty() {
        return Ok(0);
    }

    let rows: Vec<NewFrame> = new_frames
        .iter()
        .map(|f| NewFrame {
            run_id,
            ..f.clone()
        })
        .collect();

    let inserted = diesel::insert_into(frames::table)
        .values(&rows)
        .on_conflict((frames::run_id, frames::tick))
        .do_nothing()
        .execute(conn)
        .await?;

    Ok(inserted)
}

/// The highest tick stored for a run, or `None` when it has no frames yet.
///
/// This is the resume cursor: a workflow restarting after a crash asks for it
/// and continues from `max_tick + 1` (AC-36).
///
/// # Errors
/// Returns an error if the query fails.
pub async fn max_tick(conn: &mut AsyncPgConnection, run_id: i64) -> AutumnResult<Option<i32>> {
    let highest = frames::table
        .filter(frames::run_id.eq(run_id))
        .select(diesel::dsl::max(frames::tick))
        .first::<Option<i32>>(conn)
        .await?;
    Ok(highest)
}

/// Frames for a run in ascending tick order.
///
/// With `limit = Some(n)` the result is subsampled to **at most** `n` evenly
/// spaced frames, always including the first and last so a trajectory ribbon
/// still spans the whole run. That is the SVG rendering budget: a 10 000-tick
/// run must not put 10 000 marks on a page.
///
/// `limit = None` returns every frame; `limit = Some(0)` returns nothing.
///
/// # Errors
/// Returns an error if the query fails.
pub async fn frames_for_run(
    conn: &mut AsyncPgConnection,
    run_id: i64,
    limit: Option<usize>,
) -> AutumnResult<Vec<Frame>> {
    let all: Vec<Frame> = frames::table
        .filter(frames::run_id.eq(run_id))
        .order(frames::tick.asc())
        .select(Frame::as_select())
        .load(conn)
        .await?;

    Ok(match limit {
        None => all,
        Some(n) => subsample(all, n),
    })
}

/// Take at most `n` evenly spaced items, always including the first and last.
fn subsample<T>(items: Vec<T>, n: usize) -> Vec<T> {
    if n == 0 {
        return Vec::new();
    }
    if n >= items.len() {
        return items;
    }
    if n == 1 {
        return items.into_iter().take(1).collect();
    }

    // Slot s maps to round(s * (len - 1) / (n - 1)), pinning slot 0 to the
    // first item and slot n-1 to the last. Ascending and distinct because
    // n < len, so a single forward pass picks them up.
    let last = items.len() - 1;
    let divisor = n - 1;
    let mut wanted = (0..n).map(|slot| (2 * slot * last + divisor) / (2 * divisor));

    let mut next_wanted = wanted.next();
    let mut kept: Vec<T> = Vec::with_capacity(n);
    for (i, item) in items.into_iter().enumerate() {
        if next_wanted == Some(i) {
            kept.push(item);
            next_wanted = wanted.next();
        }
    }
    kept
}

/// Ticks missing from a run's frame sequence.
///
/// A healthy checkpointed run stores a contiguous ascending run of ticks; a
/// non-empty result means a batch was lost and the run cannot be replayed or
/// scrubbed faithfully. Gaps are looked for **between** the lowest and highest
/// stored tick — a run whose frames start at tick 5 is not "missing" 0..5, it
/// simply has not been asked to store them.
///
/// Returns an empty vector for a contiguous run and for a run with no frames.
///
/// # Errors
/// Returns an error if the query fails.
pub async fn tick_gaps(conn: &mut AsyncPgConnection, run_id: i64) -> AutumnResult<Vec<i32>> {
    let ticks: Vec<i32> = frames::table
        .filter(frames::run_id.eq(run_id))
        .order(frames::tick.asc())
        .select(frames::tick)
        .load(conn)
        .await?;

    let mut gaps = Vec::new();
    for pair in ticks.windows(2) {
        for missing in (pair[0] + 1)..pair[1] {
            gaps.push(missing);
        }
    }
    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::subsample;

    #[test]
    fn subsample_keeps_endpoints_and_spreads_the_rest() {
        let items: Vec<i32> = (0..101).collect();
        assert_eq!(subsample(items.clone(), 5), vec![0, 25, 50, 75, 100]);
        assert_eq!(subsample(items.clone(), 3), vec![0, 50, 100]);
        assert_eq!(subsample(items.clone(), 2), vec![0, 100]);
    }

    #[test]
    fn subsample_handles_degenerate_budgets() {
        let items: Vec<i32> = (0..10).collect();
        assert!(subsample(items.clone(), 0).is_empty());
        assert_eq!(subsample(items.clone(), 1), vec![0]);
        assert_eq!(subsample(items.clone(), 10), items);
        assert_eq!(subsample(items.clone(), 99), items);
        assert!(subsample(Vec::<i32>::new(), 5).is_empty());
    }
}
