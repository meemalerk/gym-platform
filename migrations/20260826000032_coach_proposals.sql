-- Who raised a coaching request — because the answer decides who may answer it.
--
-- Until now there was one direction: a member asked a coach. A manager pairing
-- a trainer with a member did it directly, creating an active relationship with
-- nobody's consent but their own.
--
-- The gym wants the other direction as well: the owner PROPOSES a trainer for a
-- member, and the trainer accepts. Both directions are the same row — an
-- athlete, a coach, a pending decision — and the only thing that differs is who
-- started it. So this is one column, not a second table.
--
-- It is load-bearing rather than informational: for a manager-raised proposal,
-- ONLY the named coach may answer. Without knowing who raised it, a manager
-- could accept their own proposal on the trainer's behalf, which is not a
-- handshake — it is the old direct pairing with extra steps.

ALTER TABLE coaching_requests
    ADD COLUMN raised_by UUID REFERENCES users(id);

-- Existing rows were all member-raised (that was the only path), so the athlete
-- is the honest backfill. Done before the NOT NULL so no row is invented.
UPDATE coaching_requests SET raised_by = athlete_id WHERE raised_by IS NULL;

ALTER TABLE coaching_requests
    ALTER COLUMN raised_by SET NOT NULL;

COMMENT ON COLUMN coaching_requests.raised_by IS
    'Who started it. Equal to athlete_id when the member asked; a manager''s id '
    'when the gym proposed a coach. A manager-raised request may be answered '
    'ONLY by coach_id — see CoachingRequest::may_answer.';

-- "Proposals waiting on this trainer" is the trainer's inbox, and it is the one
-- query the new direction adds. Partial, like the existing pending indexes.
CREATE INDEX coaching_requests_proposals_for_coach
    ON coaching_requests (gym_id, coach_id, requested_at DESC)
    WHERE status = 'pending' AND raised_by <> athlete_id;
