-- A named, immutable-once-used parameter set.
CREATE TABLE scenarios (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT      NOT NULL,
    config      JSONB     NOT NULL,
    config_hash TEXT      NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX scenarios_config_hash_idx ON scenarios (config_hash);
