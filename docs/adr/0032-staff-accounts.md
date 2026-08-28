# ADR-0032: An owner can create a staff account outright

- **Status:** Accepted
- **Date:** 2026-08-24
- **Extends:** [ADR-0031](0031-standing-not-invitations.md)

## Context

ADR-0031 removed invitations and replaced them with promotion: everybody walks
through the open door as a `member`, and a manager sets their standing from the
roster. The rules are right and the audit trail is clean.

It is also, in the one case that matters most, the wrong shape. Setting a gym up
means creating its staff, and the promotion flow asks the owner to do this:

1. Open the door in Settings.
2. Ask the trainer to download the app.
3. Wait for them to sign up.
4. Wait for them to find the gym and join.
5. Find them on the roster and promote them.

Steps 2–4 are somebody else's afternoon, and the owner is blocked on all of
them. Every product that manages staff lets the person who runs the place
create the account — because at the moment of hiring, the manager is the one
sitting in front of a computer.

There is a second, quieter problem underneath. An account created *for*
somebody starts on a password that somebody else chose, and this codebase has
no way off one: `set_password_hash` is reachable only from the reset flow, the
reset flow needs a link in an email, and the `EmailSender` port has no adapter
(ADR-0029). Shipping staff accounts without a way to change the password would
mean every trainer's password is one their manager typed and can still
remember.

## Decision

### `POST /gyms/{id}/staff` — create the account and the standing together

Manager-only, one transaction, both rows or neither: an account with no standing
is a person who cannot do anything and whom nobody can find on a roster to fix.

The rules are ADR-0031's, split so the creation path gets the half that applies.
`check_standing_grant` holds what is true with no "before" — the actor must be
allowed to set standing, the standing cannot be empty, and **only an owner may
create an owner**. `check_standing_change` now calls it and adds the rules that
need a current standing (the owner-transition rule, the last-owner rule).

That split is the point rather than tidiness: reusing the full check would have
meant passing a fake `current`, and a fake argument is how a rule quietly stops
applying. The owner rule in particular has to hold here, or "create a staff
account" becomes a way for an admin to mint an owner and sign in as it.

### An address that already has an account is a **409, not a merge**

Attaching somebody's existing account to a gym because a manager typed their
address would be a membership granted without their consent — precisely what
invitations existed to prevent, and the one property of them worth keeping. The
refusal says what to do instead: have them join through the door, then set their
standing from the roster. Two taps, and it involves them.

### The password is **generated**, and shown once

Not chosen by the creator. An owner picking a colleague's first password picks a
bad one, and picks the same one twice. It is cut from the same 256-bit CSPRNG
that mints refresh tokens, hashed with Argon2 like every other password, and
returned in the creation response and nowhere else — not stored in plaintext,
not retrievable, **not in the audit trail**. The audit records that an account
was created and what it holds, which is what an investigator needs, and not the
credential, which is what they must never find.

Both clients treat that response as a one-time handover: a card showing email,
password and standing, dismissed by a deliberate press rather than by the table
refreshing underneath it.

### `POST /auth/change-password` — the way off it

Requires the current password. Not a formality: an access token is a bearer
credential, and a stolen one must not be enough to lock the real owner out of
their own account.

Success revokes every session, exactly as a reset does, because from the
account's point of view it is the same event. Note what that does and does not
mean, since the suite now pins it: the **refresh** tokens die immediately; the
**access** token is a short-lived stateless JWT that is not checked against the
session table and keeps working until it expires. That is the trade ADR-0029
already made, and asserting otherwise would be asserting something this system
does not do.

## Alternatives considered

- **Let the owner type the password.** Simpler, and worse in the way that
  matters: the passwords would be guessable and reused, and the owner would then
  know a credential they have no business keeping.
- **Return a one-time sign-in link instead of a password.** Better UX, and it is
  the invitation token again under another name — single-use tokens, an expiry,
  a redeem endpoint, and a way to deliver it. The delivery problem is exactly
  what killed invitations.
- **Force a password change at first sign-in.** The right end state. It needs a
  `must_change_password` flag on the account and a gate in front of every
  authenticated route, which is a bigger change than this one and can land on
  top of it without rework. Today the prompt is a row on the You screen and a
  sentence on the handover card.
- **Merge into an existing account when the address matches.** Rejected above —
  it is a membership without consent, and it also turns the endpoint into an
  address-existence oracle for anyone who runs a gym.

## Consequences

- **Positive:** a gym can be set up by one person in one sitting. The promotion
  path from ADR-0031 is unchanged and remains correct for people who already
  have an account — the two now cover the two real cases instead of one flow
  covering both badly. Everybody can change their own password, which the
  product simply could not do before.
- **Negative / costs:** a password now travels through a screen and, probably, a
  chat message. That is a real weakening compared with a link only the recipient
  could open, and it is the price of the deployment not being able to send mail;
  the mitigation is that it is generated, single-purpose and changeable. There
  is no forced rotation yet, so a staff member who never changes it stays on a
  password their manager once saw.
- **New surface:** `GymService` gained a hasher and a token issuer — for staff
  accounts only. Both are the same shared instances `AuthService` uses, so there
  is one Argon2 configuration and one CSPRNG in the process, not two.

## Verification

`scripts/verify-capacities.sh` grows from 29 to **50 checks**, covering:

- an owner creates a staff account, the returned password **actually signs in**,
  and the standing is real (on the roster, in `/me`);
- the same address twice is a 409;
- a member and a head coach are both refused; an admin may create ordinary staff
  but **not an owner**;
- an empty standing, a malformed address and an unknown capacity are each a 400
  rather than a 500;
- the wrong current password is a 401, a short new one is a 400, a good change
  is a 204, the new password signs in, the old one does not, and the old
  session cannot be renewed;
- `staff.created` appears in the audit trail.

## References

- [ADR-0031](0031-standing-not-invitations.md) — the promotion path this extends
- [ADR-0029](0029-auth-hardening.md) — the reset flow, and the `EmailSender`
  port with no adapter that makes it unusable here
- [ADR-0014](0014-identity-capacities-and-profiles.md) — capacities as a set
