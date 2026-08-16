//! Repository for [`Frame`](crate::models::Frame).

use crate::models::{Frame, NewFrame, UpdateFrame};
use crate::schema::frames;

/// Generated CRUD plus the derived lookup below, on `PgFrameRepository`.
///
/// The interesting frame queries are the hand-written connection-level
/// functions in this module's siblings — see [`insert_frames_idempotent`],
/// [`max_tick`], [`frames_for_run`] and [`tick_gaps`] in
/// [`crate::repositories`].
#[autumn_web::repository(Frame)]
pub trait FrameRepository {
    /// Every frame of a run, unordered and unbounded. Prefer
    /// [`frames_for_run`](crate::repositories::frames_for_run), which orders by
    /// tick and can subsample to a rendering budget.
    fn find_by_run_id(run_id: i64) -> Vec<Frame>;
}
