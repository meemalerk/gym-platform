-- Row-level security: the second layer of ADR-0004's defence in depth.
--
-- The application already filters every tenant-owned query by gym_id. This makes
-- the DATABASE enforce it too, so a cross-tenant leak needs two independent
-- failures rather than one forgotten WHERE clause.
--
-- THREE THINGS MAKE OR BREAK THIS (docs/research-2026.md §5):
--
-- 1. The app must connect as a role that does NOT own these tables and is NOT
--    superuser. RLS is bypassed for table owners and superusers — connect as the
--    owner and every policy below is silently inert.
-- 2. Tenant context must be set with SET LOCAL / set_config(..., true), i.e.
--    TRANSACTION-scoped. Session-scoped SET leaks across a connection pool: the
--    next request to borrow that connection inherits the previous tenant.
-- 3. Unset context must fail CLOSED. current_setting(..., true) returns NULL when
--    unset, and `gym_id = NULL` is NULL (not true), so rows are invisible. A
--    query that forgets to set the tenant returns nothing rather than everything.

-- --------------------------------------------------------------- app role

-- NOINHERIT so it cannot pick up privileges from any role it may be granted.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gym_app') THEN
        CREATE ROLE gym_app LOGIN PASSWORD 'gym_app_dev_password' NOINHERIT;
    END IF;
END
$$;

GRANT USAGE ON SCHEMA public TO gym_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO gym_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO gym_app;

-- Tables created by future migrations must also be reachable.
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO gym_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO gym_app;

-- The migration bookkeeping table is not tenant data; the app never touches it.
REVOKE ALL ON TABLE _sqlx_migrations FROM gym_app;

-- ------------------------------------------------------ tenant-owned tables

-- exercises: strictly scoped to the active gym.
ALTER TABLE exercises ENABLE ROW LEVEL SECURITY;
ALTER TABLE exercises FORCE ROW LEVEL SECURITY;

CREATE POLICY exercises_tenant_isolation ON exercises
    USING      (gym_id = current_setting('app.current_gym', true)::uuid)
    WITH CHECK (gym_id = current_setting('app.current_gym', true)::uuid);

-- gym_capacities is read two legitimate ways:
--   a) "who is in THIS gym"      → gym_id matches the active gym
--   b) "which gyms am I in"      → user_id matches the acting user (powers /me,
--                                   which is intentionally cross-gym)
-- Writes are restricted to the active gym only: you may never grant yourself a
-- capacity in a gym you are not currently acting in.
ALTER TABLE gym_capacities ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_capacities FORCE ROW LEVEL SECURITY;

CREATE POLICY gym_capacities_read ON gym_capacities
    FOR SELECT
    USING (
        gym_id  = current_setting('app.current_gym',  true)::uuid
        OR user_id = current_setting('app.current_user', true)::uuid
    );

CREATE POLICY gym_capacities_write ON gym_capacities
    FOR ALL
    USING      (gym_id = current_setting('app.current_gym', true)::uuid)
    WITH CHECK (gym_id = current_setting('app.current_gym', true)::uuid);

-- gym_invitations: belong to the inviting gym. An invitee also needs to look up
-- their own pending invitation by token before they have any gym context, so
-- that path is handled by a dedicated policy keyed on the token hash.
ALTER TABLE gym_invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE gym_invitations FORCE ROW LEVEL SECURITY;

CREATE POLICY gym_invitations_tenant ON gym_invitations
    USING      (gym_id = current_setting('app.current_gym', true)::uuid)
    WITH CHECK (gym_id = current_setting('app.current_gym', true)::uuid);

-- Redeeming an invite: the caller proves knowledge of the token by presenting its
-- hash. Without a matching hash this policy grants nothing.
CREATE POLICY gym_invitations_redeem ON gym_invitations
    FOR SELECT
    USING (
        token_hash = decode(
            coalesce(nullif(current_setting('app.invitation_hash', true), ''), ''),
            'hex'
        )
    );

-- gyms: readable when it is the active gym. Creating a gym happens before any
-- gym context exists, so inserts are allowed and the application controls them.
ALTER TABLE gyms ENABLE ROW LEVEL SECURITY;
ALTER TABLE gyms FORCE ROW LEVEL SECURITY;

CREATE POLICY gyms_read ON gyms
    FOR SELECT
    USING (id = current_setting('app.current_gym', true)::uuid);

CREATE POLICY gyms_insert ON gyms
    FOR INSERT
    WITH CHECK (true);

-- ------------------------------------------------- deliberately NOT under RLS
--
-- users, sessions, athlete_profiles, trainer_profiles.
--
-- These are person-owned, not tenant-owned, and the critical paths run BEFORE any
-- tenant context exists: login looks a user up by email; refresh looks a session
-- up by token hash. A gym-scoped policy on them would break authentication, and a
-- user-scoped one cannot help when there is no authenticated user yet.
--
-- They are protected by the application layer instead: sessions are only ever
-- reachable via a 256-bit token hash, and profile access is gated on capacities.
-- Documented here so the omission reads as a decision, not an oversight.
