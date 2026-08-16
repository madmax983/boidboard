//! `runs` — one execution of a scenario, plus its provenance record.

use crate::schema::runs;

/// The lifecycle states a run can be in.
///
/// Stored as `TEXT` rather than a Postgres `ENUM` so adding a state is an
/// application change, not a migration that locks the table. These constants
/// exist so callers never have to spell a status by hand.
pub mod status {
    /// Created, not yet picked up by a workflow.
    pub const QUEUED: &str = "queued";
    /// A workflow is driving batches.
    pub const RUNNING: &str = "running";
    /// Reached `max_ticks` or its own termination condition.
    pub const COMPLETED: &str = "completed";
    /// Stopped early by a cancel signal (AC-33); partial frames intact.
    pub const CANCELLED: &str = "cancelled";
    /// Stopped by an error; see `runs.error`.
    pub const FAILED: &str = "failed";
    /// Stopped by the `max_ticks` guardrail (AC-35).
    pub const BUDGET_EXCEEDED: &str = "budget_exceeded";

    /// Every valid status, for validation and for UI filters.
    pub const ALL: [&str; 6] = [
        QUEUED,
        RUNNING,
        COMPLETED,
        CANCELLED,
        FAILED,
        BUDGET_EXCEEDED,
    ];

    /// Whether `status` is terminal — no further batches will be scheduled.
    #[must_use]
    pub fn is_terminal(status: &str) -> bool {
        matches!(status, COMPLETED | CANCELLED | FAILED | BUDGET_EXCEEDED)
    }
}

/// One execution of a scenario.
///
/// `config_snapshot` is a **copy** of the scenario's config taken when the run
/// was created, not a reference to it. That copy is what makes AC-39 true:
/// editing the scenario afterwards cannot mutate what an already-created run
/// says it ran. `config_hash`, `kernel_version`, `seed` and `final_state_hash`
/// together form the provenance record required by AC-38 — everything needed to
/// reproduce the run and check that the reproduction matched.
#[autumn_web::model]
pub struct Run {
    #[id]
    pub id: i64,
    #[indexed]
    pub scenario_id: i64,
    pub seed: i64,
    #[indexed]
    pub status: String,
    pub max_ticks: i32,
    // Deliberately NOT `#[default]`: `#[default]` excludes a field from BOTH
    // `NewRun` and `UpdateRun`, and the workflow must advance this on every
    // batch. The column keeps its `DEFAULT 0` for hand-written SQL; callers
    // going through the model pass `0` explicitly at creation.
    pub ticks_completed: i32,
    pub config_snapshot: serde_json::Value,
    pub config_hash: String,
    pub kernel_version: String,
    pub final_state_hash: Option<String>,
    pub error: Option<String>,
    pub workflow_execution_id: Option<String>,
    #[default]
    pub created_at: chrono::NaiveDateTime,
    // Maintained by the `runs_set_updated_at` database trigger, not by the
    // application — `#[default]` keeps it out of `NewRun`/`UpdateRun`, so no
    // caller can forget to bump it and no caller can backdate it.
    #[default]
    pub updated_at: chrono::NaiveDateTime,
}
