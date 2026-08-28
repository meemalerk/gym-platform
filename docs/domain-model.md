# Domain Model

The domain is the product. Model it so **invalid states are unrepresentable** — prefer
enums over bags of nullable fields. This document defines the core entities, the programme
lifecycle, and the storage model.

## Modelling principle

Rust example — do this:

```rust
enum ProgramStatus {
    Draft,
    InReview,
    Published { published_at: DateTime<Utc>, published_by: UserId },
    Archived,
}
```

Not this:

```rust
struct Program { status: String, published_at: Option<...>, published_by: Option<...> }
```

Same for prescriptions — the shape of the data depends on the kind:

```rust
enum ExercisePrescription {
    Repetitions { sets: NonZeroU8, target: RepRange, rir: Option<Rir> },
    Duration    { sets: NonZeroU8, seconds: NonZeroU32 },
    Distance    { meters: NonZeroU32, pace: Option<PaceRange> },
}
```

## Programme versioning — the most important part

**A plan is NOT directly editable workouts.** Programmes are versioned; published versions
are immutable ([ADR-0006](adr/0006-immutable-program-versioning.md)).

```
Program
  ├── ProgramVersion 1
  │      ├── Week 1
  │      ├── Week 2
  │      └── Week 3
  └── ProgramVersion 2
         ├── Week 1 ...
```

Editing a published version does **not** mutate it — it creates a new draft version:

```
Version 3 draft  →  review  →  Version 3 published
      →  optionally migrate selected members
      →  new assignments reference Version 3
```

This prevents an instructor from changing Monday's prescription and accidentally rewriting
what a member was historically meant to perform.

### Version lifecycle

```
Draft → In review → Approved → Published → Archived
```

Once **Published**, a version is immutable.

### From template to performed set

```
Program template
      ↓
Program version        (immutable once published)
      ↓
Member assignment
      ↓
Scheduled workout
      ↓
Workout session
      ↓
Performed exercise / set   (immutable history)
```

Published prescriptions and performed sets are two separate, immutable records. Never
collapse them.

## Identity: one account, capacities as a set (ADR-0014, capped to one gym by ADR-0023)

Three concepts that are easy to conflate, kept deliberately separate:

```
Account   — who you are.        One per person, platform-wide. Never a second login.
Capacity  — what you may do HERE. In the (one) gym. A SET, not a single role.
Profile   — data describing you.  Per person, not per gym.
```

- **Capacities:** `owner`, `trainer`, `member` (ADR-0036). A person may hold
  several at once (a trainer who also trains there). Effective permission is the union,
  resolved **only** through `Capabilities::can_*`.
- **"Gym owner" is a capacity, not a profile** — there is no personal data describing you *as*
  an owner. Multiple people can hold `owner` in the gym.
- **Profiles** (`athlete_profiles`, `trainer_profiles`) are keyed by `user_id` with **no
  `gym_id`** — a holdover from the multi-gym design this data model was built for (ADR-0014);
  now vestigial in a single-gym deployment, but harmless: the schema still correctly says a
  profile belongs to the person, not the tenant.
- **`gyms.is_personal`** ("solo users get a personal gym") is likewise vestigial post-ADR-0023
  — there is only ever one gym row, so nothing for a personal one to be distinct *from*.
- **Joining is by invitation** (`gym_invitations`): accepting adds capacities to the *existing*
  account. Pre-ADR-0023 this was also how a trainer became affiliated with several gyms; now
  there is only the one gym to be invited into.

## Storage model (Postgres, single shared schema)

Every tenant-owned table carries `gym_id` and is indexed on it. IDs are UUIDv7/ULID so
records can be created offline on-device.

### Organisations & identity
```
gyms (is_personal) · gym_capacities · gym_invitations
athlete_profiles · trainer_profiles          -- person-owned, no gym_id
branches                                      -- not yet built
```

### Coaching
```
coach_athlete_relationships · coach_availability · coach_notes · consultations
```

### Exercise catalogue
```
exercises · exercise_variants · exercise_media · equipment · muscle_groups
exercise_tags · exercise_constraints · exercise_alternatives
```

### Programming
```
programs · program_versions · training_blocks · program_weeks
workout_templates · workout_template_exercises · prescription_rules · progression_rules
```

### Assignment
```
program_assignments · scheduled_workouts · workout_adjustments · coach_approvals
```

### Execution
```
workout_sessions · session_exercises · performed_sets · exercise_feedback · session_feedback
```

### Health & progress
```
readiness_entries · body_measurements · progress_photos
imported_health_records · injuries_and_limitations
```

### AI
```
assistant_threads · assistant_messages · recommendation_runs
tool_invocations · recommendation_evidence · policy_violations
```

### Governance
```
audit_log · domain_events · outbox_messages
```

### Example table (note `gym_id` + index)

```sql
CREATE TABLE programs (
    id          UUID PRIMARY KEY,           -- UUIDv7
    gym_id      UUID NOT NULL REFERENCES gyms(id),
    created_by  UUID NOT NULL REFERENCES users(id),
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX programs_gym_id_idx ON programs (gym_id);
```

## Command handler shape

Use-cases authorize, validate against domain policy, then persist. Example:

```rust
impl AssignProgramHandler {
    pub async fn execute(&self, actor: &Actor, command: AssignProgram)
        -> Result<ProgramAssignment, AssignProgramError>
    {
        self.authorization
            .require(actor, Action::AssignProgram,
                     Resource::Member { gym_id: command.gym_id, member_id: command.member_id })
            .await?;                                   // may this actor?
        self.programs.verify_publishable(command.program_version_id).await?;  // is it valid?
        self.assignments.assign(command).await         // persist
    }
}
```

Authorization first, domain validation second, persistence third — always in that order.

## Offline & conflict resolution

Members record data offline; sync is idempotent via an operation log
([ADR-0008](adr/0008-offline-sync-operation-log.md)). Conflict handling is
**domain-specific**, never blanket last-write-wins:

| Data | Strategy |
|------|----------|
| Performed sets | Append-only; preserve both records |
| Instructor edit vs member workout | Published prescription stays historical; member performance is separate |
| Profile settings | Last server-confirmed wins, or prompt the user |
| Coach notes | Version and preserve edit history |

Blanket last-write-wins is how data quietly acquires ghosts.
