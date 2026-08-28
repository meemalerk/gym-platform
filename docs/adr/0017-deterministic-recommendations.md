# ADR-0017: Recommendations are deterministic rules with readable reasons

- **Status:** Accepted
- **Date:** 2026-07-19 *(recorded after implementation; the decision was made when the feature was built)*
- **Deciders:** Project author

## Context

Goals (targets on observable metrics) created an obvious product opening: suggest
programmes and coaches that serve the goal. The obvious 2026 implementation is a learned
ranker or an LLM call. Both collide with this platform's established posture:

- [ADR-0007](0007-ai-authority-levels.md) commits to **deterministic validation first**,
  with model-driven assistance arriving later as a bounded, audited authority — not as
  silent ranking sprinkled into screens.
- The UX rule in [feature-plan-2026-07.md](../feature-plan-2026-07.md) §9: *no screen
  shows a number it cannot explain*. A suggestion is a claim; an unexplainable claim in a
  coached-training context erodes exactly the trust the platform sells.
- The survey literature on learned recommenders (Zhang et al. 2019, research library #6)
  is explicit about their costs: opacity, data hunger, evaluation difficulty. This
  platform has neither the interaction volume to train on nor any appetite for
  unaccountable output between a coach and their athlete.

## Decision

Recommendations are **pure functions over facts the member can see**, with every
suggestion carrying a human-readable `because`:

1. Each programme carries a coach-chosen **focus** (strength / hypertrophy /
   conditioning / general). `general` deliberately recommends nothing — honest until the
   coach says otherwise.
2. Each active goal maps to a focus by a fixed rule (lift goal → strength; cut →
   conditioning; gain → hypertrophy), implemented as a domain function with tests.
3. Published programmes with a matching focus that the member is not already assigned,
   and coaches whose stated profile specialties match by **deliberately dumb substring
   keywords** and who do not already coach the member, are suggested.
4. Suggestions **retire when acted on**. No goals → empty lists, never guesses. Trainers
   without a profile are never suggested: no evidence, no suggestion.

No scoring, no weights, no learning, no model call.

## Alternatives considered

- **Learned ranking (collaborative filtering → deep models).** Needs interaction data
  that does not exist, cannot explain itself at the per-suggestion level, and would put
  an unaccountable artefact into the coach–athlete relationship. Rejected on posture,
  not just pragmatics.
- **LLM-generated suggestions.** Non-deterministic output on a surface that feeds
  training decisions; contradicts ADR-0007's sequencing (the assistant arrives *behind*
  guardrails, not before them). Rejected here; the deterministic engine becomes the
  guardrail the future assistant is validated against.
- **No recommendations.** Defensible, but wastes the goal data the member already gave
  us, and "the system noticed my goal" is genuine, cheap product value when it is honest.

## Consequences

- **Positive:** Every suggestion is auditable, testable end-to-end (15 scripted
  assertions), and explainable in one sentence. The rule set is small enough to state in
  the UI itself. EU AI Act Art. 50 disclosure questions do not even arise — there is
  nothing model-driven to disclose.
- **Negative / costs:** Recall is deliberately poor: a mislabelled programme or a
  specialty phrased unusually is missed. Substring matching is crude and anglophone.
  The rules must be maintained by hand as metrics and foci grow.
- **Follow-ups:** When the Phase 5 assistant lands, it may *draft* richer suggestions,
  but they route through the same deterministic validation and the same `because`
  contract — the assistant explains; it does not silently rank.

## References

- [ADR-0007](0007-ai-authority-levels.md) — AI authority levels
- Zhang, S. et al. (2019) *Deep Learning based Recommender System: A Survey* —
  [research/06](../../research/06-deep-learning-recsys-survey-zhang-2017.pdf)
- Verified by `scripts/verify-recommendations.sh`
