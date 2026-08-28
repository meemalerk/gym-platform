# AI Authority Model

The AI's power is **explicitly granted, bounded, deterministically validated, and audited**.
This is what separates the product from a generic fitness chatbot. Reasoning:
[ADR-0007](adr/0007-ai-authority-levels.md).

## Authority levels

Each gym, instructor, and member can be assigned a different level.

### Level 0 — Explain only
The assistant explains the instructor's programme. It cannot alter anything.

### Level 1 — Suggest
It proposes a substitution or adjustment that requires member or instructor approval before
taking effect.

### Level 2 — Adjust within guardrails
The instructor defines boundaries; inside them the AI may act.

```
AI MAY:
  - reduce working weight by up to N%   (instructor-set, e.g. 10%)
  - remove one working set
  - replace an exercise only from an approved alternatives pool
  - move a session by up to two days
  - reduce intensity when readiness is below a threshold

AI MAY NOT:
  - add new exercises
  - increase weekly volume
  - change rehabilitation exercises
  - modify testing sessions
  - override instructor-locked blocks
```

### Level 3 — Autonomous programme generation
For independent members with no instructor. Still validated by deterministic domain rules;
autonomy is not a bypass of validation.

## Policy is per gym / instructor / member

```rust
pub struct AssistantAuthority {
    pub may_explain: bool,
    pub may_suggest_changes: bool,
    pub may_apply_session_adjustments: bool,
    pub max_load_reduction_percent: u8,
    pub may_substitute_from_approved_pool: bool,
    pub requires_coach_approval_for_program_changes: bool,
}
```

## Orchestration flow (inside the backend boundary)

```
User message
  → intent classification
  → permission check                (authz — may this actor + is AI allowed?)
  → load relevant training context
  → call approved domain tools
  → validate proposed action        (deterministic domain policy)
  → persist decision and evidence   (audit)
  → return explanation
```

## Tool contract

The model is given **narrow business operations only**. Suggested toolset:

```
get_member_summary            get_active_program
get_recent_sessions           get_readiness_history
get_exercise_constraints      get_instructor_ai_policy
suggest_exercise_substitution calculate_training_load
simulate_program_adjustment   request_coach_approval
apply_session_adjustment
```

**Never** expose raw mutation tools:

```
execute_sql        ✗
update_workout     ✗
delete_program     ✗
```

## Model: self-hosted, small, open-weight

Because the model is only ever a proposer gated by deterministic Rust, we run a **small
self-hosted open model** (Qwen3.5-9B or similar) with **grammar-constrained decoding** — no
paid frontier API, no per-token fee. The model bar is "reliable schema-conformant JSON +
adequate tool-calling", not frontier reasoning. See [ADR-0011](adr/0011-self-hosted-open-llm.md).

## Structured output is necessary but not sufficient

Use **grammar-constrained decoding** (self-hosted: llama.cpp GBNF / vLLM / SGLang+XGrammar —
pass the JSON Schema straight through) so the model *cannot* emit non-conformant output — but
the output still passes through domain validation before it can take effect:

```rust
let proposal = ai.propose_adjustment(context).await?;
let validated = workout_policy.validate(proposal)?;      // deterministic gate

match validated.approval {
    ApprovalRequirement::Automatic => apply(validated).await?,       // within guardrails
    ApprovalRequirement::Coach     => queue_for_review(validated).await?,  // route to coach
}
```

Schema adherence guarantees shape, not safety. The domain policy is the safety gate.

## Audit trail

Every AI decision is recorded (tables in [domain-model.md](domain-model.md)):

```
recommendation_runs · tool_invocations · recommendation_evidence · policy_violations
```

Health-adjacent recommendations must be auditable. Store the decision, the evidence it was
based on, the tools invoked, and any policy violations that routed it to a human.

## Guiding stance

The competitive advantage is the programme model, instructor workflow, adjustment engine,
and longitudinal data — **not** the chatbot. Keep the AI a talkative, well-governed doorway
into deterministic systems. Instructors stay in control of what it may change.
