-- Back to a non-unique index. Note that `create_run`'s ON CONFLICT clause
-- names this index's column and will fail without the unique constraint, so
-- reverting this migration means reverting that code too.
DROP INDEX IF EXISTS scenarios_config_hash_idx;
CREATE INDEX scenarios_config_hash_idx ON scenarios (config_hash);
