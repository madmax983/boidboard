//! `frames` — a checkpointed simulation snapshot.

use crate::schema::frames;

/// One checkpointed tick of a run.
///
/// `UNIQUE (run_id, tick)` in the migration is what makes at-least-once
/// activity retries idempotent by construction (AC-32) — see
/// [`insert_frames_idempotent`](crate::repositories::insert_frames_idempotent).
///
/// `agents` and `metrics` are opaque `JSONB`; the persistence layer never looks
/// inside them.
#[autumn_web::model]
pub struct Frame {
    #[id]
    pub id: i64,
    #[indexed]
    pub run_id: i64,
    pub tick: i32,
    pub agents: serde_json::Value,
    pub state_hash: String,
    pub metrics: serde_json::Value,
}
