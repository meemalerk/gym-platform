# Cost Analysis & Profitability Model

> **These are illustrative models built on July-2026 research, not forecasts.** Every number
> carries assumptions stated inline — change the assumptions and the outputs change. Sources:
> [hosting-deployment.md](hosting-deployment.md) (infra), [market-analysis.md](market-analysis.md)
> (pricing/benchmarks), [research-2026.md](research-2026.md) §2a (AI). Re-verify prices before
> relying on them.
>
> **⚠ Flagged 2026-08-23** ([ADR-0023](adr/0023-single-gym-deployment.md)): the unit-economics
> tables below (CAC/LTV/MRR by account count) assume many gym-org customers on one shared
> platform. Each deployment now serves a single gym — see [market-analysis.md](market-analysis.md)'s
> matching flag. Not reconciled here; treat the per-account figures as pre-pivot.

---

## 1. The headline: this is a high-margin, low-fixed-cost business

Three facts from the research drive everything:

1. **Infra is cheap** — Hetzner + free tiers put MVP at **~$0–25/mo** and early production at
   **~$100–450/mo**.
2. **AI is nearly free at our scale** — the model is a small open-weight model; per-token
   serverless open-model inference costs **fractions of a cent per active member per month**
   (math below). It is *not* a meaningful COGS line until very large scale.
3. **We never touch card data and payments are pass-through** — Stripe fees are the gym's cost,
   not ours (and the Connect application fee is *revenue*, [ADR-0010](adr/0010-payments-and-billing.md)).

Net: gross margins are **SaaS-typical 80–90%**, fixed costs are trivially small, and the real
constraints are **customer acquisition and founder time**, not infrastructure.

---

## 2. AI cost — the decision, with the actual math

**Correction to earlier assumption:** self-hosting a GPU is **not** the cheapest AI option at
our scale. Cheap serverless open-model APIs are.

### Per-token pricing (Qwen3.5-9B-class, July 2026)
| Provider | $/M in | $/M out | Privacy |
|----------|-------:|--------:|---------|
| DeepInfra (Llama-3.1-8B) | $0.02 | $0.03 | zero-retention default, SOC2/ISO |
| Together AI (Qwen3.5-9B) | $0.17 | $0.25 | no-train; ZDR opt-in |
| Together / Groq (gpt-oss-20B) | $0.05–0.075 | $0.20–0.30 | check ToS |
| OpenRouter (Qwen3.5-9B routed) | $0.10 | $0.15 | depends on backend |

### Cost per active member per month
Assume a heavy **20 AI interactions/member/month × ~1,500 tokens each = 30,000 tokens/member/mo**.
At a blended **~$0.15/M tokens**:

```
30,000 tokens × $0.15 / 1,000,000  ≈  $0.0045 per member per month
```

**~half a cent per active member per month.** Even **10,000 active members ≈ $45/month** of AI.
This is a rounding error against subscription revenue.

### Serverless vs self-hosted GPU — breakeven
- Self-hosted 24 GB GPU: **~$248/mo** (RunPod RTX 4090 24/7) or **~$75–150/mo** business-hours.
- Breakeven vs serverless: **~1–4 billion tokens/month** at high GPU utilization. Real early
  traffic (bursty, 5–25% utilization) pushes the effective crossover **4–10× higher**.
- A *successful* app (1,000 DAU × 20 interactions × 1,500 tokens ≈ **900M tokens/mo**) is still
  below breakeven.

### Decision (updates [ADR-0011](adr/0011-self-hosted-open-llm.md))
- **Start on cheap serverless open-model inference** (DeepInfra primary for price + zero-retention
  privacy; Together as verified fallback; benchmark **gpt-oss-20B** — cheaper per token and ties
  on function-calling). Behind our `genai`/`async-openai` abstraction so switching is trivial.
- **Self-host a GPU only** at sustained high volume (low billions of tokens/mo) **or** if
  data-control/privacy policy requires health data never leave our infra. That is a
  *deliberate* choice, not the cost-minimizing default.
- **Free hosted tiers (Groq/Gemini/OpenRouter)** = dev/prototype only — **never real client
  health data** (they log/train on prompts).
- **Domain knowledge → RAG over pgvector**, not fine-tuning (confirmed 2026 best practice — your
  "feed our data into our own DB" instinct is correct; details in [ADR-0012](adr/0012-domain-data-rag.md)).
- **Embeddings:** cheap hosted (Voyage-4-lite / OpenAI-3-small, **$0.02/M**) — corpus is tiny;
  self-hosting BGE-M3 isn't worth the ops until tens of millions of tokens/mo re-embedded.

**Bottom line:** budget **AI COGS ≈ $5–50/month** through early growth. It is not a cost driver.

---

## 3. Monthly cost stack by stage

| Line item | MVP | Early (few gyms) | Growth (dozens) |
|-----------|----:|-----------------:|----------------:|
| Compute (Hetzner) | $0–5 | $10–30 | $60–150 |
| Postgres | $0 (free/self) | $30–60 (managed HA) | $150–300 |
| **AI inference** | **$0–20** | **$5–50** (serverless) | **$50–250** |
| Object storage/CDN (R2) | $0 | $5–15 | $30–100 |
| Email / push | $0 | $0–20 | $20–100 |
| Errors / secrets / CI | $0 | $0–30 | $50–200 |
| Monitoring / misc | $0 | $10–20 | $50–150 |
| **Total infra** | **~$0–25** | **~$100–250** | **~$900–2,500+** |

> If you self-host a GPU instead of serverless AI, add ~$200–250/mo to the Early stage (which is
> why serverless wins early). Growth-stage range assumes managed HA Postgres + possible GPU +
> paid tiers.

---

## 4. Pricing model (illustrative — grounded in [market-analysis.md](market-analysis.md))

Designed to undercut fragmented incumbents with **transparent, low-add-on** pricing:

| Plan | Target | Price | Includes |
|------|--------|------:|----------|
| **Solo** | independent trainer | **$29/mo** | up to ~15 active clients, full features, AI assistant |
| **Studio** | single-location gym/studio | **$99/mo** + **$2/active client** over 25 | branches:1, staff, programming, AI |
| **Gym** | multi-branch | **$249/mo** + per-branch | multi-branch RBAC, head-coach review, AI |
| **Enterprise** | chains | custom | SSO/SCIM, data residency, SLA |
| **Payments** | all | **Stripe Connect application fee ~0.5–1%** on gym→member volume | expansion revenue ([ADR-0010](adr/0010-payments-and-billing.md)) |

Principles: **no feature-gating of core capabilities** (differentiation vs Trainerize/Mindbody);
AI is **bundled into tiers, not metered** (its cost is negligible); a **modest** payments fee,
not Mindbody's ~20% marketplace take.

---

## 5. Unit economics (per gym customer, illustrative)

Assume an average paying account (a small studio) at **~$150/mo blended** (subscription + a few
per-client seats), with **~30 active members**:

```
Revenue / account / month           $150
COGS / account / month:
  - infra share (amortized)          ~$3
  - AI inference (30 members)        ~$0.15
  - support (amortized)              ~$10
  - Stripe fees on OUR $150 charge   ~$4.65  (2.9% + $0.30 — our cost of collecting)
  ---------------------------------------
  Total COGS                         ~$18
Gross profit / account              ~$132   →  ~88% gross margin
```

**AI is $0.15 of a $150 account.** Payment processing on our own subscription charge is the
biggest COGS line — still leaves ~88% margin.

### CAC / LTV
- **CAC** (SMB SaaS): **$200–700** (content/community-led can be lower in this niche).
- **Churn**: model B2B (gym) churn at **~5%/mo** → avg lifetime ~20 months.
- **LTV** = $150 × 20 × 88% ≈ **$2,640**.
- **LTV:CAC** ≈ **3.8–13:1** (target ≥3:1) — healthy, *if* CAC stays disciplined.
- **~30–40% of member cancellations are involuntary** → our dunning feature is a real
  retention/upsell lever.

---

## 6. Break-even & profitability scenarios (illustrative)

**Break-even on cash infra costs is trivial:** at ~$150/account and ~$250/mo early infra,
**2 paying accounts cover infrastructure.** The real "salary break-even" is what matters:

| Scenario | Accounts | Avg $/mo | MRR | Infra | +Payments fee¹ | Gross profit/mo |
|----------|---------:|---------:|----:|------:|---------------:|----------------:|
| **Ramen** | 15 | $150 | $2,250 | ~$250 | ~$150 | **~$2,000** |
| **Traction** | 60 | $180 | $10,800 | ~$800 | ~$900 | **~$10,000** |
| **Growth** | 250 | $220 | $55,000 | ~$2,200 | ~$5,000 | **~$52,000** |

¹ Payments fee ≈ 0.5% of gym→member GMV. Assumes avg gym processes ~$8–12k/mo through us;
scales with account count. At Growth: ~$1M/mo GMV × 0.5% ≈ $5k/mo.

**Reading it:**
- **Ramen (15 accounts)** covers a lean solo founder's costs — reachable early because fixed
  costs are tiny.
- **Traction (60 accounts)** ≈ $10k/mo gross — a real one-person business or small team.
- **Growth (250 accounts)** ≈ $52k/mo gross, ~88% margin — now CAC and team are the spend, not
  infra.

The **payments application fee becomes material at scale** — at Growth it's ~10% of revenue and
nearly pure margin. This is the same lever incumbents use; we apply it modestly.

---

## 7. What actually determines success (not infra)

1. **CAC discipline.** Infra is noise; customer acquisition is the cost. Content, community
   (coaching/CrossFit/PT circles), and gym-owner word-of-mouth are the cheap channels.
2. **Retention.** Gym-*businesses* churn less than gym-*members*, but SMB churn is still 5–7%.
   The versioning/AI/offline differentiation must translate into stickiness.
3. **Positioning proof.** The "no incumbent does multi-tenant + versioned programming + bounded
   AI + offline" thesis is **unvalidated** — do live competitor teardowns before over-investing.
4. **Payments attach rate.** Getting gyms to run member billing through our Stripe Connect is
   the expansion-revenue flywheel; if they keep external billing, we lose that ~10%.

## 8. Sensitivities / risks to the model

- **CAC blowout** is the #1 risk — at CAC $700 and higher churn, LTV:CAC compresses toward 3:1.
- **Market-size figures are unreliable** ([market-analysis.md](market-analysis.md)) — don't
  build projections on a specific TAM number.
- **App-store / payments law is in flux** ([ADR-0010](adr/0010-payments-and-billing.md)) — the
  IAP-exemption for memberships is the assumption that keeps payments margin; re-verify.
- **AI cost** only becomes real if usage explodes to billions of tokens/mo — at which point
  self-hosting flips to cheaper (a good problem).
- **Hetzner price hikes** (twice in 2026) — cheap infra is cheap *today*; keep the stack
  portable (Docker) so we're not locked in.
