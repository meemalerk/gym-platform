# ADR-0007: Tiered, bounded, validated AI authority

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

The AI assistant touches health-adjacent recommendations. It must be genuinely useful while
never becoming an ungoverned actor that can silently alter training plans. Different gyms,
instructors, and members warrant different levels of trust. The product's advantage is the
deterministic domain (programme model, adjustment engine, longitudinal data), not the
chatbot — so the AI must remain a governed doorway into that domain.

## Decision

- **Explicit authority levels 0–3**, configurable per gym / instructor / member:
  - **L0 Explain only** — no changes.
  - **L1 Suggest** — proposals require member/instructor approval.
  - **L2 Adjust within guardrails** — instructor-defined bounds (e.g. ≤N% load reduction,
    remove one set, substitute from approved pool, move a session ≤2 days, reduce intensity
    below a readiness threshold). May not add exercises, increase volume, change rehab or
    testing sessions, or override locked blocks.
  - **L3 Autonomous generation** — for members with no instructor; still deterministically
    validated.
- **The AI is given narrow domain tools only.** Never raw mutation tools (`execute_sql`,
  `update_workout`, `delete_program`).
- **Structured Outputs for shape + deterministic domain validation for safety.** Schema
  adherence is necessary but not sufficient; every proposal passes through domain policy,
  which either auto-applies (within guardrails) or routes to coach approval.
- **Full audit trail:** recommendation_runs, tool_invocations, recommendation_evidence,
  policy_violations.
- Orchestration lives **inside the backend boundary**, not a separate AI microservice
  (initially).

## Alternatives considered

- **Free-form AI with DB access / function-calling into mutations** — unacceptable safety
  and auditability risk for health-adjacent changes.
- **AI fully external / advisory only (L0 everywhere)** — safe but under-delivers; instructors
  want bounded automation.

## Consequences

- **Positive:** instructors stay in control; changes are safe, auditable, and reversible via
  approval routing; the product is meaningfully more serious than a generic fitness chatbot.
- **Negative / costs:** more policy modelling, a validation layer, and audit tables; the AI
  cannot "just do things," which is the point.
- **Follow-ups:** implement `AssistantAuthority` policy, the tool contract, and the
  validation/approval pipeline ([roadmap.md](../roadmap.md) Phase 5).

## References

- [ai-authority-model.md](../ai-authority-model.md)
