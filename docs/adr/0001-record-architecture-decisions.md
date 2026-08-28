# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

This is a greenfield, multi-session project spanning a complex domain. Decisions made early
(language, tenancy, authorization, versioning) shape everything downstream. Without a
durable record of *why*, future sessions — human or AI — will re-litigate settled choices or
silently contradict them, causing drift.

## Decision

We will record every significant architectural decision as an ADR in `docs/adr/`, using the
lightweight Nygard-style format in [`0000-template.md`](0000-template.md). Foundational ADRs
are also linked from [CLAUDE.md](../../CLAUDE.md), which every session reads first.

## Alternatives considered

- **No formal record (decisions live in chat/commits)** — lost across sessions; the primary
  failure mode we're avoiding.
- **A single "decisions" doc** — grows unwieldy and loses the per-decision status/supersede
  lifecycle.

## Consequences

- **Positive:** decisions survive session boundaries; reasoning is discoverable; reversals
  are explicit (supersede, don't overwrite).
- **Negative:** a small discipline cost — each hard choice needs a short write-up.
- **Follow-ups:** keep the index in [README.md](README.md) and the decisions list in
  CLAUDE.md current.

## References

- [CLAUDE.md](../../CLAUDE.md), [README.md](README.md)
