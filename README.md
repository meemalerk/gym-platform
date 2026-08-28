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

## Prerequisites

- Rust 1.97 (pinned via `rust-toolchain.toml`)
- Docker (for Postgres)
- Node 22+ and pnpm (for the mobile app, later)

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

The server applies pending migrations on boot, so step 3 is only needed if you want to
migrate without starting the server.

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

## Mobile app (Expo SDK 57)

```bash
cd apps/mobile
npm install
npm run typecheck          # tsc --noEmit
npm start                  # Expo dev server
```

A **custom dev client is required** (`expo-dev-client` is installed) — Expo Go cannot load
`expo-secure-store`. Build one with EAS or `npx expo run:ios` / `run:android`.

Point the app at the API. A physical device cannot reach the host's `localhost`:

```bash
EXPO_PUBLIC_API_URL=http://192.168.1.5:8080 npm start
```

Regenerate the API types whenever the backend contract changes (server must be running):

```bash
npm run codegen:api        # writes src/api/schema.d.ts from /api-docs/openapi.json
```

`src/api/gym.ts` **derives its types from that generated schema**, so a backend contract
change becomes a TypeScript error rather than a runtime failure on a phone.

## Layout

```
crates/
  domain/          pure types + invariants (no I/O)
  application/     use-cases + ports (traits)
  infrastructure/  adapters: Postgres repos, Argon2 hashing, JWT issuing
  api/             Axum router, extractors, Problem Details errors, OpenAPI
bins/server/       entrypoint: config, wiring, graceful shutdown
migrations/        sqlx migrations (never edit an applied one — add a new one)
apps/mobile/       Expo SDK 57 app (expo-router, TanStack Query, Zustand)
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

16 suites: 163 workspace unit tests; 49 e2e HTTP assertions (pinning the OpenAPI path
count); and per-feature suites against the live server and database — RLS 7,
invitations 22, audit 18, programme immutability 22, programme authoring 55,
coaching 41, assignments 24, execution 39, profiles/measurements 38, goals 20,
recommendations 15, navigation 252, activity 42, progress 35, formatting/timers 33 —
plus `tsc --noEmit`, `expo-doctor`, and a real `expo export` bundle.

Suites build before running — a stale binary makes results meaningless. Negative cases
(what must be *refused*) are asserted as deliberately as happy paths. CI runs the same
set (`.github/workflows/ci.yml`). Demo data: `bash scripts/seed-demo.sh`, then see
[docs/test-accounts.md](docs/test-accounts.md).

### Running the app in a browser (no simulator needed)

The web target is a **development/verification convenience only** — the production client is
native, and per [ADR-0009](docs/adr/0009-client-stack.md) the real browser app is a separate
React + Vite project.

```bash
# API must allow the web origin explicitly (CORS is closed by default)
CORS_ALLOWED_ORIGINS=http://localhost:8212 cargo run --bin server

cd apps/mobile
EXPO_PUBLIC_API_URL=http://localhost:8211 npx expo start --web --port 8210
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
