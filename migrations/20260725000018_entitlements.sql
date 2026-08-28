-- What a plan actually confers.
--
-- subscriptions-billing.md draws the line this migration implements:
--
--     payment event → reconcile → ENTITLEMENT → feature gating
--
-- Gating reads entitlements, never payment-provider state, so a membership
-- granted by a card charge, an in-app purchase or an admin comp all look the
-- same to the code that asks "may this person do this?".
--
-- Entitlements themselves are RESOLVED, not stored: a row saying "entitled
-- until the 30th" is wrong on the 31st unless something rewrites it, and that
-- something is a nightly job — the same second-source-of-truth this codebase
-- refuses for "overdue". So the only new state is the mapping from a plan to
-- the features it grants, which a gym owner genuinely chooses.

ALTER TABLE membership_plans
    ADD COLUMN grants TEXT[] NOT NULL DEFAULT ARRAY['gym_access']::TEXT[];

-- Every element must be a feature the platform knows about. A typo here would
-- otherwise become an entitlement nobody holds and no error anyone sees.
ALTER TABLE membership_plans
    ADD CONSTRAINT membership_plans_grants_known CHECK (
        grants <@ ARRAY['gym_access', 'coached_programming', 'class_credits']::TEXT[]
    );

COMMENT ON COLUMN membership_plans.grants IS
    'Features this plan confers. Resolved into entitlements at read time; never '
    'stored per member, because an entitlement that depends on today cannot be '
    'stored without a job to keep it true.';
