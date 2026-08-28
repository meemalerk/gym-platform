-- Fix: the frozen-content trigger could not insert into program_weeks at all.
--
-- The previous version resolved the target version with a single CASE
-- expression. A CASE is one SQL expression, so every branch's column references
-- are resolved regardless of which branch is selected — and `NEW.week_id` does
-- not exist on a program_weeks row. Every insert failed with
-- `record "new" has no field "week_id"`.
--
-- Branching with IF/ELSIF instead keeps each field reference inside a statement
-- that only executes for the table that actually has that column.
--
-- Worth noting how this was caught: the first run of
-- scripts/verify-program-immutability.sh reported the "must be refused" checks
-- as passing while the setup was quietly broken, because psql exits 0 on a failed
-- statement unless ON_ERROR_STOP is set. A test harness that cannot fail is worse
-- than no harness — it actively tells you the invariant holds.

CREATE OR REPLACE FUNCTION reject_frozen_version_content()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    target_version UUID;
    current_status TEXT;
BEGIN
    IF TG_TABLE_NAME = 'program_weeks' THEN
        target_version := COALESCE(NEW.version_id, OLD.version_id);

    ELSIF TG_TABLE_NAME = 'workout_templates' THEN
        SELECT version_id INTO target_version
        FROM program_weeks
        WHERE id = COALESCE(NEW.week_id, OLD.week_id);

    ELSE
        SELECT w.version_id INTO target_version
        FROM workout_templates t
        JOIN program_weeks w ON w.id = t.week_id
        WHERE t.id = COALESCE(NEW.workout_id, OLD.workout_id);
    END IF;

    SELECT status INTO current_status
    FROM program_versions WHERE id = target_version;

    -- A cascading delete of the whole version already removed the parent row.
    -- That is teardown, not an edit, so let it through.
    IF current_status IS NULL THEN
        RETURN COALESCE(NEW, OLD);
    END IF;

    IF current_status <> 'draft' THEN
        RAISE EXCEPTION
            'cannot modify content of a % program version', current_status
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN COALESCE(NEW, OLD);
END;
$$;
