-- Workout execution: sessions and performed sets — training history itself.
--
-- Prescribed and performed are separate immutable records (docs/domain-model.md).
-- This migration is where "immutable" stops being a docstring:
--
--   * performed_sets gets INSERT and SELECT only. No UPDATE, no DELETE, for
--     anyone using the app role. A performed set is a fact about the past.
--   * sessions can change status exactly once (open -> completed/abandoned),
--     enforced by trigger.
--   * sets can only be written into an OPEN session, enforced by trigger.
--
-- Ids arrive from the client (ADR-0008): a phone mints UUIDv7s offline and syncs
-- later. Inserts are therefore idempotent — a replayed insert with a known id is
-- a no-op, which the repository implements with ON CONFLICT DO NOTHING.

CREATE TABLE workout_sessions (
    id                   UUID        PRIMARY KEY,
    gym_id               UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    athlete_id           UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    assignment_id        UUID        NOT NULL REFERENCES program_assignments(id) ON DELETE RESTRICT,
    workout_template_id  UUID        NOT NULL REFERENCES workout_templates(id) ON DELETE RESTRICT,
    started_at           TIMESTAMPTZ NOT NULL,
    status               TEXT        NOT NULL
        CHECK (status IN ('in_progress','completed','abandoned')),
    completed_at         TIMESTAMPTZ,
    abandoned_at         TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT workout_sessions_status_evidence CHECK (
        (status <> 'completed' OR completed_at IS NOT NULL)
        AND (status <> 'abandoned' OR abandoned_at IS NOT NULL)
        AND (status <> 'in_progress' OR (completed_at IS NULL AND abandoned_at IS NULL))
    )
);

CREATE INDEX workout_sessions_athlete_idx
    ON workout_sessions (gym_id, athlete_id, started_at DESC);
CREATE INDEX workout_sessions_assignment_idx ON workout_sessions (assignment_id);

CREATE TABLE performed_sets (
    id                    UUID    PRIMARY KEY,
    session_id            UUID    NOT NULL REFERENCES workout_sessions(id) ON DELETE RESTRICT,
    gym_id                UUID    NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    exercise_id           UUID    NOT NULL REFERENCES exercises(id) ON DELETE RESTRICT,
    -- Which prescription line this answers; NULL is work outside the plan.
    template_exercise_id  UUID    REFERENCES workout_template_exercises(id) ON DELETE RESTRICT,
    set_number            INTEGER NOT NULL CHECK (set_number BETWEEN 1 AND 100),
    -- Tagged union, exactly as the domain enum serialises — same pattern as
    -- prescriptions. The CHECK keeps the tag honest.
    performed             JSONB   NOT NULL
        CHECK (performed->>'kind' IN ('repetitions','duration','distance')),
    rpe                   SMALLINT CHECK (rpe BETWEEN 0 AND 10),
    logged_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row per (session, exercise, set n). Idempotent replays hit the id
-- conflict; genuine double-logging of "set 2" hits this one.
CREATE UNIQUE INDEX performed_sets_unique
    ON performed_sets (session_id, exercise_id, set_number);
CREATE INDEX performed_sets_gym_idx ON performed_sets (gym_id);
CREATE INDEX performed_sets_exercise_idx ON performed_sets (exercise_id);

-- ------------------------------------------------- session state, database-side

-- The session's template must belong to the assignment's pinned version, and
-- the assignment must belong to the session's athlete. Without this a session
-- could claim to execute a workout from a programme the athlete was never on,
-- and every adherence number downstream becomes fiction.
CREATE OR REPLACE FUNCTION check_session_validity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    a RECORD;
    template_version UUID;
BEGIN
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

CREATE TRIGGER workout_sessions_validity
    BEFORE INSERT ON workout_sessions
    FOR EACH ROW EXECUTE FUNCTION check_session_validity();

-- A finished session is finished. The only legal UPDATE is the single move
-- out of in_progress; everything else about the row is frozen even then.
CREATE OR REPLACE FUNCTION reject_finished_session_edit()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'in_progress' THEN
        RAISE EXCEPTION 'a % session cannot be changed', OLD.status
            USING ERRCODE = 'raise_exception';
    END IF;
    IF NEW.id <> OLD.id
       OR NEW.gym_id <> OLD.gym_id
       OR NEW.athlete_id <> OLD.athlete_id
       OR NEW.assignment_id <> OLD.assignment_id
       OR NEW.workout_template_id <> OLD.workout_template_id
       OR NEW.started_at <> OLD.started_at
    THEN
        RAISE EXCEPTION 'only a session''s status may change'
            USING ERRCODE = 'raise_exception';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER workout_sessions_frozen
    BEFORE UPDATE ON workout_sessions
    FOR EACH ROW EXECUTE FUNCTION reject_finished_session_edit();

-- Sets go into open sessions only. The domain refuses too; this catches
-- everything the domain never sees.
CREATE OR REPLACE FUNCTION check_set_session_open()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    s RECORD;
BEGIN
    SELECT status, gym_id INTO s FROM workout_sessions WHERE id = NEW.session_id;

    IF s IS NULL THEN
        RAISE EXCEPTION 'session not found' USING ERRCODE = 'raise_exception';
    END IF;
    IF s.status <> 'in_progress' THEN
        RAISE EXCEPTION 'cannot log sets into a % session', s.status
            USING ERRCODE = 'raise_exception';
    END IF;
    IF s.gym_id <> NEW.gym_id THEN
        RAISE EXCEPTION 'set does not belong to the session''s gym'
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER performed_sets_open_session
    BEFORE INSERT ON performed_sets
    FOR EACH ROW EXECUTE FUNCTION check_set_session_open();

-- ------------------------------------------------------------------------ RLS

ALTER TABLE workout_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE workout_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY workout_sessions_tenant_isolation ON workout_sessions
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

ALTER TABLE performed_sets ENABLE ROW LEVEL SECURITY;
ALTER TABLE performed_sets FORCE ROW LEVEL SECURITY;
CREATE POLICY performed_sets_tenant_isolation ON performed_sets
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE ON workout_sessions TO gym_app;
-- History: INSERT and SELECT. Nothing else, for anyone holding this role.
GRANT SELECT, INSERT ON performed_sets TO gym_app;
