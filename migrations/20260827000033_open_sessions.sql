-- Open sessions — a workout the member builds themselves.
--
-- ADR-0035. Someone on an Open Gym membership holds `gym_access` and nothing
-- else: no coach, so no coach to prescribe, so no assignment, so — until now —
-- no way to record that they trained at all. The app tracked nothing for the
-- membership the gym sells most of.
--
-- The fix is not a new table. `performed_sets.template_exercise_id` has been
-- nullable since the first execution migration, described there as "work
-- outside the plan"; a session with no plan at all is the same idea one level
-- up. So the plan link becomes optional and everything downstream — sets,
-- progress, est-1RM, exercise history — keeps working untouched, because none
-- of it reads the prescription. Prescribed and performed stay two records; an
-- open session simply has no prescribed half, which is the honest shape.

-- Both or neither, never one. A session pointing at an assignment but no
-- workout (or the reverse) is a half-written plan link, and the validity
-- trigger below would have nothing coherent to check.
ALTER TABLE workout_sessions
    ALTER COLUMN assignment_id       DROP NOT NULL,
    ALTER COLUMN workout_template_id DROP NOT NULL,
    ADD CONSTRAINT workout_sessions_plan_link_whole CHECK (
        (assignment_id IS NULL) = (workout_template_id IS NULL)
    );

-- What the member called it. Only ever set on an open session: a planned one
-- is named by its workout template, and storing a copy of that name here would
-- be a second source of truth that goes stale the moment the workout is
-- renamed in a new version.
ALTER TABLE workout_sessions
    ADD COLUMN title TEXT
        CHECK (title IS NULL OR (length(btrim(title)) BETWEEN 1 AND 80));

ALTER TABLE workout_sessions
    ADD CONSTRAINT workout_sessions_title_is_for_open CHECK (
        title IS NULL OR assignment_id IS NULL
    );

-- The validity trigger asserted three things about the assignment. With no
-- assignment there is nothing to assert, and the old body would raise
-- "assignment not found" on every open session — so it now returns early.
-- The checks themselves are unchanged: this widens what may be inserted, it
-- does not weaken what is checked when a plan link is present.
CREATE OR REPLACE FUNCTION check_session_validity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    a RECORD;
    template_version UUID;
BEGIN
    -- An open session: no plan to contradict. The CHECK constraint above has
    -- already established that the other half is null too.
    IF NEW.assignment_id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT athlete_id, program_version_id, status INTO a
    FROM program_assignments WHERE id = NEW.assignment_id;

    IF a IS NULL THEN
        RAISE EXCEPTION 'assignment not found' USING ERRCODE = 'raise_exception';
    END IF;
    IF a.athlete_id <> NEW.athlete_id THEN
        RAISE EXCEPTION 'session athlete does not match the assignment'
            USING ERRCODE = 'raise_exception';
    END IF;
    IF a.status <> 'active' THEN
        RAISE EXCEPTION 'assignment is not active' USING ERRCODE = 'raise_exception';
    END IF;

    SELECT w.version_id INTO template_version
    FROM workout_templates t
    JOIN program_weeks w ON w.id = t.week_id
    WHERE t.id = NEW.workout_template_id;

    IF template_version IS NULL OR template_version <> a.program_version_id THEN
        RAISE EXCEPTION 'workout does not belong to the assigned programme version'
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN NEW;
END;
$$;

-- The freeze trigger compared with `<>`, which was total while these columns
-- were NOT NULL and stops being so the moment they are nullable: `NULL <> 'x'`
-- is NULL, an IF on NULL does not fire, and the guard would have quietly
-- allowed an open session to be given an assignment after the fact. Every
-- comparison here is now IS DISTINCT FROM, which is the null-safe form. The
-- new `title` column joins the frozen set for the same reason the others are
-- in it — only the status may change.
CREATE OR REPLACE FUNCTION reject_finished_session_edit()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'in_progress' THEN
        RAISE EXCEPTION 'a % session cannot be changed', OLD.status
            USING ERRCODE = 'raise_exception';
    END IF;
    IF NEW.id                  IS DISTINCT FROM OLD.id
       OR NEW.gym_id              IS DISTINCT FROM OLD.gym_id
       OR NEW.athlete_id          IS DISTINCT FROM OLD.athlete_id
       OR NEW.assignment_id       IS DISTINCT FROM OLD.assignment_id
       OR NEW.workout_template_id IS DISTINCT FROM OLD.workout_template_id
       OR NEW.started_at          IS DISTINCT FROM OLD.started_at
       OR NEW.title               IS DISTINCT FROM OLD.title
    THEN
        RAISE EXCEPTION 'only a session''s status may change'
            USING ERRCODE = 'raise_exception';
    END IF;
    RETURN NEW;
END;
$$;

-- The assignment index was unconditional; most open-session rows would sit in
-- it as nulls for no reader. Partial keeps it the size of the question it
-- answers ("which sessions ran against this assignment").
DROP INDEX IF EXISTS workout_sessions_assignment_idx;
CREATE INDEX workout_sessions_assignment_idx
    ON workout_sessions (assignment_id)
    WHERE assignment_id IS NOT NULL;
