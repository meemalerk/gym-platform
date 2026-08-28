# Origin: From prototype to product

This codebase began as a **BSc Computer Science final assignment prototype** (2026-07-18 to 2026-07-19) — a platform for multi-tenant coaching and gym management, built to explore architecture decisions under the constraints of verification-first development and declared generative-AI assistance.

## The pivot

As of 2026-07-25, this project was **withdrawn from assessment** and development shifted to product. The code is now a standalone application being built as a real commercial platform; it is not being submitted to UWE Bristol.

## Historical record

- The original development history (with full AI-assistance attribution) is preserved on the `archive/assignment` branch — the complete record of how this prototype was built.
- The product branch (main) carries a clean history, beginning from the same codebase but representing the evolution of a product rather than assessed work.
- Assignment-related documentation (the academic report, project log) is archived in [`docs/archive/`](docs/archive/) for reference and portfolio use.

## What carries forward

The **architecture, design decisions, and verification suites** built during the prototype phase remain the foundation:

- 19 ADRs recording why each choice was made
- Modular monolith design (Rust backend, React Native mobile)
- Multi-tenant isolation via shared schema + RLS
- Verification-first methodology (16 suites, ~1,000 assertions)
- Immutable programme versioning, append-only execution history, deterministic recommendations

The prototype proved the technical foundation works. The product builds from there.

## Next phase

The roadmap lives in [docs/roadmap.md](docs/roadmap.md). Phase 3 (assignment and execution) is complete; Phase 4 (monitoring and background work) is next, followed by the bounded AI assistant in Phase 5. The business model (gym subscriptions, member memberships, Stripe billing) is designed but not yet built — see [docs/subscriptions-billing.md](docs/subscriptions-billing.md).

---

**For context on how the prototype was developed** (methodology, defects found, verification evidence), see the archived [docs/assignment-report.md](docs/archive/assignment-report.md) and [docs/project-log.md](docs/archive/project-log.md).
