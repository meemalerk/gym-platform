# Gym Platform

A coaching and gym-management platform: one gym, its staff and its members. Coaches write
versioned training programmes, members train against them on a phone, and the gym runs its
roster, timetable, memberships and invoices from a web console. Training history is
append-only, and progress is calculated from it rather than stored.

Two clients talk to one Rust API:

- **the app** — React Native (Expo), for members and floor trainers
- **the console** — React + Vite, for owners and coaches

---

## 1. Just look at it (nothing to install but Docker)

```bash
docker compose -f docker-compose.demo.yml up --build
```

Postgres, the API, demo data and the app all start together. Open <http://localhost:8210>.
The first build takes a few minutes; after that it starts in seconds.

To stop it: `Ctrl+C`, then `docker compose -f docker-compose.demo.yml down`.

[START-HERE.md](START-HERE.md) is the same thing written for someone non-technical, with
double-clickable launchers for Windows, macOS and Linux.

**To show it to someone else**, including on their phone: `bash demo/share.sh` (or
double-click `share-demo.bat` on Windows). It opens a free Cloudflare tunnel and prints a
public `https://` link and a QR code. The link works until you stop it. The card-payment
page is the one screen a shared link cannot reach.

**To hand over the source**: `bash demo/package.sh` writes a zip of the tracked files only
— no build output, no `node_modules`, no `.env`.

---

## 2. Signing in

The demo data is built around a fixed set of accounts, so sign in as one of those rather
than registering. The demo above creates them for you; if you are running things yourself,
`bash scripts/seed-demo.sh` does it (safe to run more than once).

You can also register a new account and look around as a member with no data — the demo
leaves the door open on purpose. A real deployment keeps it shut unless the owner opens it.

**The password for every account is `demopassword`.**

| Account | Role | App | Console | What it shows |
|---------|------|:---:|:-------:|---------------|
| `owner@demo.test` | owner | yes | yes | Everything: roster, billing, the exercise catalogue, the activity log, publishing programmes |
| `trainer@demo.test` | trainer | yes | yes | Only their own clients — not the gym roster |
| `trainer2@demo.test` | trainer | yes | yes | A second coach, so the boundaries are visible: the first coach's clients and class registers are refused |
| `member@demo.test` | member | yes | — | The fullest account: programme, workout history, goals, measurements, a booked class |
| `solo@demo.test` | member | yes | — | No coach and an Open Gym plan, so the app offers *Start your own workout* |

Both sign-in screens list the accounts as one-tap buttons, so there is nothing to type.
Members can sign into the console but will find little there — it is a staff tool.

More detail on what each account demonstrates: [docs/test-accounts.md](docs/test-accounts.md).

---

## 3. Run it yourself

### Prerequisites

- **Rust 1.97** — the exact version is pinned in `rust-toolchain.toml`, so `rustup` fetches
  it for you
- **Docker** — for PostgreSQL
- **Node 22 or newer** and npm — for the two clients

### Start the API

```bash
# 1. PostgreSQL (host port 5455, so it will not clash with a Postgres you already run)
docker compose up -d postgres

# 2. Configuration
cp .env.example .env

# 3. Apply the migrations
cargo install sqlx-cli --no-default-features --features rustls,postgres --locked
export DATABASE_URL="postgres://gym:gym_dev_password@localhost:5455/gym"
sqlx migrate run --source ./migrations

# 4. Run
cargo run --bin server
```

Then:

- <http://localhost:8080/health> — liveness
- <http://localhost:8080/ready> — readiness, including the database
- <http://localhost:8080/swagger-ui> — browse and call the API
- <http://localhost:8080/api-docs/openapi.json> — the OpenAPI document

Finally, create the demo accounts and data:

```bash
bash scripts/seed-demo.sh
```

> **Step 3 is not optional the first time.** The SQL in this project is checked against a
> real schema *while it compiles*, so building against an empty database fails with a wall
> of `relation "..." does not exist` before the server ever starts. Either apply the
> migrations first, or build against the checked-in query cache with
> `SQLX_OFFLINE=true cargo run --bin server`. Once the schema exists, the server applies any
> new migrations itself at startup.

### Start the console

```bash
cd apps/console
npm install
npm run dev
```

Open <http://localhost:5174>. It expects the API on port 8080 and forwards `/api` to it, so
there is no address to configure.

### Start the app

```bash
cd apps/mobile
npm install --legacy-peer-deps
npm start
```

Press `w` for a browser, `i` or `a` for a simulator, or scan the QR code with **Expo Go**
(App Store / Play Store) to run it on a real phone. No Apple Developer account or custom
build is needed.

> `npm start` runs `expo start --go`, and the `--go` matters. This project also has
> `expo-dev-client` installed for standalone builds, and the Expo CLI treats that as "this
> project uses a custom development build": plain `expo start` then advertises a
> `gymplatform://` link that only such a build can open, so **Expo Go shows nothing**. If
> you are working on a standalone build instead, use `npm run start:dev-client`.

**On a real phone**, the phone and this computer must be on the same Wi-Fi, and the phone
cannot reach `localhost` — that would be the phone itself. Point the app at this computer's
address on the network:

```bash
# find your address:
#   Windows   ipconfig                 (look for IPv4 Address)
#   macOS     ipconfig getifaddr en0
#   Linux     hostname -I
EXPO_PUBLIC_API_URL=http://YOUR-ADDRESS:8080 npm start
```

If the phone cannot connect, it is almost always a firewall blocking incoming connections
to ports 8081 and 8080 on this computer.

### Keeping the clients in step with the API

The clients derive their types from the API's OpenAPI document, so a backend change becomes
a compile error rather than a surprise at runtime. With the server running:

```bash
cd apps/console && npm run codegen:api            # writes src/lib/schema.d.ts
cd apps/mobile  && API_PORT=8080 npm run codegen:api   # writes src/api/schema.d.ts
```

---

## 4. Testing it

One command runs everything. It needs the database from step 1 running, plus Node:

```bash
bash scripts/all-check.sh
```

That is around 1,450 assertions: formatting, lint, 283 unit tests, then per-feature suites
that start a real server against a real database and check both what must work and what
must be **refused** — tenant isolation, permissions, billing, programme immutability,
bookings — and typechecks for both clients. Individual suites live in `scripts/` and can be
run on their own, for example `bash scripts/verify-billing.sh`.

`.github/workflows/ci.yml` runs a subset of the same suites on every push.

> On Windows this stops any development server you have running: each suite clears stray
> `server.exe` processes before rebuilding, and it cannot tell yours from a leftover.

---

## 5. Layout

```
crates/
  domain/          types and rules, no I/O
  application/     use-cases and the interfaces they need
  infrastructure/  PostgreSQL, password hashing, token issuing
  api/             HTTP routes, errors, OpenAPI
bins/server/       the API entrypoint
bins/worker/       background jobs: recurring billing, overdue notices
migrations/        database schema, applied in order
apps/mobile/       the app (Expo)
apps/console/      the console (React + Vite)
scripts/           test suites, plus seed-demo.sh
docs/              product and architecture documentation
```

Dependencies point one way: `api` uses `application`, which uses `domain`.

---

## 6. If something goes wrong

| Symptom | Cause |
|---------|-------|
| `cargo run` fails with `relation "..." does not exist` | The migrations have not been applied — see the note in step 3 |
| `port is already allocated` | Something else is using 5455, 8080, 8081, 5174, 8210 or 8211. Stop it, or change the port in `docker-compose.yml` or in the command you are running |
| The console loads but cannot sign in | The API is not running on port 8080 |
| The phone cannot reach the app, or times out | Wrong network, or a firewall blocking ports 8081 and 8080 |
| The browser demo shows an empty page | Give it a moment on first start, then reload; `docker compose -f docker-compose.demo.yml logs` shows what each part is doing |
| A test suite reports every check as failed | The database is not running: `docker compose up -d postgres` |

---

## 7. Reading further

- [docs/product-specification.md](docs/product-specification.md) — what the product does, and what it deliberately does not
- [docs/developer-guide.md](docs/developer-guide.md) — how the code is organised and how to add a feature
- [docs/architecture.md](docs/architecture.md) — the shape of the system
- [docs/adr/README.md](docs/adr/README.md) — why the significant choices were made
- [screenshots/README.md](screenshots/README.md) — every screen, per role
- [CLAUDE.md](CLAUDE.md) — working notes for anyone changing the code
