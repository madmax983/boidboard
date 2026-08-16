//! Repository for [`Run`](crate::models::Run).

use crate::models::{NewRun, Run, UpdateRun};
use crate::schema::runs;

/// Generated CRUD plus the derived lookups below, on `PgRunRepository`.
#[autumn_web::repository(Run)]
pub trait RunRepository {
    /// Every run of a given scenario — backs the compare view (AC-46).
    fn find_by_scenario_id(scenario_id: i64) -> Vec<Run>;
    /// Runs in a given lifecycle state — backs the run list's status filter.
    fn find_by_status(status: String) -> Vec<Run>;
}
