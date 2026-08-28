# ADR-0011: Self-hosted open-weight LLM (no per-token AI fees)

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author
- **Supersedes:** the "Claude Opus 4.8 / OpenAI GPT-5.5" model choice previously noted in
  [tech-stack.md](../tech-stack.md) and [research-2026.md](../research-2026.md) §2.
- **Informed by:** [research-2026.md](../research-2026.md) §2a (self-hosted LLM research)

## Context

We do not want a per-token AI bill or dependence on a paid frontier API. A frontier model
(Claude/GPT) is overkill for this product for an architectural reason, not just a cost one:
the AI is designed as an **untrusted proposer**. It emits a schema-constrained JSON "proposed
training adjustment", and **deterministic Rust code does the actual validation and safety
gating** ([ai-authority-model.md](../ai-authority-model.md), [ADR-0007](0007-ai-authority-levels.md)).
So the model bar is not frontier reasoning — it is (a) reliable grammar-constrained JSON, (b)
adequate tool/function-calling to gather context, (c) cheap to serve.

The decisive 2026 finding: **all major self-hosted serving stacks (llama.cpp, Ollama, vLLM,
SGLang) can now GUARANTEE JSON-schema-conformant output via grammar-constrained decoding**
(GBNF / XGrammar / Outlines). This is mature, production-proven, not experimental — which is
exactly the property our design relies on.

## Decision

Run a **small open-weight model on our own infrastructure**, behind an OpenAI-compatible
endpoint, with grammar-constrained decoding.

- **Model:** **Qwen3.5-9B** (Apache 2.0) primary — best-documented tool-calling lineage at its
  size. Alternatives with equally permissive licenses: **gpt-oss-20b** (native tool-use,
  MXFP4 ~16GB), **Granite 4.1-8B** (IBM, explicit tool-calling + compliance posture),
  **Mistral Small 4**. Smaller fallback for tight VRAM/CPU: **Qwen3.5-4B** or the
  function-calling-tuned **xLAM-2-3b**. MoE headroom option: **Qwen3.5-35B-A3B** (~3B active).
- **Serving:** **Ollama** for local dev (near-zero setup, same schema-constrained `format`
  param); **vLLM or SGLang** for production (SGLang + XGrammar is measured fastest for a high
  volume of small structured-JSON completions — our exact workload).
- **Constrained decoding:** pass the Rust-side JSON Schema straight through as `guided_json`
  (vLLM/SGLang) / `format` (Ollama). Do not hand-roll grammars.
- **Rust glue:** call the OpenAI-compatible endpoint via **`async-openai`** or **`genai`** (the
  latter keeps a hosted fallback behind the same abstraction). In-process Rust inference
  (`mistral.rs`/`candle`) is deferred — unnecessary complexity for v1.
- **RAG/embeddings (when needed):** self-host **BGE-M3** (or nomic/Qwen-embed) served via
  Ollama, vectors in **pgvector on our existing Postgres** — no new infra, no paid vector DB.
- **Provider abstraction is mandatory** so we can swap models or fall back without touching
  domain code.

## Alternatives considered

- **Paid frontier API (Claude/OpenAI/Gemini)** — rejected as default: per-token cost and
  external dependency for a task where a small local model + Rust validation suffices. Kept
  only as an optional swap-in behind the abstraction.
- **Free-tier hosted inference** (Google AI Studio, Groq, OpenRouter, Cloudflare Workers AI) —
  usable for **prototyping only**. **Must NOT receive real client health/training data** —
  free tiers commonly log/train on prompts. Not a production answer for this app's data.
- **In-process Rust inference (mistral.rs/candle)** — viable but adds complexity; revisit only
  to cut the network hop.

## Consequences

- **Positive:** no per-token fees; client health data stays on our infrastructure (helps GDPR
  Art. 9 / privacy — see [research-2026.md](../research-2026.md) §5); model choice decoupled
  from code; constrained decoding gives guaranteed-shape output that our Rust validator gates.
- **Negative / costs — self-hosting is NOT literally free:** either a one-time GPU purchase
  (a single **24GB RTX 4090 ≈ $1.5–2k** runs the whole 7–14B / MoE-3B-active tier **plus**
  embeddings) or **~$250–500/month rented** (RunPod/Vast) if we don't own hardware, **plus our
  own ops/monitoring time**. The 1–4B tier can run CPU-only for dev/low volume (multi-second
  latencies). "No marginal per-token cost", not "free".
- **Follow-ups:** benchmark the chosen model against our actual "proposed adjustment" schema
  (constrained-decoding edge cases exist); decide owned-vs-rented GPU at Phase 5; keep the
  free hosted tier wired only for dev.

## Update (2026-07-14): serverless open-model is cheaper than self-hosting at our scale

Follow-up cost research ([cost-analysis.md](../cost-analysis.md) §2) refined the *deployment*
choice (not the "open-weight, not frontier" decision, which stands):

- **Cheap serverless open-model APIs** (DeepInfra ~$0.02–0.03/M, Together ~$0.17/$0.25,
  gpt-oss-20B ~$0.05/$0.20) cost **~half a cent per active member per month** — a few dollars
  total at early scale. **Breakeven vs a dedicated GPU is ~1–4 billion tokens/month**, far
  beyond early usage.
- **Therefore: start on serverless open-model inference** (DeepInfra primary — cheapest +
  zero-retention privacy default; Together fallback; benchmark gpt-oss-20B). Keep it behind the
  `genai`/`async-openai` abstraction so switching is trivial.
- **Self-host a GPU** (RunPod RTX 4090 ~$248/mo, or ~$75–150/mo business-hours; Hetzner GEX44
  flat ~$200/mo/20 GB) **only** at sustained high volume **or** when data-control/privacy policy
  requires health data never leave our infra. That is the one strong non-cost reason to
  self-host early — a deliberate tradeoff, not the default.

Net: "no per-token *frontier* bill" is achieved either way; the cost-minimizing path at low
scale is actually cheap open-model serverless, with self-hosting as a scale/privacy option.

## Note on reducing AI dependence

Much of the product's "intelligence" is deterministic domain logic, not the LLM. Prefer
rule-based progression/validation where it fits; the LLM is the talkative doorway, and keeping
its role narrow also keeps the self-hosting bill small.

## References

- [research-2026.md](../research-2026.md) §2 / §2a, [ai-authority-model.md](../ai-authority-model.md),
  [ADR-0007](0007-ai-authority-levels.md)
