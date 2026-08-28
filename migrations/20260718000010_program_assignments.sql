-- Programme assignments: a published version reaching a person (ADR-0006).
--
-- The invariant the schema defends: an assignment pins a SPECIFIC published
-- version. Publishing v2 must not move anyone silently, and a draft must never
-- be executable by a member. The domain refuses both; these triggers refuse them
-- again, because the application is not the only thing that can write here.

CREATE TABLE program_assignments (
    id                  UUID        PRIMARY KEY,
    gym_id              UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,

    -- RESTRICT everywhere a delete would orphan training history. Accounts are
    -- deactivated, not dropped; programmes with assignments are archived, not
    -- deleted.
    athlete_id          UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    program_id          UUID        NOT NULL REFERENCES programs(id) ON DELETE RESTRICT,
    program_version_id  UUID        NOT NULL REFERENCES program_versions(id) ON DELETE RESTRICT,
    assigned_by         UUID        NOT NULL REFERENCES users(id),

    start_date          DATE        NOT NULL,
    status              TEXT        NOT NULL
        CHECK (status IN ('active','completed','withdrawn')),

    completed_at        TIMESTAMPTZ,
    withdrawn_at        TIMESTAMPTZ,
    withdrawn_by        UUID REFERENCES users(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Mirrors the AssignmentStatus enum: each state carries its evidence, and
    -- only its own.
    CONSTRAINT program_assignments_status_evidence CHECK (
        (status <> 'completed' OR completed_at IS NOT NULL)
        AND (status <> 'withdrawn' OR (withdrawn_at IS NOT NULL AND withdrawn_by IS NOT NULL))
        AND (status <> 'active' OR (completed_at IS NULL AND withdrawn_at IS NULL AND withdrawn_by IS NULL))
    )
);

-- One ACTIVE assignment per athlete per programme. Re-assignment after a
-- withdrawal is normal (hence partial); two simultaneous active assignments to
-- the same programme is a contradiction — which version is the member on?
CREATE UNIQUE INDEX program_assignments_one_active
    ON program_assignments (gym_id, athlete_id, program_id)
    WHERE status = 'active';

-- "My programme" and "my clients' programmes" are the hot paths.
CREATE INDEX program_assignments_athlete_idx
    ON program_assignments (gym_id, athlete_id) WHERE status = 'active';
CREATE INDEX program_assignments_version_idx
    ON program_assignments (program_version_id);

-- The validity checks the domain performs, repeated where nothing can skip them:
-- the version must be published, must belong to the named programme and gym
-- (program_id is denormalised for the unique index above, so it must be provably
-- consistent), and the athlete must be a current member of the gym.
CREATE OR REPLACE FUNCTION check_assignment_validity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    v RECORD;
BEGIN
    SELECT status, program_id, gym_id INTO v
    FROM program_versions WHERE id = NEW.program_version_id;

    IF v IS NULL THEN
        RAISE EXCEPTION 'program version not found'
            USING ERRCODE = 'raise_exception';
    END IF;
    IF v.status <> 'published' THEN
        RAISE EXCEPTION 'only a published program version can be assigned'
            USING ERRCODE = 'raise_exception';
    END IF;
    IF v.program_id <> NEW.program_id OR v.gym_id <> NEW.gym_id THEN
        RAISE EXCEPTION 'assignment does not match the version''s programme'
            USING ERRCODE = 'raise_exception';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM gym_capacities
        WHERE gym_id = NEW.gym_id AND user_id = NEW.athlete_id AND revoked_at IS NULL
    ) THEN
        RAISE EXCEPTION 'athlete does not belong to this gym'
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER program_assignments_validity
    BEFORE INSERT ON program_assignments
    FOR EACH ROW EXECUTE FUNCTION check_assignment_validity();

-- ------------------------------------------------------------------------ RLS

ALTER TABLE program_assignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE program_assignments FORCE ROW LEVEL SECURITY;

CREATE POLICY program_assignments_tenant_isolation ON program_assignments
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE ON program_assignments TO gym_app;
-- No DELETE grant: assignments are ended, never erased. Performed workouts will
-- reference them, and history that can vanish is not history.
