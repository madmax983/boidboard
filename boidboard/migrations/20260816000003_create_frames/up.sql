-- A checkpointed simulation snapshot.
--
-- UNIQUE (run_id, tick) is load-bearing: it is what makes at-least-once
-- activity retries idempotent by construction (AC-32). The constraint's
-- backing btree index is also the (run_id, tick) lookup index — Postgres
-- creates it implicitly, so no separate CREATE INDEX is needed.
CREATE TABLE frames (
    id         BIGSERIAL PRIMARY KEY,
    run_id     BIGINT  NOT NULL REFERENCES runs (id),
    tick       INTEGER NOT NULL,
    agents     JSONB   NOT NULL,
    state_hash TEXT    NOT NULL,
    metrics    JSONB   NOT NULL,
    CONSTRAINT frames_run_id_tick_key UNIQUE (run_id, tick)
);
