//! Repository for [`RunSignal`](crate::models::RunSignal).

use crate::models::{NewRunSignal, RunSignal, UpdateRunSignal};
use crate::schema::run_signals;

/// Generated CRUD plus the derived lookup below, on `PgRunSignalRepository`.
#[autumn_web::repository(RunSignal)]
pub trait RunSignalRepository {
    /// Every intervention recorded against a run, in insertion order — the
    /// provenance trail required by AC-34.
    fn find_by_run_id(run_id: i64) -> Vec<RunSignal>;
}
