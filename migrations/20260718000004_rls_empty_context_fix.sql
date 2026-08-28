-- Fix: tenant context must tolerate an EMPTY STRING, not just NULL.
--
-- Found by scripts/verify-rls.sh. After a transaction-scoped
-- `set_config('app.current_gym', ..., true)` commits, the setting does not become
-- unset — it reverts to its reset value, the empty string. `''::uuid` then raises
--
--     ERROR: invalid input syntax for type uuid: ""
--
-- so the next query on that pooled connection ERRORS instead of returning no
-- rows. Under a connection pool that is every connection that has ever served a
-- request. Still fail-closed (an error yields no data), but it turns a clean
-- empty result into a 500.
--
-- `nullif(..., '')` maps both "never set" and "reset to empty" to NULL, and
-- `gym_id = NULL` is NULL — invisible. Fail-closed without the exception.

CREATE OR REPLACE FUNCTION app_current_gym() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT nullif(current_setting('app.current_gym', true), '')::uuid
$$;

CREATE OR REPLACE FUNCTION app_current_user_id() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT nullif(current_setting('app.current_user', true), '')::uuid
$$;

GRANT EXECUTE ON FUNCTION app_current_gym()     TO gym_app;
GRANT EXECUTE ON FUNCTION app_current_user_id() TO gym_app;

-- Recreate every policy in terms of the helpers.

DROP POLICY IF EXISTS exercises_tenant_isolation ON exercises;
CREATE POLICY exercises_tenant_isolation ON exercises
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

DROP POLICY IF EXISTS gym_capacities_read  ON gym_capacities;
DROP POLICY IF EXISTS gym_capacities_write ON gym_capacities;

CREATE POLICY gym_capacities_read ON gym_capacities
    FOR SELECT
    USING (gym_id = app_current_gym() OR user_id = app_current_user_id());

CREATE POLICY gym_capacities_write ON gym_capacities
    FOR ALL
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

DROP POLICY IF EXISTS gym_invitations_tenant ON gym_invitations;
DROP POLICY IF EXISTS gym_invitations_redeem ON gym_invitations;

CREATE POLICY gym_invitations_tenant ON gym_invitations
    USING      (gym_id = app_current_gym())
    WITH CHECK (gym_id = app_current_gym());

CREATE POLICY gym_invitations_redeem ON gym_invitations
    FOR SELECT
    USING (
        token_hash = decode(nullif(current_setting('app.invitation_hash', true), ''), 'hex')
    );

DROP POLICY IF EXISTS gyms_read   ON gyms;
DROP POLICY IF EXISTS gyms_insert ON gyms;

CREATE POLICY gyms_read ON gyms
    FOR SELECT
    USING (id = app_current_gym());

CREATE POLICY gyms_insert ON gyms
    FOR INSERT
    WITH CHECK (true);
