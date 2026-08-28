# Developer Guide

> For someone who has just been handed this repository. Read
> [product-specification.md](product-specification.md) first if you want to know *what* it is;
> this document is *how to work on it*.

---

## 1. Get it running in ten minutes

```bash
docker compose up -d postgres          # Postgres 17 on host port 5455
cp .env.example .env
cargo run --bin server                 # migrations apply at boot
bash scripts/seed-demo.sh http://127.0.0.1:8080
```

Then `http://localhost:8080/swagger-ui`.

For the phone: `bash scripts/dev-phone.sh` starts the API, Metro and a LAN proxy, and prints an
Expo Go URL. For a browser: `docker compose -f docker-compose.demo.yml up --build` and open
<http://localhost:8210>.

**Port 5455, not 5432**, because other projects on the original development machine already held
the usual ports. The same reasoning gives the API 8092 in `dev-phone.sh` and the demo 8210/8211.

## 2. The shape of the code

```
crates/
  domain/          pure rules. No IO, no SQL, no HTTP. Enums that make bad states unrepresentable.
  application/     use-cases. Orchestrates domain + ports. Owns authorization decisions.
  infrastructure/  adapters: Postgres, Argon2, JWT. Implements the ports.
  api/             HTTP only. Translate request → use-case → response. Nothing else.
bins/server/       the composition root: builds every adapter and hands them to services.
apps/mobile/       Expo app. Types are GENERATED from the backend's OpenAPI spec.
migrations/        sqlx migrations, embedded into the binary at compile time.
scripts/           the verification suite. See §6 — this is not optional scaffolding.
```

**Dependency direction is one-way**: `api → application → domain`, with `infrastructure`
implementing traits that `application` defines. If you find yourself importing `sqlx` into
`domain`, stop — the design has gone wrong, not the compiler.

### Why a modular monolith
One deployable, organised by business capability ([ADR-0003](adr/0003-modular-monolith.md)).
Microservices would buy independent scaling nobody needs and cost distributed transactions
everybody would feel.

## 3. The five things that will bite you

Each of these cost a real debugging session. They are not style preferences.

1. **Tenant context is not optional.** Every repository call takes it:
   `find_program(tenant, id)`, never `find_program(id)`. A missing tenant is a cross-gym leak.

2. **RLS only works if you connect as a non-owner role.** Postgres bypasses row-level security
   for table owners, silently. `APP_DATABASE_URL` must be the `gym_app` role. Tenant context is
   set **transaction-scoped** (`set_config(..., true)`) or it leaks across the connection pool.
   Policies use `nullif(...,'')` because a committed GUC resets to an empty string, not NULL.

3. **Serde bypasses your validating constructors.** `#[derive(Deserialize)]` builds a struct
   field by field and never calls `new()`. Anything validated in a constructor must ALSO be
   validated at the persistence path — this is why `ExercisePrescription::validate()` and
   `GoalMetric::validate()` exist and are called explicitly. The API once happily accepted
   99 sets because of exactly this.

4. **Migration 0003's `ALTER DEFAULT PRIVILEGES` makes GRANTs additive.** A new table is granted
   to `gym_app` automatically, so restricting one needs an explicit `REVOKE`. That is how the
   audit log stays append-only and performed sets stay insert-only.

5. **`sum()` over BIGINT returns NUMERIC in Postgres.** Cast with `::bigint` or sqlx demands a
   decimal feature you do not want.

## 4. Writing a feature, end to end

The order matters — each step constrains the next.

1. **Model it in `domain` first.** Can the invalid state be made unrepresentable? Prefer an enum
   carrying its evidence (`Published { at, by }`) over a nullable-field bag.
2. **Write the domain tests.** They are cheap, they run in milliseconds, and they are where the
   rule actually lives.
3. **Add the port** (a trait in `application/ports.rs`) and the use-case. Authorization decisions
   live here, not in the route.
4. **Implement the adapter** in `infrastructure`. Use `sqlx::query!` for commands,
   permission-sensitive reads, billing and workout mutations; a runtime builder for dynamic
   filtering and analytics.
5. **Add the route.** Thin. If there is a decision in your handler, it belongs one layer down.
6. **Enforce it twice where it matters.** The domain refuses *and* a database trigger refuses,
   because the application is not the only thing that can reach those tables.
7. **Write the verify script** (§6) before you call it done.
8. **Regenerate the mobile types**: restart the server, then `npm run codegen:api` in
   `apps/mobile`. Contract drift then fails at compile time rather than at runtime on a phone.

## 5. Authorization: where each question is answered

Getting this layering wrong is the most common way to introduce a security bug here.

| Question | Answered by | Example |
|----------|-------------|---------|
| May this actor *attempt* this at all? | `Capabilities::can_*` in `domain` | Only managers set prices |
| May they do it *to this person*? | the coaching relationship | A trainer assigns only to their own clients |
| Is the resulting state *valid*? | domain constructors and lifecycle | You cannot approve a draft |
| Is the tenant even visible? | `TenantScope` extractor → 404 | An outsider gets "not found", never "forbidden" |
| Is the row reachable? | RLS, as defence in depth | The database refuses too |

**Never inspect a raw capacity set.** `capacities.includes('owner')` scattered through the code
is how two places disagree about what an owner may do.

## 6. Verification-first

`bash scripts/all-check.sh` runs the whole gate. It is the definition of "working".

- **Unit tests** (`cargo test`) — domain rules.
- **`scripts/e2e.sh`** — the API over HTTP with RLS enforced.
- **`scripts/verify-*.sh`** — one per feature, asserting against a live server and database.
  These start their own server on their own port so they never kill your dev server.
- **`scripts/verify-*.mjs`** — pure client modules under `apps/mobile/src/features/*`, tested by
  plain Node with no device and no renderer.

**A bug becomes a pinned regression.** When something breaks, the fix includes the assertion that
would have caught it. That is how `verify-refresh-hazard.mjs` and the CORS preflight checks exist.

**Deliberate behaviour gets pinned too**, not just correct behaviour. An overdue member can still
train; there is a test saying so, so that the day suspension arrives, someone has to delete a test
that explains itself.

## 7. Working on the mobile app

- **Read the versioned Expo docs** for the exact SDK (see `apps/mobile/AGENTS.md`). Expo changes
  quickly and last year's answer is often wrong.
- **Versions are pinned by Expo SDK's `bundledNativeModules.json`, which is authoritative.**
  `npm outdated` will lie to you; `npx expo install --check` and `expo-doctor` are the arbiters.
- **Navigation comes from a data manifest** (`src/navigation/tabs.ts`), never a hand-written list
  in a layout. Add a tab there and `verify-nav.mjs` checks all 32 capacity combinations for free.
- **Hidden tabs are `Tabs.Protected`** (unmounted), not `href: null` (merely hidden), so "hidden"
  and "unreachable" mean the same thing.
- **The design system is enforced by script** (`verify-design-consistency.mjs`): square corners,
  one icon family, colours from tokens, no `fontWeight` (Archivo ships a file per weight).
- **Two client invariants**: refresh rotation is compare-and-swap on the server, and the client
  de-duplicates concurrent refreshes (`refreshOnce`). Without the second, two simultaneous 401s
  rotate the token twice and sign the user out of every device.

## 8. Conventions

- **Comments explain *why*, never *what*.** If a line needs explaining, the explanation is
  usually a reason, a trade-off, or a bug that once happened.
- **Commit messages are written to a file** and passed with `git commit -F` — inline messages get
  mangled by shell quoting, repeatedly.
- **Money is integers.** Currency formatting lives in `domain::format_money` so an invoice, a
  receipt and a dashboard cannot disagree.
- **Dates that mean "a wall-clock day"** are `DATE`; opening hours are `TIME` in the gym's IANA
  timezone, never `timestamptz` ([ADR-0015](adr/0015-gym-operating-calendar.md)).

## 9. When you finish something meaningful

- New hard choice → write an ADR in `docs/adr/`, link it from `CLAUDE.md` and `adr/README.md`.
- Phase boundary → move the "Current phase" marker in `roadmap.md`.
- New domain word → add it to `glossary.md`.
- Gap you are choosing not to close → write it into
  [feature-plan-2026-07.md §11](feature-plan-2026-07.md), so it is a decision rather than a
  surprise for the next person.
