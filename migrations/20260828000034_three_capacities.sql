-- Three capacities, not five: owner, trainer, member (ADR-0036).
--
-- `admin` and `head_coach` were rungs nobody in the product ever occupied. The
-- demo seeded neither, the console rendered them in a picker that could grant
-- them, and every authority question they answered is answered by `owner`:
--   admin      = owner minus the right to make other owners
--   head_coach = the catalogue, which ADR-0034 had just moved to it
-- Two rungs whose only distinguishing rights were "slightly less than owner"
-- are two more states every permission test has to cover for no user it serves.
--
-- What survives is the shape, not the ladder: capacities remain a SET on one
-- account (ADR-0014), so "trainer who also trains here" is still expressible,
-- which was always the point.

-- ---------------------------------------------------------------- live rows
--
-- Mapping, and it is not symmetrical:
--
--   admin      -> owner    an ESCALATION (gains the right to make owners)
--   head_coach -> trainer  a DEMOTION   (loses the catalogue)
--
-- Both are deliberate and both are the safer direction for their case. An
-- admin ran the gym; dropping their standing would lock the person who
-- administers a gym out of it, and being unable to fix that is worse than the
-- one extra right. A head coach coached; promoting them to owner to preserve
-- their catalogue rights would hand billing and settings to every senior coach
-- in every gym, which is a far larger grant than the one being replaced. They
-- keep coaching, and the catalogue goes to whoever runs the place.
--
-- Collisions first: the partial unique index allows one LIVE row per
-- (gym, user, capacity), so somebody holding both `admin` and `owner` would
-- collide on update. Revoke the redundant one rather than deleting it — the
-- audit trail is meant to answer "what could they do last Tuesday", and a
-- delete is the one thing that stops it.
UPDATE gym_capacities g
SET revoked_at = now()
WHERE g.revoked_at IS NULL
  AND g.capacity IN ('admin', 'head_coach')
  AND EXISTS (
    SELECT 1 FROM gym_capacities other
    WHERE other.gym_id = g.gym_id
      AND other.user_id = g.user_id
      AND other.revoked_at IS NULL
      AND other.capacity = CASE g.capacity
                             WHEN 'admin' THEN 'owner'
                             ELSE 'trainer'
                           END
  );

UPDATE gym_capacities
SET capacity = CASE capacity WHEN 'admin' THEN 'owner' ELSE 'trainer' END
WHERE revoked_at IS NULL
  AND capacity IN ('admin', 'head_coach');

-- ------------------------------------------------------------ the constraint
--
-- Scoped to LIVE rows on purpose. A revoked row is history: it records what
-- somebody held and when it was taken away, and rewriting those strings to
-- satisfy a constraint would be falsifying the trail this table exists to
-- keep. So the rule is "a capacity you currently hold must be one of the
-- three", which is the rule that actually matters — and Postgres re-checks it
-- on UPDATE, so un-revoking a historical `head_coach` row is refused rather
-- than quietly restoring a rung that no longer exists.
--
-- `Capacity::parse` is the third line of defence: it returns None for both
-- strings, so even a row that somehow survived grants nothing.
ALTER TABLE gym_capacities DROP CONSTRAINT gym_capacities_capacity_check;

ALTER TABLE gym_capacities
    ADD CONSTRAINT gym_capacities_capacity_check CHECK (
        revoked_at IS NOT NULL
        OR capacity IN ('owner', 'trainer', 'member')
    );
