-- Open registration: the owner-held switch that lets someone join without an
-- invite code (ADR-0026).
--
-- Before this, sign-up was a dead end. An account with no membership landed on
-- "paste your invite code" and had nothing else it could do — so a person who
-- downloaded the app and walked into the gym could not get in without staff
-- generating a token for them by hand. Meanwhile ADR-0023 had removed the old
-- "everyone gets a personal gym" escape hatch, and the sign-in screen was still
-- promising it.
--
-- Default FALSE, deliberately. A gym that has not thought about this should not
-- silently be accepting strangers; turning the door on is a decision an owner
-- makes, and the audit log records when they made it.

ALTER TABLE gyms
    ADD COLUMN open_registration BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN gyms.open_registration IS
    'When true, any authenticated account may join this gym as a plain member '
    '(never staff — see GymService::join). When false, an invitation is the '
    'only way in. Owner-controlled; default false.';

-- The discovery read: "which gym can I join?", asked by an account that holds
-- no membership at all and therefore has no gym_id to scope by. Tiny table, so
-- this index is about intent as much as speed — it marks the one query that is
-- deliberately allowed to run without tenant context.
CREATE INDEX gyms_open_registration_idx
    ON gyms (id)
    WHERE open_registration;

-- `gyms` has never been UPDATEd before: the row was written once at creation
-- and only ever read after that. So the table carries SELECT and INSERT
-- policies and no UPDATE policy at all — and under FORCE ROW LEVEL SECURITY,
-- "no policy" means "no rows", not "no restriction". The UPDATE silently
-- matched zero rows and the service reported the gym as missing.
--
-- Scoped to the active tenant exactly like `gyms_read`, so a manager can only
-- change the settings of the gym their request is already scoped to. The
-- *authorization* question — may this person change settings at all — stays in
-- `GymService::set_open_registration` (can_manage_gym), where it belongs;
-- this is the defence-in-depth half.
CREATE POLICY gyms_update ON gyms
    FOR UPDATE
    USING      (id = app_current_gym())
    WITH CHECK (id = app_current_gym());

-- Discovery needs a policy of its own, and this is the interesting one.
--
-- `open_for_registration` runs with NO tenant context, because the caller has
-- no membership — that is the state it exists to resolve. But FORCE ROW LEVEL
-- SECURITY applies to `gym_app` on a plain connection just as much as inside a
-- scoped transaction, and neither existing policy matches: `gyms_read` wants
-- `app.current_gym` set, `gyms_read_member` wants an existing capacity. With
-- no policy matching, the read returned zero rows and the open list looked
-- permanently empty.
--
-- So the rule is stated in the database rather than only in the WHERE clause:
-- a gym whose owner switched `open_registration` on has *published itself as
-- joinable*, and is readable by anyone authenticated. Everything else stays
-- invisible without context. The predicate is the access rule, which is why it
-- belongs here and not just in Rust.
CREATE POLICY gyms_read_open ON gyms
    FOR SELECT
    USING (open_registration);
