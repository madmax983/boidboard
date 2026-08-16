-- Keep `runs.updated_at` honest.
--
-- The column can't be maintained from the application side: Autumn's
-- `#[default]` field attribute excludes a column from `UpdateRun` (that is what
-- makes it "the database supplies it"), so nothing in Rust can write it. Left
-- alone it would sit frozen at the creation timestamp forever and quietly lie
-- to the run list's "last activity" column.
--
-- `clock_timestamp()` rather than `now()`: `now()` is the *transaction* start
-- time, so a create-then-update inside one transaction would stamp both
-- timestamps identically.
CREATE FUNCTION boidboard_set_updated_at() RETURNS trigger AS $$
BEGIN
    NEW.updated_at := clock_timestamp();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER runs_set_updated_at
    BEFORE UPDATE ON runs
    FOR EACH ROW
    EXECUTE FUNCTION boidboard_set_updated_at();
