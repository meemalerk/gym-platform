# ADR-0006: Immutable programme versioning

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

Instructors author structured programmes that members are assigned and execute over weeks.
If a plan were directly editable, an instructor changing Monday's prescription would
retroactively rewrite what a member was historically meant to perform — corrupting training
history and making performance data uninterpretable. Head coaches also need to review work
before it reaches members.

## Decision

Model programmes as **versioned, with published versions immutable**.

```
Program → ProgramVersion(s) → weeks → workout templates → prescriptions
```

- Editing a published version creates a **new draft version**; it does not mutate the
  published one.
- **Assignments reference a specific program version.**
- Lifecycle: **Draft → In review → Approved → Published → Archived.** Published is immutable.
- Publishing a new version can optionally migrate selected members; existing assignments keep
  referencing the version they were given.
- Published prescriptions and performed sets are **separate immutable records**.

## Alternatives considered

- **Directly editable plans** — simplest, but destroys history and makes longitudinal data
  meaningless; unacceptable for a coaching platform.
- **Soft-delete / audit-only history** — recovers *what changed* but not a clean "this is
  exactly what version 2 prescribed" snapshot that assignments can pin to.

## Consequences

- **Positive:** training history is sacred and interpretable; a review/approval gate is
  natural; assignments are stable; the adjustment engine and analytics have firm ground.
- **Negative / costs:** more entities and lifecycle logic; version migration UX must be
  designed; storage of multiple versions.
- **Follow-ups:** define the member-migration flow when publishing a new version
  ([roadmap.md](../roadmap.md) Phase 2).

## References

- [domain-model.md](../domain-model.md) (lifecycle + template-to-set chain)
