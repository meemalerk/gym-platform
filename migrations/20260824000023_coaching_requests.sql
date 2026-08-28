-- A member asking a coach to take them on (ADR-0025).
--
-- The gap this fills: `coach_relationships` could only ever be created by a
-- head coach, which is correct — `may_coach_athlete` grants access to an
-- athlete's whole history, and letting a trainer create that link themselves
-- would make it a self-service permission grant. But it left no route at all
-- for the ordinary case: a member who wants a coach.
--
-- So the grant gets two-sided consent. The member asks; the coach (or a
-- manager on their behalf) answers; accepting creates the relationship IN THE
-- SAME TRANSACTION, so there is never a moment where a request is accepted but
-- the pairing does not exist.
--
-- Resolved, never deleted, like every other relationship here. "Sara asked
-- Tariq and he said no" is a fact about the gym.

CREATE TABLE coaching_requests (
    id           UUID        PRIMARY KEY,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    athlete_id   UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    coach_id     UUID        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    status       TEXT        NOT NULL
        CHECK (status IN ('pending', 'accepted', 'declined', 'withdrawn')),
    message      TEXT        CHECK (message IS NULL OR length(message) <= 500),
    requested_at TIMESTAMPTZ NOT NULL,
    decided_at   TIMESTAMPTZ,
    -- Null for `withdrawn`: the athlete is already named by athlete_id, and
    -- repeating them here would invite code that reads decided_by as "the
    -- coach who answered" and gets it wrong for exactly that case.
    decided_by   UUID        REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Asking yourself is nonsense the domain already refuses; the database
    -- refuses too, because the app is not the only thing that can reach here.
    CONSTRAINT coaching_request_not_self CHECK (athlete_id <> coach_id),

    -- The status enum carries its own evidence in the domain. Mirror that:
    -- "declined, but nobody declined it and it never happened" must not be
    -- storable.
    CONSTRAINT coaching_request_status_evidence CHECK (
        (status = 'pending' AND decided_at IS NULL AND decided_by IS NULL)
        OR (status IN ('accepted', 'declined') AND decided_at IS NOT NULL AND decided_by IS NOT NULL)
        OR (status = 'withdrawn' AND decided_at IS NOT NULL)
    )
);

-- One outstanding request per pair. A member may ask again after a decline —
-- circumstances change, and a permanent block would be a strange thing for a
-- gym to enforce — but they may not spam the same coach with five pending
-- asks. Partial, so resolved rows never collide.
CREATE UNIQUE INDEX coaching_requests_one_pending
    ON coaching_requests (gym_id, athlete_id, coach_id)
    WHERE status = 'pending';

-- The two reads that exist: "what is waiting for me?" (a coach opening People)
-- and "who have I asked?" (a member checking back).
CREATE INDEX coaching_requests_for_coach_idx
    ON coaching_requests (gym_id, coach_id, requested_at DESC)
    WHERE status = 'pending';
CREATE INDEX coaching_requests_by_athlete_idx
    ON coaching_requests (gym_id, athlete_id, requested_at DESC);

-- Tenant-owned, so it lives under RLS like everything else with a gym_id
-- (ADR-0004). Defence in depth: the service layer already scopes every query.
ALTER TABLE coaching_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE coaching_requests FORCE ROW LEVEL SECURITY;
-- `app_current_gym()`, not a hand-rolled current_setting: the helper already
-- encodes the nullif(...,'') that migration 0004 added, because a committed
-- GUC resets to the empty string rather than NULL and the naive comparison
-- then silently matches nothing. Every other policy in the schema uses it.
CREATE POLICY coaching_requests_tenant_isolation ON coaching_requests
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

-- No DELETE, matching every other end-dated relationship: a resolved request
-- is history, and history is not tidied away.
--
-- The GRANT alone does NOT achieve that. Migration 0003 sets ALTER DEFAULT
-- PRIVILEGES so every new table hands gym_app full DML automatically, which
-- makes a narrow GRANT purely additive — the table would still carry DELETE.
-- Migration 0012 exists because four tables learned this the hard way. So:
-- grant what is wanted, then revoke what was assumed away.
GRANT SELECT, INSERT, UPDATE ON coaching_requests TO gym_app;
REVOKE DELETE ON coaching_requests FROM gym_app;
