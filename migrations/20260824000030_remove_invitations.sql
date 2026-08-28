-- Invitations are gone (ADR-0031).
--
-- Staff used to exist only by invitation: a manager posted an address, the
-- server minted a single-use token bound to it, and redeeming it granted the
-- capacities named in the invitation. That machinery was three tables' worth
-- of care — hashed tokens, an anti-probing 404, a conditional UPDATE so two
-- concurrent redemptions could not both grant — in service of a flow nobody
-- could complete without leaving the app to fetch a code from an email the
-- deployment does not yet send (there is an `EmailSender` port and no adapter
-- behind it).
--
-- What replaces it is smaller and, more to the point, finishable: everybody
-- walks through the open door as a `member` (ADR-0026), and somebody who runs
-- the gym changes what they hold from the roster
-- (`PUT /gyms/{id}/members/{user}/capacities`). The rules that mattered
-- survive in `check_standing_change` — only an owner may grant or remove
-- owner, and the last owner cannot demote themselves.
--
-- The table is dropped rather than left behind. A dead table with live RLS
-- policies is a thing the next person has to work out the status of, and
-- there is nothing in it worth keeping: an accepted invitation already left
-- its mark in `gym_capacities` and in the audit log, and a pending one was
-- only ever a link somebody could still click.

DROP POLICY IF EXISTS gym_invitations_tenant ON gym_invitations;
DROP POLICY IF EXISTS gym_invitations_redeem ON gym_invitations;

DROP TABLE IF EXISTS gym_invitations;

-- Migration 20260823000019 still names `scripts/verify-invitations.sh` in a
-- comment. It stays wrong on purpose: an applied migration is immutable —
-- sqlx checksums the file, and editing even a comment makes every server
-- refuse to boot with "previously applied but has been modified". The suite
-- it means is `scripts/verify-capacities.sh`.
--
-- `app.invitation_hash` was the GUC the redeem policy keyed on. Nothing sets
-- it any more; it is a session variable rather than a schema object, so there
-- is nothing to drop — noted here so a search for it lands somewhere.

COMMENT ON TABLE gym_capacities IS
    'What each person holds in each gym. Since ADR-0031 this is written by two '
    'paths only: joining through the open door (member, always) and a manager '
    'changing somebody''s standing. Revoked rows are stamped, never deleted, so '
    'the audit trail can still answer what somebody could do last Tuesday.';
