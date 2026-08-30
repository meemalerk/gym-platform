# Gym Platform

Multi-tenant coaching and gym-management platform: gyms, coaches and members share one
system; programmes are versioned and immutable once published; training history is
append-only and progress is computed from it.

**Just want to look at it?** [START-HERE.md](START-HERE.md) — install Docker, double-click
one file, the whole system runs in your browser. Written for someone with no technical
background; no toolchain, no database setup, no terminal.

**Start here (readers):**

- [docs/product-specification.md](docs/product-specification.md) — what this is, for whom, and what it deliberately is not
- [docs/developer-guide.md](docs/developer-guide.md) — how to run it, how it is laid out, how to add a feature
- [docs/delivery-stages.md](docs/delivery-stages.md) — the stages, what each one proves, what "done" means
- [ORIGIN.md](ORIGIN.md) — where this codebase came from and the prototype→product pivot
- [screenshots/README.md](screenshots/README.md) — the running system, every view per role
- [docs/adr/README.md](docs/adr/README.md) — nineteen architecture decision records
- [research/INDEX.md](research/INDEX.md) — the primary sources behind the design decisions

**Start here (contributors):** [CLAUDE.md](CLAUDE.md) is the working anchor — current
status, locked decisions, invariants; the plan lives in [docs/](docs/).

Status: **product development** — Phase 3 core complete (assignment & execution; offline
transport still owed); next up: the product UI redesign, then entitlements/billing.

## Stack

| Layer | Choice |
|-------|--------|
| Backend | Rust 1.97 (edition 2024), Axum 0.8, SQLx 0.9, PostgreSQL 17 |
| API docs | utoipa 5.5 → OpenAPI → generated TypeScript client |
| Mobile | Expo SDK 54, React Native 0.81.5, React 19.1 |
| Auth | Argon2id + short-lived access token + rotating refresh session |

See [docs/tech-stack.md](docs/tech-stack.md) (its Expo figures still say 57 — the mobile app
was downgraded to SDK 54 on 2026-08-22, see `CLAUDE.md`). Versions verified against the
registries on 2026-07-18.

## Run it without a toolchain

For demos, or for anyone who just wants to look at it:

```bash
docker compose -f docker-compose.demo.yml up --build
```

Postgres, the API, the seed data and the app (exported for the browser) all come up together;
open <http://localhost:8210>. Nothing else needs installing — no Rust, no Node, no database
setup. [START-HERE.md](START-HERE.md) is the same thing written for a non-technical reader,
with double-clickable launchers for Windows, macOS and Linux.

**To show it to someone who has nothing installed** — including on an iPhone — run
`bash demo/share.sh` (or double-click `share-demo.bat`). It opens a free Cloudflare quick
tunnel and prints a public `https://` link plus a QR code. One tunnel covers both the app and
the API because nginx proxies `/api` at the same origin, which is also why the bundle carries
no absolute API URL. The link dies when you stop it.

> **For your own iPhone**, Expo Go now works directly: the app runs on SDK 54, matching Expo
> Go's current App Store build. Run `npx expo start` in `apps/mobile` and scan the QR with
> Expo Go or the Camera app — same Wi-Fi network as this machine. The tunnel/demo route above
> is still the one to use for **someone else's** phone with nothing installed, or if the app
> is ever moved off SDK 54 again (Expo now positions Expo Go as a learning tool rather than a
> review surface, so a future SDK bump would need TestFlight instead).

To hand over the source instead: `bash demo/package.sh` writes a zip via `git archive`, so it
carries the tracked files only — no `target/`, no `node_modules/`, and no `.env`.

## Signing in

Nothing here has a public sign-up worth using: the door is owner-controlled and default
closed (ADR-0026), so use the seeded accounts. `bash scripts/seed-demo.sh` creates them,
the browser demo runs it for you, and it is idempotent — re-running it is a no-op, not a
second gym.

**The password for every account below is `demopassword`.** It is weak on purpose and only
ever meets a local server.

| Account | Holds | The app | The console | Worth signing in for |
|---------|-------|:-------:|:-----------:|----------------------|
| `owner@demo.test` | `owner` | yes | yes | Everything: roster, billing, the catalogue, the audit trail, publishing programmes |
| `trainer@demo.test` | `trainer` | yes | yes | Sees **only their own clients** — not the gym roster |
| `trainer2@demo.test` | `trainer` | yes | yes | The second trainer, and the reason the boundaries are visible: `trainer@`'s clients and class rosters are refused to them |
| `member@demo.test` | `member` | yes | — | The data-rich one: programme, history, goals, measurements, a booked class |
| `solo@demo.test` | `member` | yes | — | **No coach, Open Gym plan** — so Today offers *Start your own workout* (ADR-0035) |

Members can sign into the console, but there is nothing there for them — it is a staff
tool, and it says so.

- **The app** — the browser demo on <http://localhost:8210>, or Expo Go. Its sign-in
  screen carries a **row of one-tap buttons**, one per account, so there is nothing to
  type. They are build-time gated (`__DEV__`, or `EXPO_PUBLIC_DEMO_ACCOUNTS=true`, which
  only `demo/Dockerfile.web` sets), so a real release bundle contains neither the
  addresses nor the password.
- **The console** — <http://localhost:5174> after `npm run dev`. Same one-tap rows for
  the three staff accounts, gated on `import.meta.env.DEV`, so `vite build` strips them.

Full detail — what each account demonstrates, which tabs each one sees, and why there are
deliberately two trainers — is in [docs/test-accounts.md](docs/test-accounts.md).

## Prerequisites

- Rust 1.97 (pinned via `rust-toolchain.toml`)
- Docker (for Postgres)
- Node 22+ and npm (for the mobile app and the console; both carry a `package-lock.json`)

## Quick start

```bash
# 1. Start Postgres (host port 5455 — chosen to avoid clashing with other local DBs)
docker compose up -d postgres

# 2. Configure
cp .env.example .env

# 3. Apply migrations
cargo install sqlx-cli --no-default-features --features rustls,postgres --locked
export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"
sqlx migrate run --source ./migrations

# 4. Run
cargo run --bin server
```

Then:

- http://localhost:8080/health — liveness
- http://localhost:8080/ready — readiness (checks the database)
- http://localhost:8080/swagger-ui — API explorer
- http://localhost:8080/api-docs/openapi.json — OpenAPI document

**Step 3 is not optional the first time.** `sqlx::query!` macros are checked against the
live schema *at compile time*, so `cargo run` against a database with no tables fails to
**build** — a wall of `relation "programs" does not exist` errors — and never gets far
enough to apply its own migrations. Run the migrations first, or build against the
committed offline cache instead (`SQLX_OFFLINE=true cargo run --bin server`, which needs
no database at compile time).

Once the schema exists the server does apply pending migrations on boot, so later
migrations need no separate step.

## Development

```bash
cargo test --workspace                              # unit tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

**`DATABASE_URL` must be set to build**, because `sqlx::query!` macros are checked against
the live schema at compile time. To build without a database (e.g. in CI), commit the
offline query cache:

```bash
cargo sqlx prepare --workspace
SQLX_OFFLINE=true cargo check --workspace
```

## Mobile app (Expo SDK 54)

The member and floor-trainer client. Needs the API running (see **Quick start** above).

```bash
cd apps/mobile
npm install --legacy-peer-deps   # a transitive peer pulls in react-native-windows; see CLAUDE.md
npm run typecheck                # tsc --noEmit
npm start                        # Expo dev server — press w for web, i/a for a simulator
```

**On a physical phone**, scan the QR from `npm start` with **Expo Go** (App Store /
Play Store) — the app is on SDK 54, which matches Expo Go's current build, so no custom
dev client, Apple Developer account or EAS build is needed. The phone and this machine
must be on the same Wi-Fi. `expo-dev-client` is still installed for standalone builds
(`npx expo run:ios` / `run:android`) if you need one.

A physical device cannot reach the host's `localhost`, so point it at this machine's LAN
address:

```bash
EXPO_PUBLIC_API_URL=http://192.168.1.5:8080 npm start
```

Regenerate the API types whenever the backend contract changes (server must be running).
The mobile default is port **8092** — the layout `scripts/dev-phone.sh` uses — so point it
at a Quick-start server on 8080 with `API_PORT`:

```bash
API_PORT=8080 npm run codegen:api   # writes src/api/schema.d.ts from /api-docs/openapi.json
```

`src/api/gym.ts` **derives its types from that generated schema**, so a backend contract
change becomes a TypeScript error rather than a runtime failure on a phone.

## Console (web app for owners & coaches)

The React + Vite back-office client ([ADR-0009](docs/adr/0009-client-stack.md)) — billing
ledgers, rosters, catalogue review: the work a phone is worst at. Needs the API running.

```bash
# 1. Start the API on :8080 (the port the console's dev server proxies to)
cargo run --bin server

# 2. Start the console
cd apps/console
npm install
npm run dev                # http://localhost:5174 — proxies /api to :8080
```

Same-origin in development (Vite proxies `/api`) and in the demo (nginx does), so there is
**no absolute API URL and no CORS entry** to keep in step. Sign in with a seeded account
(`bash scripts/seed-demo.sh`, then [docs/test-accounts.md](docs/test-accounts.md)).

Keeping it honest:

```bash
npm run typecheck          # part of scripts/all-check.sh
npm run tokens             # regenerate src/tokens.css after a palette change
npm run codegen:api        # regenerate src/lib/schema.d.ts against a running API
```

`src/tokens.css` is **generated** from `apps/mobile/src/ui/theme.ts`; `all-check.sh`
regenerates it and fails if the checked-in file drifts, so the two clients cannot disagree
about colour or shape.

## Layout

```
crates/
  domain/          pure types + invariants (no I/O)
  application/     use-cases + ports (traits)
  infrastructure/  adapters: Postgres repos, Argon2 hashing, JWT issuing
  api/             Axum router, extractors, Problem Details errors, OpenAPI
bins/server/       entrypoint: config, wiring, graceful shutdown
migrations/        sqlx migrations (never edit an applied one — add a new one)
apps/mobile/       Expo SDK 54 app (expo-router, TanStack Query, Zustand)
apps/console/      React + Vite web app for owners & coaches (TanStack, shared tokens)
scripts/           verification suites (all-check.sh runs the lot) + seed-demo.sh
docs/              the plan; docs/adr/ holds the decisions
screenshots/       captures of every view, per role (see its README)
research/          cited primary sources as PDFs (see INDEX.md)
```

## Verification

One gate runs everything ([ADR-0019](docs/adr/0019-verification-first-development.md)):

```bash
bash scripts/all-check.sh
```

Around 1,450 assertions. First the Rust gate — `cargo fmt`, `cargo clippy --all-targets
-D warnings`, the sqlx offline cache, and 283 workspace unit tests. Then 57 e2e HTTP
assertions (which pin the published OpenAPI path count at 79, so adding a route is a
conscious edit) and the per-feature suites against a live server and database: standing
47, RLS 7, audit 17, programme immutability 22, programme authoring 59, coaching 52,
coaching requests 38, assignments 31, execution 39, unplanned sessions 42, profiles 38,
goals 20, recommendations 15, billing 62, entitlements 35, check-ins 29, trainer
authority 36, open registration 30, athlete view 25, worker 31, payments 36, auth
hardening 36, calendar 51, classes 51. Then the pure-logic node suites, which need
neither server nor renderer: navigation 119, routing 39, attendance 44, activity 42,
progress 35, timetable 33, prescription rendering 33, today 31, plates 18, entitlement
wording 22, session naming 12, palette contrast 100, design consistency 10, doc links 5.
Finally `tsc --noEmit` for both clients, and a check that `apps/console/src/tokens.css`
still regenerates identically from the mobile theme.

**`expo-doctor` and a real `expo export` bundle run in CI, not in this gate** — they cost
minutes and need a network.

**On Windows, running this stops any dev server you have running.** Each suite reaps stray
`server.exe` processes before building, because a running binary holds
`target/debug/server.exe` open and the next `cargo build` would silently keep a stale one.
The reap matches by image name, so it cannot tell your dev server from a leftover.

Suites build before running — a stale binary makes results meaningless. Negative cases
(what must be *refused*) are asserted as deliberately as happy paths. CI
(`.github/workflows/ci.yml`) runs a **subset**, not the lot: fmt/clippy/tests, e2e, RLS,
standing, audit, the refresh-rotation race, and the mobile typecheck/doctor/bundle. Demo
data: `bash scripts/seed-demo.sh`, then see [docs/test-accounts.md](docs/test-accounts.md).

### Running the app in a browser (no simulator needed)

The web target is a **development/verification convenience only** — the production client is
native, and per [ADR-0009](docs/adr/0009-client-stack.md) the real browser app is a separate
React + Vite project.

```bash
# API must allow the web origin explicitly (CORS is closed by default)
CORS_ALLOWED_ORIGINS=http://localhost:8210 cargo run --bin server   # :8080

cd apps/mobile
EXPO_PUBLIC_API_URL=http://localhost:8080 npx expo start --web --port 8210
```

**WSL note:** Metro binds IPv6 (`::`), and WSL2's `localhostForwarding` only creates a relay
for IPv4 binds — so the Windows browser cannot reach it. Put an IPv4 listener in front
(`scripts/ipv4-proxy.mjs`). Also check your ports: on this machine 8080 is taken by a Windows
service and 8081 by another project's Metro.

On web, the refresh token falls back to `sessionStorage` (see `src/session/secure-storage.ts`)
because `expo-secure-store` does not exist on web. That is **not** a production-safe pattern —
a real browser client must use the BFF/HttpOnly-cookie approach.

Dependency direction is strict: `api → application → domain`; `infrastructure` implements
ports declared by `application`. See [docs/architecture.md](docs/architecture.md).

## Conventions that matter

- **Tenant context is non-optional.** Repository methods take `(&TenantContext, id)`, never
  a bare id — this is the application half of [ADR-0004](docs/adr/0004-postgres-shared-schema-multitenancy.md).
- **Never serialize DB rows to clients.** Row structs are private; API types are separate.
- **Clients branch on the error `code` field**, never on human-readable messages.
- **Secrets never in git.** `.env` is ignored; `.env.example` documents the shape.
- **Refresh rotation is compare-and-swap.** `SessionRepository::revoke` returns whether *this*
  call performed the revocation; losing that race is treated as token theft and burns the
  family. Read-then-write here is a security bug — it let two concurrent refreshes both
  succeed (caught by `scripts/verify-refresh-hazard.mjs`).
- **The mobile client de-duplicates concurrent refreshes** (`refreshOnce`). Without it, two
  simultaneous 401s sign the user out of every device.
