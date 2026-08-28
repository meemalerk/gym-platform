-- Let a person read the gyms they belong to, not just the one they are acting in.
--
-- `gyms_read` only permits `id = app_current_gym()`, which is correct for
-- tenant-scoped work but makes a gym switcher impossible: to list "your gyms"
-- the client needs their names, and that query spans gyms by definition.
--
-- This is the same shape as the existing `gym_capacities_read` policy, which
-- already allows `user_id = app_current_user_id()` for exactly this reason.
-- Membership is the grant: you can read a gym's name precisely because you hold
-- a live capacity in it — not because you asked nicely.

CREATE POLICY gyms_read_member ON gyms
    FOR SELECT
    USING (
        EXISTS (
            SELECT 1
            FROM gym_capacities c
            WHERE c.gym_id = gyms.id
              AND c.user_id = app_current_user_id()
              AND c.revoked_at IS NULL
        )
    );

-- Note: multiple permissive SELECT policies are OR-ed, so `gyms_read` (active
-- tenant) and this one (any gym you belong to) compose. A gym you do not belong
-- to remains invisible under both.
