# ADR-0012: Domain knowledge via RAG over our own DB (not fine-tuning)

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author
- **Informed by:** [research-2026.md](../research-2026.md) §2a, [cost-analysis.md](../cost-analysis.md) §2

## Context

We want the AI assistant to reason over domain-specific knowledge — the exercise library,
coaching methodology, our own programme templates, a member's history. The question was whether
to bake that knowledge into the model (fine-tuning) or retrieve it at inference time (RAG), and
the user's instinct was to "feed our domain data into our own DB to make retrieval and
organization easier."

## Decision

**Store domain knowledge as structured data in our own PostgreSQL and inject it via retrieval
(RAG), not by fine-tuning the model.** The user's instinct is the 2026 best practice.

- **Structured rows in Postgres** for the exercise library, constraints, alternatives,
  coaching rules, and programme templates — the same tables already in
  [domain-model.md](../domain-model.md). These are queried with **plain SQL** for exact filters
  (equipment available, muscle group, difficulty, contraindications).
- **pgvector** (same Postgres instance — no separate vector DB) for the **semantic layer**:
  e.g. "member describes knee discomfort" → candidate substitution exercises by similarity.
- **Hybrid retrieval:** SQL structured filter **+** vector similarity re-rank. Cheaper, more
  maintainable, and more *auditable* than pure embedding search — and far cheaper/more reliable
  than fine-tuning knowledge into weights.
- The retrieved, exact domain facts go into the model's context; the model proposes; **Rust
  validates** ([ai-authority-model.md](../ai-authority-model.md)). Domain facts come from our
  DB, not model weights — the same trust reason the model is an untrusted proposer.
- **Fine-tuning / LoRA is reserved for output-format compliance only** (e.g. stamping out a
  small model's occasional malformed-JSON/XML tool-call), *not* knowledge injection. If done at
  all: cheap via Unsloth self-run (~$5–10) or HF AutoTrain (~$2–30), served as a LoRA adapter on
  Fireworks (base-model token rates, no dedicated GPU). Deferred until a real format-reliability
  problem is measured.
- **Embeddings:** cheap hosted API (Voyage-4-lite / OpenAI-3-small, ~$0.02/M) — the corpus is
  small and re-embedded rarely; self-hosting BGE-M3 isn't worth the ops overhead yet.

## Alternatives considered

- **Fine-tune the knowledge into the model** — rejected: fine-tuning doesn't reliably add
  factual/domain knowledge (it locks in tone/format), can't be updated without retraining,
  isn't auditable, and locks us to a base model. Wrong tool for a changing exercise library.
- **Pure embedding-search RAG (no structured layer)** — weaker than hybrid: loses exact
  filtering (equipment/contraindication constraints must be exact, not fuzzy).
- **Dedicated vector DB (Qdrant/Milvus/Pinecone)** — unnecessary infra and cost; pgvector on our
  existing Postgres is sufficient at this scale.

## Consequences

- **Positive:** domain data lives in one place (Postgres) with exact + semantic retrieval;
  updates are just data writes (no retraining); retrieval is auditable (important for
  health-adjacent recommendations); no new infra; near-zero cost; base model is swappable.
- **Negative / costs:** we build and maintain the retrieval layer and keep embeddings in sync
  when the library changes; hybrid ranking needs tuning.
- **Follow-ups:** design the exercise/template embedding + re-embed-on-change pipeline in Phase 5;
  measure JSON-format error rates before considering any LoRA.

## References

- [research-2026.md](../research-2026.md) §2a, [cost-analysis.md](../cost-analysis.md),
  [ai-authority-model.md](../ai-authority-model.md), [domain-model.md](../domain-model.md),
  [ADR-0011](0011-self-hosted-open-llm.md)
