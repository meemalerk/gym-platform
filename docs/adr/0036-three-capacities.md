# ADR-0036: Three capacities — owner, trainer, member

- **Status:** Accepted
- **Date:** 2026-08-28
- **Deciders:** Project author

## Context

ADR-0014 gave the gym five capacities: `owner`, `admin`, `head_coach`,
`trainer`, `member`. Two of them turned out to be furniture.

`admin` was "owner, minus the right to make other owners". `head_coach` was
"the catalogue" — a rung ADR-0024 created to hold the publishing half of
programme authoring, and which ADR-0034 then had authoring moved *to*. Between
them they carried exactly two distinguishing rights, both of the form "slightly
less than owner".

Nobody occupied either. The demo seeded neither; there was no account of either
kind anywhere in the product. The one place they reliably existed was the
standing picker in the console and the phone app, which offered them as choices
— and in fifteen verification suites, where a `head_coach` was provisioned
purely as *a second person senior enough to approve a version the owner wrote*.
That is a second owner wearing a different hat.

The cost was not the two rows in an enum. It was that every authority question
had five answers to check instead of three, `verify-nav.mjs` enumerated 32
capacity combinations instead of 8, and the ladder in front of an owner
promoting somebody listed five rungs of which two needed a sentence of
explanation each to distinguish.

## Decision

**Three capacities: `owner`, `trainer`, `member`.**

- `owner` — runs the gym: standing, settings, billing, the catalogue,
  programme publishing.
- `trainer` — coaches: reads the catalogue, assigns published programmes to
  their own clients, proposes exercises.
- `member` — trains here.

What survives untouched is the **shape**: capacities remain a *set* on one
account (ADR-0014), so "a trainer who also trains here" and "an owner who
coaches" are still expressible. That was always the point of a set, and it is
what lets `trainer` mean *coaching* rather than *seniority*.

`Capabilities::can_manage_gym` and `can_manage_catalogue` both now resolve to
`is_owner`. They are **kept as separate methods** rather than collapsed at every
call site: they answer different questions, a screen asking the catalogue
question should say so, and they would diverge again the moment a rung is added
between owner and trainer. The same reasoning applies to `manages`/`curates` in
the console.

## Consequences

**Existing rows are mapped, asymmetrically, and the asymmetry is the point.**

| Was | Becomes | Direction |
|-----|---------|-----------|
| `admin` | `owner` | escalation — gains the right to make owners |
| `head_coach` | `trainer` | demotion — loses the catalogue |

Each is the safer direction for its case. An admin ran the gym; dropping their
standing would lock the person who administers a gym out of it, and being
unable to fix that is worse than one extra right. A head coach coached;
promoting them to owner to preserve their catalogue rights would hand billing
and settings to every senior coach in every gym — a far larger grant than the
one being replaced. They keep coaching, and the catalogue goes to whoever runs
the place.

**Removed rungs are refused at three layers.** The API rejects the string; the
CHECK constraint stops a row reaching the table; and `Capacity::parse` returns
`None`, so even a row that somehow survived grants nothing. `parse` deliberately
does **not** map `admin` to `Owner` — it is what turns a database row into
authority, and silently upgrading a stale string there would grant the one right
ADR-0031 reserves.

**The CHECK is scoped to live rows.** `revoked_at IS NOT NULL OR capacity IN
(...)`. A revoked row is history — it records what somebody held and when it was
taken away — and rewriting those strings to satisfy a constraint would falsify
the trail this table exists to keep. Postgres re-checks on UPDATE, so
un-revoking a historical `head_coach` row is refused rather than quietly
restoring a rung that no longer exists.

**`OwnerIsOwnersToGive` is now unreachable and stays.** It guarded a non-owner
manager promoting somebody to owner; `admin` was the only such actor. It is kept
as defence in depth against a future rung being added without anybody
re-deriving the rule, and a domain test pins that a trainer attempting it is
refused as `NotPermitted` first — the guard nobody can reach sitting behind a
guard everybody hits.

**The trainer directory needed a real fix, not a rename.** Its query read
`WHERE gc.capacity IN ('trainer', 'head_coach')`, and the line it drew was
*coaching rungs* rather than *every rung that can coach* — owner and admin were
already excluded, because a list a member picks a coach from should not be
padded with whoever holds the keys. It is now `= 'trainer'`, which expresses the
same intent in one value. An owner who genuinely coaches holds `trainer` too and
appears on that basis. While in there, `AND gc.revoked_at IS NULL` was added: it
was missing, and an ex-trainer stayed browsable in the directory for ever.

**Fifteen verification suites changed**, and the edit was not mechanical. Where
a `head_coach` was a second approver it became a second **owner**; where it was
"staff, but not a manager, so this must be refused" — opening hours, class
timetables, plan prices, registration settings — it became a **trainer**.
Getting that backwards turns a refusal test into a permission test that passes
for the wrong reason, which is exactly what a blanket replace did on the first
attempt and what running them caught.

**`verify-nav.mjs` now checks 8 combinations rather than 32**, so its count fell
from 353 assertions to 119. That is the combinatorial space shrinking, not
coverage being dropped: it still checks *every* possible capacity set, which was
the property worth having.

## Alternatives considered

**Keep them, unused.** Rejected: an authority model is checked by reading it,
and two rungs that exist only in a picker are two more things every reader has
to hold and every test has to cover, for no user they serve.

**Map `head_coach` to `owner` to preserve catalogue rights.** Rejected above —
it grants billing and settings to solve a catalogue problem.

**Keep `admin` and drop only `head_coach`.** Tempting, since `admin` at least
names a real job. But its only distinguishing right is *not* being able to make
owners, and a gym that needs that distinction needs a permissions system rather
than one more rung. If it returns, it returns as an ADR with a case behind it.
