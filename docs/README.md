# Documentation

Start with the row that describes you.

| You are… | Read, in this order |
|----------|---------------------|
| **Just looking** | [START HERE](../START-HERE.md) — install nothing, open a link |
| **Deciding whether this is worth building** | [problem.md](problem.md) → [product-specification.md](product-specification.md) → [market-analysis.md](market-analysis.md) → [cost-analysis.md](cost-analysis.md) |
| **New to the codebase** | [developer-guide.md](developer-guide.md) → [architecture.md](architecture.md) → [domain-model.md](domain-model.md) → [glossary.md](glossary.md) |
| **Planning the next piece of work** | [delivery-stages.md](delivery-stages.md) → [roadmap.md](roadmap.md) → [feature-plan-2026-07.md](feature-plan-2026-07.md) |
| **About to change a foundational choice** | [adr/README.md](adr/README.md) — find the ADR first, then write one that supersedes it |
| **Demoing it** | [test-accounts.md](test-accounts.md) — who the demo accounts are and what each shows |

---

## Product — what and why

| Doc | Holds |
|-----|-------|
| [problem.md](problem.md) | The problem, the users, the vision, the non-goals |
| [product-specification.md](product-specification.md) | **What the product is**: every capability, the invariants, what it refuses to do |
| [market-analysis.md](market-analysis.md) | Market size, competitor pricing, the positioning gap |
| [subscriptions-billing.md](subscriptions-billing.md) | The B2B2C billing model: SaaS + memberships + entitlements |
| [cost-analysis.md](cost-analysis.md) | Cost model, AI economics, unit economics, break-even |

## Engineering — how it is built

| Doc | Holds |
|-----|-------|
| [developer-guide.md](developer-guide.md) | **Start here as an engineer**: run it, the layering, the five things that bite, how to add a feature |
| [architecture.md](architecture.md) | System architecture, module map, dependency direction |
| [domain-model.md](domain-model.md) | Entities, the programme lifecycle, the storage model |
| [authorization-model.md](authorization-model.md) | ReBAC model; the boundary between auth and domain policy |
| [tech-stack.md](tech-stack.md) | The chosen stack, why, and the version pins that each cost a bug |
| [glossary.md](glossary.md) | Domain vocabulary — use these words exactly |
| [research-2026.md](research-2026.md) | Stack research and sources. **Version numbers are snapshots — re-verify** |

## Delivery — what order and when

| Doc | Holds |
|-----|-------|
| [delivery-stages.md](delivery-stages.md) | The stages, what each one *proves*, and what "done" means |
| [roadmap.md](roadmap.md) | **The authoritative "current phase" marker.** Check this before starting work |
| [feature-plan-2026-07.md](feature-plan-2026-07.md) | Every requested capability mapped to a phase, plus **§11: the known gaps** |
| [hosting-deployment.md](hosting-deployment.md) | Hosting options, costs, deployment approach |

## Decisions

[adr/README.md](adr/README.md) — twenty-two architecture decision records: the *why* behind
every hard choice, and what each one costs.

**Before changing a foundational decision** (language, tenancy, auth, versioning), read its ADR.
To reverse one, write a new ADR that supersedes it. Do not silently contradict it — the next
person will find the old reasoning and assume it still holds.

## Elsewhere in the repository

| Where | What |
|-------|------|
| [../CLAUDE.md](../CLAUDE.md) | The working anchor: current status, locked decisions, invariants |
| [../ORIGIN.md](../ORIGIN.md) | Where this codebase came from — the prototype→product pivot |
| [../screenshots/README.md](../screenshots/README.md) | Every view, per role, captured from the running app |
| [../research/INDEX.md](../research/INDEX.md) | Downloaded primary sources, each annotated with the decision it grounds |
| [archive/](archive/) | Prototype-era documents. Reference only — do not treat as current |

---

## Keeping this honest

- Every relative link here is checked by `node scripts/verify-docs.mjs`, which runs in
  `scripts/all-check.sh`. A moved or renamed document fails the build rather than rotting into a
  dead link.
- The files are deliberately **flat**, not filed into `product/`, `engineering/` and `delivery/`
  subfolders. Grouping is what this index is for; moving them would rewrite 250-odd links across
  the ADRs, the code comments and the scripts, and every one of those is a chance to break a
  reference silently. The index costs nothing and cannot go stale without the checker noticing.
