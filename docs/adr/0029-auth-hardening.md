# ADR-0029: Password reset, email verification, and a login throttle

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Refines:** the auth model from Phase 1. Nothing there is reversed — Argon2id, rotating
  refresh tokens with reuse detection and hash-only storage all stand. This adds the three
  things that were missing around them.

## Context

Three holes, found together because they share a shape.

1. **A forgotten password was permanent.** There was no reset of any kind. The only
   recovery was a database edit by an operator, which is not a recovery.
2. **Sign-up accepted any address** without ever checking that anyone could read it. That
   matters more now: [ADR-0026](0026-open-registration.md) lets strangers join a gym.
3. **The login endpoint counted nothing.** Passwords could be tried as fast as the network
   allowed. Worse than the guessing itself, Argon2id is deliberately expensive by design —
   so an unthrottled login endpoint is also a very cheap way to exhaust the server's CPU.

None of the three could be built while the platform had no way to send an email at all —
invitation tokens were handed to the *inviter* to relay by hand.

## Decision

### One table for both single-use secrets

Password reset and email verification are the same object: a hashed, expiring, single-use
token bound to one account and one address. They share `auth_tokens`, distinguished by
`purpose`. Two tables would be two copies of the same bugs, and the invitation table
already proved this shape works — so this follows it deliberately rather than inventing a
third variation.

The rules are inherited wholesale and are not negotiable: **only the hash is stored**, the
token is bound to the address it was sent to (so a forwarded link is useless, and a link
that predates an email change stops working), and consumption is a **compare-and-swap** —
the same discipline as refresh rotation, because a read-then-write lets two concurrent
redemptions both set a password from one email.

### Refusals never confirm who exists

`POST /auth/forgot-password` returns **202 whether or not the address is registered**, and
`ApplicationError::InvalidToken` covers unknown, expired, already-used and wrong-purpose
identically. Both follow the same reasoning as invitation redemption: an endpoint that
answers differently is a membership oracle, and this one would be an especially good
one — post an address, read the status, learn whether that person trains here.

The screen says *"if that address has an account, a link is on its way"* for the same
reason, and the copy carries a comment saying so, because it will otherwise be "improved"
into something more helpful and less safe.

### A reset ends every session

Changing a password is what someone does when they believe it is compromised. Leaving the
attacker's existing refresh token alive would make the reset theatre, so
`complete_password_reset` revokes every session for that account and invalidates every
other outstanding reset link — including one an attacker requested.

### Verification is not a gate

`users.email_verified_at` is recorded and **does not block sign-in**.

Locking someone out of a gym they already pay for because a confirmation email went to
spam is a worse product than one that tolerates an unconfirmed address. What the flag is
for: knowing whether outbound mail is worth sending, and giving an owner an answer when
someone never replies. If a gym later wants to gate open self-registration on it, that is a
policy decision with a place to hang — but it is not the default, and it should not become
one by accident.

### The throttle counts two things, in the database

Failures are counted per **address** and per **origin**, both within a 15-minute window:

- **Per address, 10.** The one that matters, and the one that cannot be forged. Generous
  on purpose — someone cycling through their three usual passwords must not be locked out.
- **Per origin, 50.** Higher, because a whole gym behind one office NAT shares an address
  and a Monday morning of forgotten passwords must not lock the building out. Derived from
  `x-forwarded-for` when present, which a client *can* forge — which is exactly why it is
  the weaker of the two and never the only one.

In the database rather than in memory, for two reasons that both matter: a restart must not
reset an attacker's budget, and a second API instance must not double it.

Three properties worth stating because they are each one line of code and each closes a
bypass: the check runs **before** the password is verified (no timing signal, no Argon2
work for a locked-out caller); a throttled attempt is **still recorded** (hammering a locked
account extends the lock rather than waiting it out for free); and the email is
**lowercased before it becomes the key** (otherwise changing the case resets the counter).

A lock refuses the *correct* password too. A throttle that lets the right password through
is one an attacker walks past on the guess that matters.

### Mail is recorded, not sent

There is no SMTP adapter. There are no credentials, and inventing a provider integration
nobody has signed up for would be worse than saying so.

`RecordingEmailSender` writes every message to `sent_emails` and logs the subject — never
the body, because a body can contain a live credential and logs get shipped somewhere. That
buys three things a silent no-op would not: the demo can show the mail it *would* have sent
including the reset link; the verification suite can read the link it is supposed to follow,
which is what makes the whole flow testable without an account anywhere; and "did the reset
email go out?" has an answer.

**The uncomfortable part, stated plainly:** a live reset link therefore sits in a table for
as long as the row does. That is acceptable only because those tokens are single-use and
expire within the hour. A real deployment wants a retention job on `sent_emails`, and
there is not one yet.

## Consequences

**Good.** An account is recoverable. Guessing is bounded and costly. The `EmailSender` port
now exists with real call sites, so the SMTP adapter is a single new implementation rather
than a feature — and the outbox ([ADR-0027](0027-outbox-and-worker.md)) already produces
the events that would want it.

**Cost.** One insert per login attempt, on a table nothing else reads. Four new tables. A
throttle that can, in principle, be used to lock a known address out for fifteen minutes at
a time — a real and accepted trade, mitigated by the limit being high enough that reaching
it accidentally is unlikely, and by the reset flow being available to the actual owner.

**Deliberately not done.** No CAPTCHA and no progressive delay: both add a dependency or a
held connection for a threat this size. No "change password while signed in" — that wants
the current password as well, and is a different flow.

**Verified by** `scripts/verify-auth-hardening.sh` (36 assertions), covering the oracle
behaviour of the request endpoint, single-use and cross-purpose token reuse, session
revocation on reset, that verification gates nothing, that the throttle refuses the correct
password once tripped, that it does not spill onto a neighbouring account, that case does
not reset it, and that the attempt log and the mail record are append-only.
