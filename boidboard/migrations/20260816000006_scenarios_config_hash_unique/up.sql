-- Make "one scenario per canonical config hash" an invariant the database
-- enforces, rather than a race the application hopes to win.
--
-- `create_run` finds-or-creates a scenario by `config_hash`. Read-then-insert
-- is a TOCTOU: two submissions of the same preset that arrive together both see
-- no row and both insert one, and the bench then holds two scenarios that are
-- the same scenario. The fix is `INSERT … ON CONFLICT (config_hash) DO NOTHING`
-- followed by a re-read, and that conflict target requires a UNIQUE index —
-- Postgres will not accept a plain one.
--
-- The plain index this replaces served exactly the same lookups, so nothing
-- gets slower.
DROP INDEX IF EXISTS scenarios_config_hash_idx;
CREATE UNIQUE INDEX scenarios_config_hash_idx ON scenarios (config_hash);
