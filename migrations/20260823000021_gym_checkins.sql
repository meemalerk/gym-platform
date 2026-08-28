-- Gym entry: a member's QR pass, scanned by staff at the door.
--
-- The pass itself is never stored (a short-lived signed token, verified on
-- the fly — see crates/infrastructure/src/checkin_pass.rs). What IS stored is
-- every scan's outcome, allowed or not, in the same row shape either way: a
-- denial ("no active plan") is real operational data, not noise to discard.
--
-- Append-only, same reasoning and same mechanism as audit_log: `gym_app` gets
-- SELECT and INSERT only, so a bug or a compromised app role cannot rewrite
-- who was let in.

CREATE TABLE gym_checkins (
    id           UUID        PRIMARY KEY,
    gym_id       UUID        NOT NULL REFERENCES gyms(id) ON DELETE CASCADE,
    member_id    UUID        NOT NULL REFERENCES users(id),
    -- Whoever held the scanner. Never the member themselves — enforced in the
    -- application layer (`CheckInService::ensure_front_desk`), not here.
    scanned_by   UUID        NOT NULL REFERENCES users(id),
    allowed      BOOLEAN     NOT NULL,
    -- One line a screen can print verbatim — "Your Coaching membership" or
    -- "No active plan covers gym access."
    reason       TEXT        NOT NULL CHECK (length(reason) BETWEEN 1 AND 200),
    scanned_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The dominant reads: "who has come through this door recently" and "this
-- member's own entry history".
CREATE INDEX gym_checkins_gym_time_idx ON gym_checkins (gym_id, scanned_at DESC);
CREATE INDEX gym_checkins_member_idx ON gym_checkins (gym_id, member_id, scanned_at DESC);

-- Tenant isolation, same as every other tenant-owned table.
ALTER TABLE gym_checkins ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_checkins FORCE ROW LEVEL SECURITY;

CREATE POLICY gym_checkins_tenant ON gym_checkins
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

-- Append-only for the application role. The ALTER DEFAULT PRIVILEGES from the
-- RLS migration granted UPDATE/DELETE on new tables, so take them back here —
-- same pattern as audit_log and performed_sets.
REVOKE UPDATE, DELETE ON gym_checkins FROM gym_app;
GRANT SELECT, INSERT ON gym_checkins TO gym_app;
