# Authorization Model

Roles alone are insufficient. One person can simultaneously be:

```
One person is:
  - a member at Gym A
  - an instructor at Gym B
  - temporarily covering a class at Branch C
  - coaching Members 1, 2 and 3
  - allowed to view programmes but not billing
```

Global RBAC cannot express this cleanly. We use **relationship-based access control
(ReBAC)** via OpenFGA, plus application-level domain checks
([ADR-0005](adr/0005-relationship-based-authorization.md)).

## The critical boundary

Two different questions. Keep them in two different layers.

| Question | Answered by | Example |
|----------|-------------|---------|
| **"May this actor attempt this action?"** | OpenFGA + app-level authz | Can this instructor edit this programme? |
| **"Is this action valid?"** | Domain policy (deterministic Rust) | Is this programme publishable? Is this load reduction within limits? |

**Never move workout safety rules or domain validation into the authorization engine.**
Authorization gates the attempt; domain policy judges the content.

## Relationships, not just roles

```
user:instructor_1   instructor of   member:member_7
member:member_7      belongs to      gym:gym_a
program:program_4    owned by        gym:gym_a
```

## Conceptual OpenFGA model

```
type gym
  relations
    define owner: [user]
    define admin: [user]
    define instructor: [user]
    define member: [user]

type athlete
  relations
    define gym: [gym]
    define user: [user]
    define coach: [user]
    define viewer:
        user
        or coach
        or admin from gym

type program
  relations
    define gym: [gym]
    define author: [user]
    define assigned_athlete: [athlete]
    define editor:
        author
    define viewer:
        assigned_athlete
        or coach from assigned_athlete
        or editor
```

This is a starting point — the real model evolves with the domain. Version the model and
treat changes to it with the same care as schema migrations.

## OpenFGA vs Cerbos

| Use OpenFGA (chosen) when permissions are relationship-driven | Use Cerbos when policies are attribute-driven |
|---|---|
| Who coaches whom? | Instructor can change a plan only when… |
| Who owns this programme? | …member is in the same branch |
| Which branch contains this member? | …instructor certification is current |
| Who supervises this instructor? | …programme is still a draft, change below a risk level |

**Decision:** OpenFGA + application-level domain checks. The relationship graph is the hard
part here; attribute conditions we handle in domain policy where they belong (and OpenFGA
conditions cover the rest). Cerbos remains a documented alternative if attribute-heavy
policy proliferates.

## Defence in depth

Authorization is enforced primarily in the application, and **reinforced** by Postgres
row-level security keyed on `gym_id`. RLS is not the primary mechanism — it is the backstop
that makes a cross-tenant leak require *two* failures, not one.

## In code

Authorization is the first step of every command handler, before domain validation:

```rust
self.authorization
    .require(actor, Action::AssignProgram,
             Resource::Member { gym_id, member_id })
    .await?;                                  // authz: may this actor?
self.programs.verify_publishable(version_id).await?;   // domain: is it valid?
self.assignments.assign(command).await                 // persist
```
