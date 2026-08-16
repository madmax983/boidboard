-- One execution of a scenario.
--
-- `config_snapshot` is a COPY of the scenario config taken when the run is
-- created. It is what makes AC-39 true: editing the scenario afterwards can
-- never mutate a completed run's configuration or its hash.
CREATE TABLE runs (
    id                    BIGSERIAL PRIMARY KEY,
    scenario_id           BIGINT    NOT NULL REFERENCES scenarios (id),
    seed                  BIGINT    NOT NULL,
    status                TEXT      NOT NULL,
    max_ticks             INTEGER   NOT NULL,
    ticks_completed       INTEGER   NOT NULL DEFAULT 0,
    config_snapshot       JSONB     NOT NULL,
    config_hash           TEXT      NOT NULL,
    kernel_version        TEXT      NOT NULL,
    final_state_hash      TEXT,
    error                 TEXT,
    workflow_execution_id TEXT,
    created_at            TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX runs_scenario_id_idx ON runs (scenario_id);
CREATE INDEX runs_status_idx ON runs (status);
