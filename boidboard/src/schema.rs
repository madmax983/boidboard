//! Diesel table definitions for the Boidboard schema.
//!
//! Hand-maintained to mirror `migrations/`. Primary keys are `BIGSERIAL`/`i64`
//! throughout: Autumn's `#[model]`/`#[repository]` macros hardcode `i64` ids, so
//! a UUID key would compile and then fail at runtime.

diesel::table! {
    /// A named, immutable-once-used parameter set.
    scenarios (id) {
        id -> Int8,
        name -> Text,
        config -> Jsonb,
        config_hash -> Text,
        created_at -> Timestamp,
    }
}

diesel::table! {
    /// One execution of a scenario.
    runs (id) {
        id -> Int8,
        scenario_id -> Int8,
        seed -> Int8,
        status -> Text,
        max_ticks -> Int4,
        ticks_completed -> Int4,
        config_snapshot -> Jsonb,
        config_hash -> Text,
        kernel_version -> Text,
        final_state_hash -> Nullable<Text>,
        error -> Nullable<Text>,
        workflow_execution_id -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    /// A checkpointed simulation snapshot, unique per `(run_id, tick)`.
    frames (id) {
        id -> Int8,
        run_id -> Int8,
        tick -> Int4,
        agents -> Jsonb,
        state_hash -> Text,
        metrics -> Jsonb,
    }
}

diesel::table! {
    /// Provenance of an operator intervention (steer / cancel).
    run_signals (id) {
        id -> Int8,
        run_id -> Int8,
        tick -> Int4,
        kind -> Text,
        payload -> Jsonb,
        created_at -> Timestamp,
    }
}

diesel::joinable!(runs -> scenarios (scenario_id));
diesel::joinable!(frames -> runs (run_id));
diesel::joinable!(run_signals -> runs (run_id));

diesel::allow_tables_to_appear_in_same_query!(scenarios, runs, frames, run_signals);
