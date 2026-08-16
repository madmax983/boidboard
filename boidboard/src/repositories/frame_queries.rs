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
/// With `limit = Some(n)` the result is **at most** `n` evenly spaced frames,
/// always including the first and last so a trajectory ribbon still spans the
/// whole run. That is the SVG rendering budget: a 10 000-tick run must not put
/// 10 000 marks on a page.
///
/// `limit = None` returns every frame; `limit = Some(0)` returns nothing.
///
/// # The budget is a *query* budget
///
/// This used to `.load()` every frame for the run and thin the result in Rust
/// afterwards, which made the budget purely cosmetic: a 301-frame run moved
/// 2.66 MB across the wire to render a 57 KB page, linearly and without bound,
/// and a long run turned one `GET /runs/{id}` into a multi-gigabyte allocation.
///
/// Now the sampling happens **before** the rows exist. Two round trips:
/// [`tick_bounds`] asks Postgres for `min(tick)`/`max(tick)` (index-only, one
/// row), [`sampled_ticks`] computes the ≤ `n` ticks the page actually wants,
/// and the fetch is `tick = ANY($ticks)` with a matching `LIMIT`. The database
/// therefore cannot return more than `n` rows however long the run is — the
/// bound is structural, not a filter applied to something already too large.
///
/// [`subsample`] still runs on the result. It is a no-op on the normal path
/// (the query already returned at most `n` rows) and exists for the abnormal
/// one: a run with gaps in its tick sequence can match fewer ticks than the
/// slots asked for, and the invariant "at most `n`, endpoints included" should
/// hold for a damaged run too.
///
/// # Errors
/// Returns an error if the query fails.
pub async fn frames_for_run(
    conn: &mut AsyncPgConnection,
    run_id: i64,
    limit: Option<usize>,
) -> AutumnResult<Vec<Frame>> {
    let Some(budget) = limit else {
        let all: Vec<Frame> = frames::table
            .filter(frames::run_id.eq(run_id))
            .order(frames::tick.asc())
            .select(Frame::as_select())
            .load(conn)
            .await?;
        return Ok(all);
    };

    if budget == 0 {
        return Ok(Vec::new());
    }

    let Some((lowest, highest)) = tick_bounds(conn, run_id).await? else {
        return Ok(Vec::new());
    };

    let wanted = sampled_ticks(lowest, highest, budget);
    let rows: Vec<Frame> = frames::table
        .filter(frames::run_id.eq(run_id))
        .filter(frames::tick.eq_any(&wanted))
        .order(frames::tick.asc())
        // Belt as well as braces: `wanted` is already at most `budget` long, so
        // this can never truncate a correct result. It is here so the bound
        // survives a future edit to the tick selection.
        .limit(i64::try_from(budget).unwrap_or(i64::MAX))
        .select(Frame::as_select())
        .load(conn)
        .await?;

    Ok(subsample(rows, budget))
}

/// The lowest and highest stored tick for a run, or `None` when it has none.
///
/// One row out of Postgres regardless of how many frames the run has — this is
/// what makes the detail page's cost independent of run length.
///
/// # Errors
/// Returns an error if the query fails.
pub async fn tick_bounds(
    conn: &mut AsyncPgConnection,
    run_id: i64,
) -> AutumnResult<Option<(i32, i32)>> {
    let bounds: (Option<i32>, Option<i32>) = frames::table
        .filter(frames::run_id.eq(run_id))
        .select((
            diesel::dsl::min(frames::tick),
            diesel::dsl::max(frames::tick),
        ))
        .first(conn)
        .await?;

    Ok(match bounds {
        (Some(lowest), Some(highest)) => Some((lowest, highest)),
        _ => None,
    })
}

/// The ticks a budgeted query asks the database for: at most `budget` values,
/// evenly spread across `lowest..=highest`, both endpoints included.
///
/// This is the whole reason [`frames_for_run`]'s budget is a *query* budget —
/// the returned list is what goes into `tick = ANY(...)`, so its length is the
/// hard ceiling on how many rows Postgres can produce. Ascending and free of
/// duplicates, so it also doubles as the expected tick sequence in tests.
///
/// A run of one tick, or a budget of one, yields just the first tick; a budget
/// of zero yields nothing.
#[must_use]
pub fn sampled_ticks(lowest: i32, highest: i32, budget: usize) -> Vec<i32> {
    if budget == 0 {
        return Vec::new();
    }
    if budget == 1 || highest <= lowest {
        return vec![lowest];
    }

    let span = i64::from(highest) - i64::from(lowest);
    // Asking for more slots than there are ticks would build a list longer than
    // the answer can be; one slot per tick is the most that can ever help.
    let slots = i64::try_from(budget - 1).unwrap_or(span).min(span);

    // Slot s maps to lowest + round(s * span / slots), pinning slot 0 to the
    // first tick and slot `slots` to the last. Monotonic, so `dedup` removes
    // every repeat a coarse span produces.
    let mut ticks: Vec<i32> = (0..=slots)
        .map(|s| {
            let offset = (2 * s * span + slots) / (2 * slots);
            i32::try_from(i64::from(lowest) + offset).unwrap_or(highest)
        })
        .collect();
    ticks.dedup();
    ticks
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
    use super::{sampled_ticks, subsample};

    // ── the query budget (H2) ──
    //
    // `frames_for_run` asks Postgres for `tick = ANY(sampled_ticks(..))` with a
    // matching `LIMIT`, so the length of this list *is* the ceiling on how many
    // rows the database can return. Bounding it here bounds the query.

    #[test]
    fn sampled_ticks_never_exceeds_the_budget_however_long_the_run() {
        for span in [0, 1, 5, 300, 10_000, 100_000, i32::MAX - 1] {
            let ticks = sampled_ticks(0, span, 120);
            assert!(
                ticks.len() <= 120,
                "a {span}-tick run must not ask the database for {} rows on a \
                 budget of 120",
                ticks.len()
            );
            assert_eq!(ticks[0], 0, "the first tick is always asked for");
            assert_eq!(
                *ticks.last().expect("non-empty"),
                span,
                "the last tick is always asked for, so a ribbon spans the run"
            );
            assert!(
                ticks.windows(2).all(|w| w[0] < w[1]),
                "ascending and duplicate-free, or the IN list wastes slots"
            );
        }
    }

    #[test]
    fn sampled_ticks_matches_the_positions_index_subsampling_would_have_chosen() {
        // The pre-fix code loaded every frame and kept indices 0, 25, 50, 75,
        // 100. Frames are contiguous by construction, so tick == index + first,
        // and the two must agree — otherwise this "optimisation" would quietly
        // change which frames the page draws.
        assert_eq!(sampled_ticks(0, 100, 5), vec![0, 25, 50, 75, 100]);
        assert_eq!(sampled_ticks(0, 100, 3), vec![0, 50, 100]);
        assert_eq!(sampled_ticks(0, 100, 2), vec![0, 100]);
        // A run whose frames start late is sampled across what it has, not
        // across the ticks it never stored.
        assert_eq!(sampled_ticks(40, 140, 5), vec![40, 65, 90, 115, 140]);
    }

    #[test]
    fn sampled_ticks_handles_degenerate_budgets_and_spans() {
        assert!(sampled_ticks(0, 100, 0).is_empty());
        assert_eq!(sampled_ticks(0, 100, 1), vec![0]);
        assert_eq!(sampled_ticks(7, 7, 120), vec![7]);
        // A budget wider than the run asks for every tick and no more.
        assert_eq!(sampled_ticks(0, 4, 1_000), vec![0, 1, 2, 3, 4]);
    }

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
