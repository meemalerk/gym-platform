-- Coach–athlete relationships: who coaches whom.
--
-- Capacities are gym-wide — `can_coach` says "may coach here at all", never "may
-- coach Sara". Everything personal (an assigned programme, a measurement, a goal,
-- nutrition guidance) needs the second question, and this table answers it.
--
-- Relationships END rather than being deleted. A coach who stops working with an
-- athlete leaves behind programmes they wrote and notes they made; removing the
-- link would orphan that history or make it look like someone else's work.

CREATE TABLE coach_relationships (
    id          UUID        PRIMARY KEY,
    gym_id      UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,

    -- RESTRICT rather than CASCADE: deleting an account must not silently erase
    -- the record of who coached whom. Accounts are deactivated, not dropped.
    coach_id    UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    athlete_id  UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    status      TEXT        NOT NULL CHECK (status IN ('active','ended')),
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at    TIMESTAMPTZ,
    ended_by    UUID REFERENCES users(id),
    created_by  UUID        NOT NULL REFERENCES users(id),

    -- Mirrors the RelationshipStatus enum: "ended with no end date" is a state the
    -- Rust type cannot express, and this stops SQL expressing it either.
    CONSTRAINT coach_relationships_ended_evidence CHECK (
        (status <> 'ended' OR (ended_at IS NOT NULL AND ended_by IS NOT NULL))
        AND (status <> 'active' OR (ended_at IS NULL AND ended_by IS NULL))
    ),

    CONSTRAINT coach_relationships_ends_after_start CHECK (
        ended_at IS NULL OR ended_at >= started_at
    )
);

-- The pairing may repeat over time (reassigned away, then back), but only one
-- ACTIVE row may exist for a pair — otherwise ending "the" relationship is
-- ambiguous and access outlives its revocation.
CREATE UNIQUE INDEX coach_relationships_one_active
    ON coach_relationships (gym_id, coach_id, athlete_id)
    WHERE status = 'active';

-- "My clients" and "my coach" are the two hot queries.
CREATE INDEX coach_relationships_coach_idx ON coach_relationships (gym_id, coach_id)
    WHERE status = 'active';
CREATE INDEX coach_relationships_athlete_idx ON coach_relationships (gym_id, athlete_id)
    WHERE status = 'active';

-- Both parties must actually belong to this gym. Without this, a relationship
-- could name someone who was never a member — the application checks, but the
-- application is not the only thing that can write here.
CREATE OR REPLACE FUNCTION check_coach_relationship_membership()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM gym_capacities
        WHERE gym_id = NEW.gym_id AND user_id = NEW.coach_id
    ) THEN
        RAISE EXCEPTION 'coach does not belong to this gym'
            USING ERRCODE = 'raise_exception';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM gym_capacities
        WHERE gym_id = NEW.gym_id AND user_id = NEW.athlete_id
    ) THEN
        RAISE EXCEPTION 'athlete does not belong to this gym'
            USING ERRCODE = 'raise_exception';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER coach_relationships_membership
    BEFORE INSERT ON coach_relationships
    FOR EACH ROW EXECUTE FUNCTION check_coach_relationship_membership();

-- ------------------------------------------------------------------------ RLS

ALTER TABLE coach_relationships ENABLE ROW LEVEL SECURITY;
ALTER TABLE coach_relationships FORCE ROW LEVEL SECURITY;

CREATE POLICY coach_relationships_tenant_isolation ON coach_relationships
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

GRANT SELECT, INSERT, UPDATE ON coach_relationships TO gym_app;
-- No DELETE grant: ending a relationship is an UPDATE. Removing the row would
-- destroy the record of who was accountable for past coaching decisions.
