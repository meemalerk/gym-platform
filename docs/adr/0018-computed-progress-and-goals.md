# ADR-0018: Progress is computed from immutable history, never stored

- **Status:** Accepted
- **Date:** 2026-07-19 *(recorded after implementation; the decision was made across the execution, measurement and goal features)*
- **Deciders:** Project author

## Context

Members track fitness through three data families ([feature-plan-2026-07.md](../feature-plan-2026-07.md) §4):
performance (from logged sets), body measurements (self-reported), and goals (targets on
metrics). Each invites the same design mistake: storing a derived number — "current 1RM",
"current BMI", "goal progress 62 %" — next to the data it was derived from. Stored
derivations drift: a backfilled set, a corrected measurement or a changed formula leaves
the cached number silently wrong, and the platform's core asset is *trustworthy*
longitudinal data.

There is also a measurement-integrity question in the source data itself: true 1RM
testing is reliable but costly and risky to administer frequently (Grgic et al. 2020),
and coaches cannot accurately observe proximity-to-failure from the floor (Emanuel et
al. 2022) — so the honest inputs are the sets and self-reported RPE/RIR members log
anyway.

## Decision

1. **Derived numbers are computed on read, from immutable records.** Estimated 1RM uses
   Epley on logged sets, **capped at 12 reps** (beyond which rep-based estimation is
   noise), and "best set" means best **by estimate**, not by heaviest bar — 5×65 kg
   outranks 3×67.5 kg, and a test pins this precisely because intuition disagrees. BMI
   derives from profile height plus the latest weight measurement. Nothing user-facing
   is a cached aggregate.
2. **Goals are targets on observable metrics only** (bodyweight, an exercise's estimated
   1RM) — never free text pretending to be tracked. A goal captures its **baseline at
   creation**; progress is `(current − baseline) / (target − baseline)`, clamped to
   0..1, computed on read. Status is an evidence-carrying enum (`achieved` carries when).
3. **Goals are the one self-service surface**: members may set their own. Everything else
   an athlete receives flows through coaching authority; a personal target is the
   member's own business, and requiring a coach to transcribe it would only add friction
   and errors.

## Alternatives considered

- **Materialised progress columns / snapshot tables.** Faster reads, but introduces a
  second source of truth that must be invalidated on every write path — including future
  offline-sync replays, exactly where invalidation bugs hide. Rejected until a measured
  performance need exists; at that point, derive-and-cache with the computation as the
  authority.
- **Event-time evaluation via the outbox worker.** Right for *notifications* ("goal
  achieved!") but wrong as the source of the number itself; the roadmap keeps
  achievement-evaluation-on-data-arrival for the worker phase.
- **Free-text goals as first-class records.** Honest-sounding, but they rot: nothing can
  compute against "get stronger". They belong as coach notes, in a different table, so
  nothing pretends to track them.

## Consequences

- **Positive:** No drift, ever — a corrected history instantly corrects every trend,
  goal bar and BMI. The computations are pure client-side modules with node-run tests
  (35 progress assertions), and the same functions are trivially portable server-side
  later.
- **Negative / costs:** Recompute-on-read cost grows with history length (fine at
  member scale; pagination and windowing are the mitigations before caching is).
  Epley is an estimator with known error bars — the UI says "est." and never presents
  it as a tested max.
- **Follow-ups:** Adherence metrics (sessions-per-week against plan) join the same
  computed family in Phase 4; session-RPE-derived training load (Halson 2014) is the
  candidate coach-facing signal, again computed from what members already log.

## References

- Grgic, J. et al. (2020) — [research/11](../../research/11-1rm-reliability-grgic-2020.pdf)
- Emanuel, A. et al. (2022) — [research/10](../../research/10-seeing-effort-rir-coaches-2022.pdf)
- Halson, S. L. (2014) — [research/15](../../research/15-training-load-monitoring-halson-2014.pdf)
- [ADR-0006](0006-immutable-program-versioning.md) (immutability of the prescribed side),
  [ADR-0008](0008-offline-sync-operation-log.md) (immutability of the performed side)
- Verified by `scripts/verify-goals.sh`, `scripts/verify-progress.mjs`, `scripts/verify-profiles.sh`
