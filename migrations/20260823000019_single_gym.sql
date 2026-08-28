-- Single-gym deployment (ADR-0023, supersedes the multi-gym-per-account part of
-- ADR-0014). A single-gym DEPLOYMENT serves exactly one gym — but the tenancy
-- ENGINE (gym_id, RLS, TenantScope) stays fully multi-gym-capable, because the
-- verification suites deliberately create several gyms to prove tenant
-- isolation (scripts/verify-rls.sh, scripts/verify-invitations.sh). So this is
-- a per-deployment policy, not a hard schema limit: `PgGymRepository` sets
-- `app.single_gym_mode` from `Config::single_gym_mode` (env `SINGLE_GYM_MODE`,
-- off by default — see its doc comment) before every gym creation, and this
-- trigger only enforces the cap when that flag is on.
--
-- The EXISTS check still needs SECURITY DEFINER regardless: `gyms_read` only
-- shows the ACTIVE gym (USING (id = app_current_gym())), so even with the flag
-- on, the gym_app role could not see whether a *different* gym already exists
-- without running as its owning (superuser) role.
CREATE OR REPLACE FUNCTION enforce_single_gym()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
BEGIN
    IF current_setting('app.single_gym_mode', true) = 'true' AND EXISTS (SELECT 1 FROM gyms) THEN
        RAISE EXCEPTION 'a gym already exists; this deployment serves a single gym'
            USING ERRCODE = 'raise_exception';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER gyms_singleton
    BEFORE INSERT ON gyms
    FOR EACH ROW EXECUTE FUNCTION enforce_single_gym();
