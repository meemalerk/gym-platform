-- One account, many capacities. See ADR-0014.
--
-- Replaces the single-role `memberships` table with a set of granted capacities,
-- adds person-owned profiles, and adds invitations (how anyone other than a gym's
-- founder gets in).

-- ------------------------------------------------------- personal workspaces

-- A solo trainer or self-coached athlete gets a gym of their own rather than a
-- separate gym-less code path. One tenancy model, one set of RLS policies.
ALTER TABLE gyms ADD COLUMN is_personal BOOLEAN NOT NULL DEFAULT false;

-- ------------------------------------------------------------- capacities

CREATE TABLE gym_capacities (
    id          UUID        PRIMARY KEY,
    gym_id      UUID        NOT NULL REFERENCES gyms(id)  ON DELETE CASCADE,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    capacity    TEXT        NOT NULL
                CHECK (capacity IN ('owner','admin','head_coach','trainer','member')),
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_by  UUID        REFERENCES users(id),
    revoked_at  TIMESTAMPTZ
);

-- A capacity is held at most once per (gym, user) — but a user may hold SEVERAL
-- different capacities in the same gym (e.g. trainer AND member).
CREATE UNIQUE INDEX gym_capacities_active_unique
    ON gym_capacities (gym_id, user_id, capacity)
    WHERE revoked_at IS NULL;

-- "who is in this gym" and "which gyms am I in" are both hot paths.
CREATE INDEX gym_capacities_gym_idx  ON gym_capacities (gym_id)  WHERE revoked_at IS NULL;
CREATE INDEX gym_capacities_user_idx ON gym_capacities (user_id) WHERE revoked_at IS NULL;

-- Carry existing rows over. `instructor` becomes `trainer` (ADR-0014 renames it to
-- the word owners and users actually use).
INSERT INTO gym_capacities (id, gym_id, user_id, capacity, granted_at, revoked_at)
SELECT
    id,
    gym_id,
    user_id,
    CASE role WHEN 'instructor' THEN 'trainer' ELSE role END,
    created_at,
    revoked_at
FROM memberships;

DROP TABLE memberships;

-- --------------------------------------------------------------- profiles

-- Person-owned, NOT tenant-owned: deliberately no gym_id. Your goals and injury
-- history are yours and follow you between gyms — that is the point of one
-- account. A gym may read them because the person holds `member` there, which is
-- a relationship question, not ownership.

CREATE TABLE athlete_profiles (
    user_id             UUID        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    goals               TEXT        CHECK (goals IS NULL OR length(goals) <= 2000),
    training_age_months INTEGER     CHECK (training_age_months IS NULL
                                           OR training_age_months BETWEEN 0 AND 1200),
    limitations         TEXT        CHECK (limitations IS NULL OR length(limitations) <= 2000),
    date_of_birth       DATE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE trainer_profiles (
    user_id        UUID        PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    headline       TEXT        CHECK (headline IS NULL OR length(headline) <= 120),
    bio            TEXT        CHECK (bio IS NULL OR length(bio) <= 4000),
    certifications TEXT[]      NOT NULL DEFAULT '{}',
    specialties    TEXT[]      NOT NULL DEFAULT '{}',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ------------------------------------------------------------ invitations

-- How anyone other than a gym's founder gets in. Accepting adds capacities to the
-- invitee's EXISTING account — a person never ends up with a second login.
CREATE TABLE gym_invitations (
    id          UUID        PRIMARY KEY,
    gym_id      UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    email       TEXT        NOT NULL CHECK (email = lower(email)),
    capacities  TEXT[]      NOT NULL
                CHECK (
                    cardinality(capacities) > 0
                    AND capacities <@ ARRAY['owner','admin','head_coach','trainer','member']::TEXT[]
                ),
    -- Only the hash is stored; the raw token goes in the invite link exactly once.
    token_hash  BYTEA       NOT NULL UNIQUE,
    invited_by  UUID        NOT NULL REFERENCES users(id),
    expires_at  TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    accepted_by UUID        REFERENCES users(id),
    revoked_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- accepted_at and accepted_by are set together or not at all.
    CONSTRAINT gym_invitations_accepted_consistent
        CHECK ((accepted_at IS NULL) = (accepted_by IS NULL))
);

-- One outstanding invitation per (gym, email).
CREATE UNIQUE INDEX gym_invitations_pending_unique
    ON gym_invitations (gym_id, email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;

CREATE INDEX gym_invitations_gym_idx   ON gym_invitations (gym_id);
CREATE INDEX gym_invitations_email_idx ON gym_invitations (email)
    WHERE accepted_at IS NULL AND revoked_at IS NULL;
