# Delivery Stages

> The work, broken into stages that each leave the product **working and demonstrable**. This is
> the "what order and why" document. [roadmap.md](roadmap.md) holds the authoritative *current
> phase* marker; [feature-plan-2026-07.md](feature-plan-2026-07.md) maps individual requested
> capabilities to their stage. This document is the shape of the whole thing.

## The rule that decides the order

**Each stage must end with something a person can be shown.** Not "the schema is ready" — a
screen, a workflow, a refusal that makes sense. That constraint is what keeps a half-finished
data model from being mistaken for progress.

The second rule: **one at a time**. Each of these has a schema, an authorization story, a test
suite and a UI. Half-finishing four produces a codebase where nothing can be trusted.

---

## Stage 0 — Foundations ✅

*Nothing to demo. The only stage allowed to end without a screen.*

| | |
|---|---|
| **Delivers** | Cargo workspace, layering, Postgres, migrations, config, health checks, OpenAPI |
| **Proves** | The dependency direction holds and the thing boots |
| **Exit** | `cargo run` serves `/health`; migrations apply from empty |

## Stage 1 — Identity, tenancy, access ✅

| | |
|---|---|
| **Delivers** | Sign-up, login, rotating refresh sessions, capacities, invitations, RLS, audit log |
| **Depends on** | Stage 0 |
| **Proves** | Two gyms cannot see each other — *and* the database enforces it independently |
| **Demo** | Sign up, invite someone, accept, watch the app change shape around their capacities |
| **Exit** | `verify-rls.sh`, `verify-capacities.sh` green; holding no capacity yields 404 |

The decisions that made this stage expensive and correct:
[ADR-0004](adr/0004-postgres-shared-schema-multitenancy.md) (shared schema + RLS),
[ADR-0014](adr/0014-identity-capacities-and-profiles.md) (one account, many capacities).

## Stage 2 — Programmes ✅ *(model and API; authoring UI landed later — see Stage 3.5)*

| | |
|---|---|
| **Delivers** | Exercise catalogue, programme → version → week → workout → prescription, lifecycle |
| **Depends on** | Stage 1 (who may author, who may approve) |
| **Proves** | A published version cannot be edited, enforced in the domain *and* by a trigger |
| **Demo** | Write a programme, submit, have someone else approve, publish, try to edit it |
| **Exit** | `verify-programs.sh`, `verify-program-immutability.sh` green |

## Stage 2.5 — Coaching relationships ✅

| | |
|---|---|
| **Delivers** | Gym-scoped, end-dated coach ↔ athlete pairs; per-athlete authority |
| **Depends on** | Stage 1 |
| **Proves** | A trainer sees their clients and nobody else's; ending a pair keeps the history |
| **Demo** | Pair a coach, see the roster change; end it, see access go without data going |

*The spine.* Deliberately built before assignment, because assignment's authorization question
("may you put *this* athlete on a programme?") is answered by the relationship.

## Stage 3 — Assignment and execution ✅

| | |
|---|---|
| **Delivers** | Assignments pinning a version, sessions, performed sets, rest timer, history |
| **Depends on** | Stages 2 + 2.5 |
| **Proves** | Only the athlete writes their own history; a replayed insert is a no-op |
| **Demo** | Assign a programme, log a workout on a phone, watch the estimated 1RM move |
| **Exit** | `verify-assignments.sh`, `verify-execution.sh` green |

Pulled forward from later phases because the data already existed: progress metrics, body
measurements, goals, and deterministic recommendations.

## Stage 3.5 — Programme authoring UI ✅

| | |
|---|---|
| **Delivers** | Create → weeks → workouts → prescriptions → lifecycle buttons → assign |
| **Depends on** | Stage 2's API |
| **Proves** | The product can create the thing it exists to create, without curl |
| **Demo** | Write and publish a programme entirely on a phone |

Out of numbered order on purpose: it was owed from Stage 2, not new scope. Until it existed,
every authoring endpoint was reachable only by script — which meant the competitive advantage
was, from a user's point of view, not built.

## Stage 4 — Billing and entitlements ✅

| | |
|---|---|
| **Delivers** | Plans with grants, subscriptions, invoices, payments, resolved entitlements |
| **Depends on** | Stage 1 (who may take money) |
| **Proves** | An issued invoice is never rewritten; a gym that sells nothing withholds nothing |
| **Demo** | Sell a plan, subscribe a member, part-pay an invoice, void one, watch access follow |
| **Exit** | `verify-billing.sh`, `verify-entitlements.sh` green |

The entitlement resolver was built **before** it gates anything real, deliberately: retrofitting
feature-gating is invasive, writing it in is one resolver.

## Stage 5 — Presentation ✅

| | |
|---|---|
| **Delivers** | The design system, both colour schemes, capacity-aware navigation, the demo package |
| **Proves** | WCAG AA across both schemes, asserted by script; one visual language, also asserted |
| **Demo** | Send someone a link; they open it on a phone with nothing installed |

---

## Not yet built — in the order they should be

### Stage 6 — Gym operating calendar
Hours, closures, the weekly pattern. Shape already decided in
[ADR-0015](adr/0015-gym-operating-calendar.md): a recurring weekly pattern plus dated overrides,
resolved by one function; `TIME` in the gym's IANA timezone, never `timestamptz`; no RRULE.
**Why next:** several later things (classes, reminders, "is the gym open") need one honest answer
to "when is this gym open", and building them first would produce three disagreeing answers.

### Stage 7 — Outbox worker, then reminders
The worker first, reminders as its first real consumer. State change and event insert in one
transaction; a worker drains it. Postgres-backed queue to start — NATS only when there are
genuinely independent consumers.
**Why in this order:** a reminder built without an outbox becomes a cron job that reads tables,
and that is the thing that is hard to remove later.

### Stage 8 — Offline sync transport
The idempotent write path already exists ([ADR-0008](adr/0008-offline-sync-operation-log.md));
the queue does not. Evaluate PowerSync/WatermelonDB before hand-rolling transport.
**Why not sooner:** the primitive that makes it *possible* (client-generated ids, idempotent
inserts) was built in Stage 3, so this is transport, not a redesign.

### Stage 9 — Dunning and suspension
The entitlement resolver is the one place this changes. Currently an owing member still trains,
pinned by a test so the change is deliberate.
**Why last of the billing work:** it is a policy decision about people's access to a gym they may
have paid for, and it wants a real customer's opinion more than it wants an engineer's.

### Stage 10 — Nutrition guidance
Coach-authored only, per [ADR-0016](adr/0016-nutrition-scope.md). No generated plans, no calorie
targets keyed on a medical condition.

### Deferred indefinitely, on purpose
- **OpenFGA** — application-layer authorization is sufficient at this scale
  ([ADR-0013](adr/0013-mvp-application-layer-authorization.md)). Revisit when relationships get
  genuinely graph-shaped.
- **The AI surface** — the authority model is decided ([ADR-0007](adr/0007-ai-authority-levels.md))
  and the deployment shape is decided ([ADR-0011](adr/0011-self-hosted-open-llm.md)). Neither is
  built, because the deterministic systems it would sit in front of are the actual product.

---

## What "done" means for a stage

1. The feature works against a live server and database, proven by an executable script.
2. Its authorization story is explicit and tested from both sides — allowed *and* refused.
3. The invariants are enforced twice where the data matters: domain and database.
4. There is a screen. If a person cannot reach it, it is not delivered.
5. The seed demonstrates it, including its interesting failure states
   ([ADR-0022](adr/0022-seed-demonstrates-the-rules.md)).
6. Anything knowingly left undone is written down in
   [feature-plan-2026-07.md §11](feature-plan-2026-07.md), not remembered.
