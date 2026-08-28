# A multi-tenant coaching and gym-management platform

*Project report. Snapshot date: 2026-07-19.*

*Companion documents: [project-log.md](project-log.md) (dated development record and
generative-AI declaration), [docs/adr/](../adr/README.md) (19 architecture decision
records), [screenshots/](../../screenshots/README.md) (44 captures of the running system),
[research/](../../research/INDEX.md) (the 16 primary sources cited below, vendored as
PDFs).*

---

## Abstract

This project designs, implements and evaluates a multi-tenant software platform for
coaching gyms, in which gym organisations, coaches and members share one system while
remaining strictly isolated. The backend is a modular monolith in Rust over PostgreSQL,
enforcing tenant isolation twice (application-layer scoping and row-level security) and
modelling the domain so that invalid states are unrepresentable; the client is a
capacity-aware React Native application whose interface derives from what the signed-in
person may do in the active gym. The distinguishing technical commitments are an
immutable programme-versioning model, in which what a coach prescribed can never be
silently rewritten; an append-only execution history, from which all progress metrics
are computed rather than stored; and a deterministic, explainable recommendation engine
adopted in deliberate preference to learned ranking. Development followed a
verification-first methodology — every feature ships with an executable check against
live infrastructure, currently sixteen suites totalling roughly one thousand
assertions — and was carried out with declared use of a generative AI assistant under
the author's direction. The report evaluates the system against its objectives,
grounds its principal decisions in the literature, and sets out the professional,
ethical and legal considerations of building AI-adjacent fitness software.

## 1. Introduction

### 1.1 Context and problem

Commercial gym software divides into consumer workout trackers, which record training
but know nothing of coaching authority, and gym-administration systems, which manage
billing and access but treat the training itself as free text. Coaching gyms —
facilities whose product is programmed, supervised training — fall between: their
programmes live in spreadsheets, their coach–athlete assignments in group chats, and
their longitudinal training data nowhere at all.

The problem this project addresses is the construction of a single platform in which
**the programme is a first-class, versioned artefact**; **coaching authority is
modelled explicitly** (who may prescribe to whom, who may see whose data); and **the
member's training history is trustworthy enough to compute progress from** — across
many gym organisations sharing one deployment ([problem.md](../problem.md)).

### 1.2 Aim and objectives

**Aim:** to design and build a production-shaped, multi-tenant coaching platform whose
core domain — programmes, coaching relationships, execution history — is verifiably
correct, and to evaluate the design decisions against the literature.

Objectives:

1. **O1 — Tenancy.** Multiple gym organisations on one database with layered isolation,
   such that cross-tenant access is refused at two independent layers.
2. **O2 — Identity.** One account holding different capacities in different gyms
   (owner, admin, head coach, trainer, member), including several at once in one gym.
3. **O3 — Programme model.** Versioned, immutable-once-published programme authoring
   with a review workflow, enforced in the domain and in the database.
4. **O4 — Coaching and execution.** Explicit coach–athlete relationships gating
   per-client data; assignment of specific programme versions; append-only logging of
   performed work, idempotent against replay.
5. **O5 — Member value.** Computed progress (estimated 1RM, body measurements, BMI),
   measurable goals, and explainable recommendations.
6. **O6 — Verification.** An executable evidence base: every claim above demonstrable
   by a script a third party can run.

### 1.3 Report structure

Section 2 surveys the literature and standards the design leans on; section 3 states
the methodology, including the declared use of generative AI; sections 4–6 cover
requirements, design and implementation; section 7 evaluates the system against the
objectives; section 8 addresses professional, ethical and legal issues; section 9
concludes.

## 2. Background and related work

**Multi-tenancy and isolation.** The canonical trade-off runs from
database-per-tenant (strong isolation, high operational cost) to shared-schema
(efficient, but isolation becomes the application's problem). This project takes
shared-schema with a tenant key on every tenant-owned table, then treats PostgreSQL
row-level security as an independent second wall (PostgreSQL Global Development Group,
2026) — a defence-in-depth posture examined in §5.2 and recorded in
[ADR-0004](../adr/0004-postgres-shared-schema-multitenancy.md).

**Monolith vs microservices.** Dragoni et al. (2016) catalogue the costs of the
microservice style — operational complexity, distributed transactions, interface
versioning — alongside its benefits. At single-team scale those costs dominate, which
motivates the modular monolith: the paper's modularity goals, achieved by internal
crate boundaries rather than network boundaries
([ADR-0003](../adr/0003-modular-monolith.md)).

**Language-level safety.** Rust's ownership discipline has been given machine-checked
foundations by Jung et al. (2018), and Evans, Campbell and Soffa (2020) provide
empirical evidence that practising developers overwhelmingly stay within the safe
subset. Wadler (2015) supplies the older intellectual frame — types as propositions —
that this project applies at the domain level: status enums that *carry their
evidence*, so that, for instance, a published programme version without a publisher is
not an invalid row but an inexpressible one (§5.3).

**REST.** The external interface follows Fielding's (2000) architectural constraints —
statelessness, uniform interface — concretely expressed as an OpenAPI contract from
which the client's types are generated, making contract drift a compile-time failure.

**Offline-first and distributed data.** Kleppmann et al. (2019) argue for local-first
software: the user's device as the primary replica of the user's own data. Bailis et
al. (2013) delimit which transactional guarantees are achievable under partition;
Preguiça (2018) surveys CRDTs, whose convergence guarantees do not extend to domain
validity. Together these shape the project's offline posture (§5.5): local-first for
the member's *own* training data, server-authoritative for tenancy and coaching, and
domain-specific conflict handling rather than algebraic merge
([ADR-0008](../adr/0008-offline-sync-operation-log.md)).

**Recommender systems.** Zhang et al. (2019) survey deep-learning recommenders and are
candid about their opacity and data requirements. In a coached-training context the
project instead adopts deterministic, reason-carrying suggestion rules
([ADR-0017](../adr/0017-deterministic-recommendations.md)) — a considered rejection
discussed in §6.4.

**Security standards.** Password storage follows the Argon2 RFC (Biryukov et al.,
2021); the refresh-token model implements the OAuth 2.0 Security Best Current
Practice's rotation-with-reuse-detection (Lodderstedt et al., 2025); the overall
posture is checked against OWASP's Application Security Verification Standard (OWASP,
2021). Client-generated identifiers use UUIDv7 (Davis, Peabody and Leach, 2024) for
offline generation with index-friendly time ordering. Token structure follows JWT
(Jones, Bradley and Sakimura, 2015).

**Sports science.** Two findings ground the execution model's data choices: coaches
systematically misestimate an athlete's proximity to failure by observation (Emanuel et
al., 2022), which justifies member-logged RPE/RIR as first-class data; and true-1RM
testing, while reliable, is costly to administer frequently (Grgic et al., 2020), which
justifies estimated-1RM trends computed from ordinary logged sets. Halson (2014)
motivates the planned session-RPE-derived training-load monitoring in the roadmap.

## 3. Methodology

**Process.** Development proceeded in dependency-ordered phases (identity → tenancy →
programme model → coaching → assignment → execution → member value), each phase leaving
the system demonstrable ([roadmap.md](../roadmap.md)). Every significant decision is an
architecture decision record with context, alternatives and consequences — nineteen at
the time of writing ([adr/README.md](../adr/README.md)) — following the practice
popularised by Nygard (2011). Reversals supersede; they do not silently contradict.

**Verification-first.** The project's defining methodological choice
([ADR-0019](../adr/0019-verification-first-development.md)): no feature is complete until
an executable script proves it against the live server and database, asserting refusals
as deliberately as successes. Section 7 presents the resulting evidence base. The
justification is that the system's claims — immutability, isolation, append-only
history — live in the seams between layers, where mocked tests are blind.

**Use of generative AI (declaration).** Implementation and documentation drafting were
carried out with substantial, declared use of a generative AI coding assistant under
the author's direction, in line with UWE Bristol's principles for generative AI use
(UWE Bristol, 2026a; 2026b): the author owned direction, decisions and review; the
assistant produced code and drafts for ratification; AI-assisted commits are attributed
as such in the version history. The full statement — division of labour, why the
verification-first methodology exists partly *because* of this collaboration model, and
the defects the checks caught in generated code — is in
[project-log.md](project-log.md), which also serves as the project diary.

**Tooling.** Rust 1.88 / Axum / SQLx over PostgreSQL 17; React Native via Expo SDK 57
with TypeScript; OpenAPI via utoipa with generated client types; docker-compose for
infrastructure; a headless-browser harness (Puppeteer) for the screenshot corpus.

## 4. Requirements

Requirements were gathered from the product brief and refined into the dependency map
in [feature-plan-2026-07.md](../feature-plan-2026-07.md), which orders twelve requested
capabilities by what each structurally needs, distinguishes the already-planned from
the genuinely new, and rules one item (AI-generated diet prescription) out of scope on
safety grounds ([ADR-0016](../adr/0016-nutrition-scope.md)). Functional requirements
trace to objectives O1–O5; the overriding non-functional requirements are tenant
isolation, auditability, and honesty of displayed data ("no screen shows a number it
cannot explain").

## 5. Design

### 5.1 Architecture

A modular monolith with strict inward-pointing dependencies:

```
crates/
  domain          — entities, invariants, pure logic; no IO
  application     — use-cases; owns transactions; depends on ports (traits)
  infrastructure  — SQLx repositories implementing the ports; migrations
  api             — Axum routes, extractors, OpenAPI
bins/server       — composition root
apps/mobile       — Expo client; types generated from the OpenAPI contract
```

The domain crate compiles without a database, which keeps its 163 unit tests instant
and honest; the API layer contains no business rules, only translation.

### 5.2 Tenancy and identity (O1, O2)

Every tenant-owned table carries `gym_id`, and every repository method takes tenant
context — `find_program(tenant, id)`, never `find_program(id)`. Above that,
**row-level security** enforces the same isolation in the database: the application
connects as a non-owner role (table owners bypass RLS silently), and tenant context is
set per transaction, because a pooled connection that remembers the previous request's
tenant is a cross-tenant leak. Person-owned data (accounts, profiles, measurements)
deliberately sits outside RLS — it has no tenant — and is guarded by the service layer
alone, a boundary stated rather than hidden.

Identity is **one account, many capacities**
([ADR-0014](../adr/0014-identity-capacities-and-profiles.md)): a person holds a set of
capacities per gym, so "trainer at two gyms" and "trainer who also trains here" are
ordinary states, not schema violations. Permission questions are answered only by
`Capabilities::can_*` methods; absence of standing yields 404, not 403, so tenant
existence is never confirmed to outsiders.

### 5.3 The programme model (O3)

`Program → ProgramVersion → Week → Workout → PrescribedExercise`, lifecycle
`Draft → In review → Approved → Published → Archived`, with one invariant above all:
**a published version is immutable** ([ADR-0006](../adr/0006-immutable-program-versioning.md)).
Editing creates a new draft; assignments pin the version they were given, so "what
exactly was this athlete's programme in March?" has a permanent answer.

The invariant is enforced twice. In the domain, following the types-as-propositions
tradition (Wadler, 2015), lifecycle states carry their evidence — `Published` contains
publisher and timestamp — and transitions out of terminal states do not exist to be
called. In the database, triggers reject mutation of published content, because the
application is not the only possible writer. Prescriptions are a modality-keyed enum
("4×8 reps over 5 km" is inexpressible), and validation is wired into the single
persistence path because serde deserialisation bypasses validating constructors — a
defect the test suite now pins (§7).

### 5.4 Coaching, assignment, execution (O4)

Coach–athlete relationships are gym-scoped, many-to-many, end-dated and never deleted,
so past work remains attributable. Two distinct authority questions are kept distinct:
*may view* (self, manager, or current coach) and *may coach* (coach or manager — self
grants nothing). Assignment pins a specific published version; a trigger re-validates
publishedness on insert.

Execution separates **prescribed** from **performed** permanently: sessions and sets
are append-only records, writable only by the athlete they belong to, with the
application role denied UPDATE/DELETE at the SQL-grant level. Sets carry RPE/RIR
(Emanuel et al., 2022). Identifiers are client-generated UUIDs (Davis, Peabody and
Leach, 2024) with idempotent inserts, so a replayed write is harmless — the primitive
on which the planned offline transport rests (Kleppmann et al., 2019; Bailis et al.,
2013; [ADR-0008](../adr/0008-offline-sync-operation-log.md)).

### 5.5 Computed member value (O5)

All derived numbers — estimated 1RM (Epley, capped at twelve reps; Grgic et al., 2020),
BMI, goal progress against a baseline captured at goal creation — are computed on read
from immutable records, never stored ([ADR-0018](../adr/0018-computed-progress-and-goals.md)).
Goals target only metrics the system can observe. Recommendations are deterministic
rules with a human-readable reason per suggestion
([ADR-0017](../adr/0017-deterministic-recommendations.md)).

## 6. Implementation notes

Four implementation matters deserve record beyond the design account:

1. **Authentication hardening.** Refresh-token rotation is compare-and-swap: the
   revocation query reports whether *this* call revoked, and losing the race is treated
   as theft, revoking the token family (Lodderstedt et al., 2025). The client
   de-duplicates concurrent refreshes; both sides exist because a race harness proved
   the naïve versions wrong.
2. **Audit.** Every tenant mutation writes its audit row in the same transaction as
   the change — a separately-written audit log can fail alone, which is the very case
   it exists for — and the application role cannot update or delete audit rows.
3. **Capacity-aware navigation.** The mobile tab bar derives from a pure manifest
   keyed on capacities in the active gym; hidden tabs are unmounted, not merely
   unlinked, so "hidden" and "unreachable" are the same fact. All 32 capacity
   combinations are asserted by script.
4. **Timezone honesty.** The server's "today" is UTC; members east of Greenwich live
   in tomorrow, so measurement dates tolerate +1 day — a defect found by using the
   system from a UTC+5 machine, now a named test. Timers are wall-clock-anchored so
   backgrounding the app loses no time.

## 7. Testing and evaluation

### 7.1 Evidence base

The verification inventory (16 suites, ~1,000 assertions, one gate:
`scripts/all-check.sh`):

| Suite | Checks | Objective | What it proves |
|---|---:|---|---|
| `cargo test` (workspace) | 163 | O2–O5 | Domain invariants, lifecycle, validation |
| `e2e.sh` | 49 | O1–O4 | Auth, tenancy, catalogue, audit over HTTP; pins the OpenAPI path count (39) |
| `verify-rls.sh` | 7 | O1 | DB-level isolation; fails closed; owner-bypass demonstrated |
| `verify-invitations.sh` | 22 | O2 | Email binding, single-use, uniform 404s |
| `verify-audit.sh` | 18 | O1 | Same-transaction writes; append-only grants |
| `verify-program-immutability.sh` | 22 | O3 | Triggers refuse mutation of published content |
| `verify-programs.sh` | 55 | O3 | Authoring lifecycle; review gates; prescription validation |
| `verify-coaching.sh` | 41 | O4 | Relationship lifecycle; per-client visibility |
| `verify-assignments.sh` | 24 | O4 | Version pinning; relationship-gated authority |
| `verify-execution.sh` | 39 | O4 | Idempotent writes; append-only sets; privilege matrix |
| `verify-profiles.sh` | 38 | O5 | Person-owned profiles; measurement date tolerance |
| `verify-goals.sh` | 20 | O5 | Baseline capture; computed progress; self-service |
| `verify-recommendations.sh` | 15 | O5 | Reasoned suggestions; retirement; empty over guessed |
| `verify-nav.mjs` | 252 | O2 | All capacity combinations → exact tab sets |
| `verify-activity.mjs` | 42 | O1 | Audit rendering incl. midnight/timezone cases |
| `verify-progress.mjs` + format/clock | 68 | O5 | Epley edge cases; best-set-by-estimate; timer arithmetic |

### 7.2 Evaluation against objectives

O1–O4 are met and doubly enforced (application and database), with the refusals — not
just the successes — under test. O5 is met for progress, measurements, goals and
recommendations; readiness and adherence metrics remain future work. O6 is the
methodology itself: each row above is reproducible by a marker with one command.

The defects table in [project-log.md](project-log.md) is the candid part of the
evaluation: ten representative failures — a token-rotation race, a serde validation
bypass, vacuous SQL assertions, a privilege leak — each found by the evidence base and
each now pinned as a regression. The methodology's value is measured by what it caught.

### 7.3 Limitations

The offline transport (queue and replay) is designed but unbuilt; the write path is
idempotent in anticipation. Billing is decided ([ADR-0010](../adr/0010-payments-and-billing.md))
but stubbed. The web build is a development surface, not the product's web client.
Native builds were exercised on Android via Expo Go over LAN, not on iOS. Estimated
1RM is an estimator and is labelled as such in the interface.

## 8. Professional, ethical and legal considerations

**Academic integrity.** Generative AI use in this project is declared and attributed
(§3; [project-log.md](project-log.md)), consistent with UWE Bristol's principles, under
which unacknowledged AI-created work constitutes an assessment offence (UWE Bristol,
2026a; 2026b).

**Data protection.** The design minimises and separates personal data: profiles are
person-owned; rosters expose no email addresses; tokens are stored only as hashes;
passwords are Argon2id-hashed (Biryukov et al., 2021) and never logged. All development
data is synthetic. A production deployment would require a lawful-basis analysis and
retention policy under UK GDPR — body measurements are health-adjacent data and would
warrant special-category handling; this is documented as deployment-blocking future
work rather than silently ignored.

**AI in the product.** The platform's own AI ambitions are bounded *in advance*: a
tiered authority model with deterministic validation
([ADR-0007](../adr/0007-ai-authority-levels.md)), no model-driven output in the current
product (the recommender is deterministic by decision, [ADR-0017](../adr/0017-deterministic-recommendations.md)),
an explicit refusal of generated diet prescription on safety grounds
([ADR-0016](../adr/0016-nutrition-scope.md)), and a roadmap commitment that the first
model-driven feature ships with EU AI Act Article 50 disclosure (European Union, 2024),
not ahead of it.

**Security ethics.** The platform holds positions of trust (coach over athlete, gym
over member); the authorization model treats those as distinct, auditable authorities,
and the audit log is tamper-evident against the application's own credentials.

**Accessibility.** Interactive controls carry accessibility labels and 44-pt targets;
a full WCAG 2.1 AA pass (W3C, 2018) is scheduled as its own roadmap item (UX-5) on the
stated principle that retrofitted accessibility is how accessibility fails to happen.

## 9. Conclusion and future work

The project set out to prove that a coaching platform's core — tenancy, identity,
immutable programmes, authorised coaching, trustworthy execution history — could be
built small, typed and verifiable, and to make every architectural claim executable.
That standard held: the system runs end to end, one binary serves five materially
different user experiences from one identity model, and the sixteen-suite evidence base
reproduces every claim in this report.

Future work follows the roadmap's dependency order: the entitlement read model (feature
gating ahead of billing), the offline operation-log transport with per-entity conflict
resolution (Preguiça, 2018; Bailis et al., 2013), the outbox worker and reminders,
training-load monitoring (Halson, 2014), the operating calendar
([ADR-0015](../adr/0015-gym-operating-calendar.md)), and — last, deliberately — the
bounded AI assistant, arriving into guardrails that already exist.

## References

Bailis, P., Davidson, A., Fekete, A., Ghodsi, A., Hellerstein, J.M. and Stoica, I.
(2013) *Highly Available Transactions: Virtues and Limitations*. arXiv:1302.0309.
[research/02](../../research/02-highly-available-transactions-bailis-2013.pdf).

Biryukov, A., Dinu, D., Khovratovich, D. and Josefsson, S. (2021) *RFC 9106: Argon2
Memory-Hard Function for Password Hashing and Proof-of-Work Applications*. IETF.
[research/07](../../research/07-rfc9106-argon2.pdf).

Davis, K., Peabody, B. and Leach, P. (2024) *RFC 9562: Universally Unique IDentifiers
(UUIDs)*. IETF. [research/14](../../research/14-rfc9562-uuidv7.pdf).

Dragoni, N., Giallorenzo, S., Lafuente, A.L., Mazzara, M., Montesi, F., Mustafin, R.
and Safina, L. (2016) *Microservices: yesterday, today, and tomorrow*.
arXiv:1606.04036. [research/04](../../research/04-microservices-dragoni-2016.pdf).

Emanuel, A., Har-Nir, I., Obolski, U. and Halperin, I. (2022) 'Seeing Effort:
Assessing Coaches' Prediction of the Number of Repetitions in Reserve Before
Task-Failure', *Sports Medicine – Open*, 8.
[research/10](../../research/10-seeing-effort-rir-coaches-2022.pdf).

European Union (2024) *Regulation (EU) 2024/1689 (Artificial Intelligence Act),
Article 50: Transparency obligations*. Official Journal of the European Union.

Evans, A.N., Campbell, B. and Soffa, M.L. (2020) 'Is Rust Used Safely by Software
Developers?', *Proceedings of ICSE 2020*. arXiv:2007.00752.
[research/05](../../research/05-rust-safety-evans-2020.pdf).

Fielding, R.T. (2000) *Architectural Styles and the Design of Network-based Software
Architectures*. PhD dissertation, University of California, Irvine.
[research/13](../../research/13-rest-fielding-dissertation-2000.pdf).

Grgic, J., Lazinica, B., Schoenfeld, B.J. and Pedisic, Z. (2020) 'Test–Retest
Reliability of the One-Repetition Maximum (1RM) Strength Assessment: A Systematic
Review', *Sports Medicine – Open*, 6(31).
[research/11](../../research/11-1rm-reliability-grgic-2020.pdf).

Halson, S.L. (2014) 'Monitoring Training Load to Understand Fatigue in Athletes',
*Sports Medicine*, 44(S2), pp. 139–147.
[research/15](../../research/15-training-load-monitoring-halson-2014.pdf).

Jones, M., Bradley, J. and Sakimura, N. (2015) *RFC 7519: JSON Web Token (JWT)*. IETF.
Available at: https://www.rfc-editor.org/rfc/rfc7519 (Accessed: 19 July 2026).

Jung, R., Jourdan, J.-H., Krebbers, R. and Dreyer, D. (2018) 'RustBelt: Securing the
Foundations of the Rust Programming Language', *Proceedings of the ACM on Programming
Languages*, 2(POPL). [research/12](../../research/12-rustbelt-jung-2018.pdf).

Kleppmann, M., Wiggins, A., van Hardenberg, P. and McGranaghan, M. (2019)
'Local-first software: You own your data, in spite of the cloud', *Onward! 2019*.
[research/03](../../research/03-local-first-kleppmann-2019.pdf).

Lodderstedt, T., Bradley, J., Labunets, A. and Fett, D. (2025) *RFC 9700: Best
Current Practice for OAuth 2.0 Security*. IETF.
[research/08](../../research/08-rfc9700-oauth2-security-bcp.pdf).

Nygard, M. (2011) *Documenting Architecture Decisions*. Available at:
https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
(Accessed: 19 July 2026).

OWASP (2021) *Application Security Verification Standard 4.0.3*. OWASP Foundation.
[research/09](../../research/09-owasp-asvs-4.0.3.pdf).

PostgreSQL Global Development Group (2026) *PostgreSQL 17 Documentation: Row Security
Policies*. Available at: https://www.postgresql.org/docs/17/ddl-rowsecurity.html
(Accessed: 19 July 2026).

Preguiça, N. (2018) *Conflict-free Replicated Data Types: An Overview*.
arXiv:1806.10254. [research/01](../../research/01-crdt-overview-preguica-2018.pdf).

UWE Bristol (2026a) *Principles for using generative artificial intelligence (AI)*.
Available at: https://www.uwe.ac.uk/study/academic-information/artificial-intelligence-principles
(Accessed: 19 July 2026).

UWE Bristol (2026b) *Using generative AI at UWE Bristol*. Available at:
https://www.uwe.ac.uk/study/study-support/study-skills/using-generative-ai-at-uwe-bristol
(Accessed: 19 July 2026).

W3C (2018) *Web Content Accessibility Guidelines (WCAG) 2.1*. W3C Recommendation.
Available at: https://www.w3.org/TR/WCAG21/ (Accessed: 19 July 2026).

Wadler, P. (2015) 'Propositions as Types', *Communications of the ACM*, 58(12),
pp. 75–84. [research/16](../../research/16-propositions-as-types-wadler-2015.pdf).

Zhang, S., Yao, L., Sun, A. and Tay, Y. (2019) 'Deep Learning based Recommender
System: A Survey and New Perspectives', *ACM Computing Surveys*, 52(1).
[research/06](../../research/06-deep-learning-recsys-survey-zhang-2017.pdf).

## Appendices (by reference)

- **A. Development record and AI declaration** — [project-log.md](project-log.md)
- **B. Architecture decision records** — [adr/README.md](../adr/README.md) (ADR-0001 … ADR-0019)
- **C. Screenshot corpus** — [screenshots/README.md](../../screenshots/README.md) (44 captures, per role)
- **D. Research library** — [research/INDEX.md](../../research/INDEX.md) (16 vendored primary sources)
- **E. Demo accounts and guided tour** — [test-accounts.md](../test-accounts.md)
