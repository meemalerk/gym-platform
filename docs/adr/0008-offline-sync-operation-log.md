# ADR-0008: Offline-first sync via operation log

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

Members train in gyms with poor connectivity. Workout logging must work fully offline and
sync cleanly later, without duplicating records or corrupting history. Naive last-write-wins
would quietly destroy data ("ghosts").

## Decision

- **Offline-first member app** backed by on-device storage (Expo SQLite).
- **An operation log** captures each change as an operation:

  ```
  operation_id · device_id · entity_id · entity_version ·
  operation_type · payload · client_timestamp
  ```

- **IDs generated on-device (UUIDv7 or ULID)** so records can be created offline and remain
  globally unique.
- **Idempotent server sync**: the server accepts operations idempotently (keyed on
  `operation_id`) and returns the canonical version.

  ```
  record set offline → SQLite commits immediately → operation queued
    → network available → server accepts idempotently → returns canonical version
  ```

- **Domain-specific conflict resolution — never blanket last-write-wins:**

  | Data | Strategy |
  |------|----------|
  | Performed sets | Append-only; preserve both |
  | Instructor edit vs member workout | Published prescription stays historical; performance separate |
  | Profile settings | Last server-confirmed wins, or prompt user |
  | Coach notes | Version and preserve edit history |

## Alternatives considered

- **Online-only** — unacceptable for the core in-gym logging use case.
- **Generic CRDT/last-write-wins everywhere** — LWW loses data silently; a full CRDT stack is
  heavier than needed given that the highest-volume data (performed sets) is append-only.

## Consequences

- **Positive:** reliable in-gym logging; idempotent sync avoids duplicates; history stays
  intact; conflict handling matches the semantics of each data type.
- **Negative / costs:** client and server both carry sync/idempotency logic; per-entity
  conflict rules must be defined and maintained.
- **Follow-ups:** implement the sync endpoint and per-entity conflict rules
  ([roadmap.md](../roadmap.md) Phase 3).

## Update (2026-07-14, from research)

The **domain semantics above still govern** (append-only sets, domain-specific conflict
resolution, on-device IDs). But July-2026 research ([research-2026.md](../research-2026.md) §4)
recommends **not hand-rolling the entire sync transport**. Evaluate a proven engine:

- **PowerSync** — managed; streams our Postgres → client SQLite with configurable sync rules
  and conflict handling. Least custom code; fits our Postgres backend.
- **WatermelonDB** — batteries-included sync protocol, opinionated ORM-like model.
- **expo-sqlite + Drizzle** is the foundation under either.

Also worth prototyping: **CRDT libs (Automerge / Loro)** for *document-like* edits (programme
drafts, coach notes) where op-log semantics are awkward. **Action:** treat the operation-log
design as the domain contract; choose the transport engine during Phase 3 rather than building
it from scratch.

## References

- [domain-model.md](../domain-model.md) (offline & conflict resolution table),
  [research-2026.md](../research-2026.md) §4
