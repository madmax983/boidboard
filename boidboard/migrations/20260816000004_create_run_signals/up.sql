-- Provenance of operator interventions (AC-34): every steer or cancel signal
-- is recorded so a run explains its own behaviour after the fact.
CREATE TABLE run_signals (
    id         BIGSERIAL PRIMARY KEY,
    run_id     BIGINT    NOT NULL REFERENCES runs (id),
    tick       INTEGER   NOT NULL,
    kind       TEXT      NOT NULL,
    payload    JSONB     NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX run_signals_run_id_tick_idx ON run_signals (run_id, tick);
