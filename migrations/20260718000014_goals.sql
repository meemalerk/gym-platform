-- Goals: a measurable target for one athlete in one gym.
--
-- Tenant-owned (not person-owned) because a lift goal references a gym-scoped
-- exercise, and because a goal is a coaching artefact — it belongs to the
-- relationship context it was set in. The metric is a tagged JSONB union, same
-- pattern as prescriptions, carrying the BASELINE captured at creation: without
-- it, "70% of the way there" has no denominator. Progress itself is computed at
-- the edge and never stored.

CREATE TABLE goals (
    id           UUID        PRIMARY KEY,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    athlete_id   UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    set_by       UUID        NOT NULL REFERENCES users(id),
    metric       JSONB       NOT NULL
        CHECK (metric->>'kind' IN ('bodyweight','exercise_est_1rm')),
    target_date  DATE,
    status       TEXT        NOT NULL CHECK (status IN ('active','achieved','abandoned')),
    achieved_at  TIMESTAMPTZ,
    confirmed_by UUID REFERENCES users(id),
    abandoned_at TIMESTAMPTZ,
    abandoned_by UUID REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT goals_status_evidence CHECK (
        (status <> 'achieved' OR (achieved_at IS NOT NULL AND confirmed_by IS NOT NULL))
        AND (status <> 'abandoned' OR (abandoned_at IS NOT NULL AND abandoned_by IS NOT NULL))
        AND (status <> 'active' OR (achieved_at IS NULL AND abandoned_at IS NULL))
    )
);

CREATE INDEX goals_athlete_idx ON goals (gym_id, athlete_id) WHERE status = 'active';
CREATE INDEX goals_gym_idx ON goals (gym_id);

ALTER TABLE goals ENABLE ROW LEVEL SECURITY;
ALTER TABLE goals FORCE ROW LEVEL SECURITY;
CREATE POLICY goals_tenant_isolation ON goals
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE ON goals TO gym_app;
-- No DELETE: an abandoned goal is part of the coaching story. The explicit
-- revoke, because migration 0003's default privileges would otherwise grant it
-- silently — the lesson of migration 0012.
REVOKE DELETE ON goals FROM gym_app;
