# Market Analysis

> Sourced from July-2026 research ([research-2026.md](research-2026.md) is the tech companion;
> this is the market companion). **Market-size figures in this space are inconsistent across
> analysts (definitions vary 5–10×) — treat absolute $ figures as order-of-magnitude only.**
> Competitor prices were pulled from pricing pages in July 2026; re-check before quoting.
>
> **⚠ Flagged 2026-08-23** ([ADR-0023](adr/0023-single-gym-deployment.md)): this analysis
> positions the product as B2B2C SaaS sold to *many* gym-org customers. The codebase now
> caps each deployment to a single gym. Whether that means "sell one deployment per gym
> customer" (same business model, different packaging) or something else is an open
> business-strategy question this document has not been reconciled with yet — read the
> pricing-tier and competitor-positioning sections below with that in mind.

## Market size & growth (directional)

| Segment | Rough size / growth | Confidence |
|---------|--------------------|-----------|
| Gym/fitness **management software** | double-digit CAGR **~10–15%**; absolute $ disputed | low on absolute size |
| Online **coaching / PT software** | high-single to low-double-digit CAGR (~4–11%) | low (definition variance) |
| **Fitness app** market (most consistent) | ~$12–14B (2025→26), **~12–14% CAGR** (Grand View, Polaris) | medium |

**Use ~12–14% CAGR as the base-case growth assumption.** Do not cite a single absolute
market-size number without the caveat.

## Competitive landscape

The market splits into two camps that **don't overlap well** — which is the opportunity:

### Coaching-focused tools (per-client pricing)
Trainerize, TrueCoach, TrainHeroic, Everfit, CoachRx, Hevy Coach, FitBudd, My PT Hub,
PTDistinction.

- **Model:** client-count banding, often + a per-extra-client marginal rate.
- **Effective price:** ~**$1.60–$6 / active client / month**, declining toward **$2–3** at scale.
- **Entry:** ~**$20–30/mo** for solo trainers; **$60–160/mo** all-in for 20–50-client coaches.
- **Free tiers** common as a wedge (Trainerize, Everfit, FitBudd — capped 1–5 clients).
- **Add-on fragmentation** is rampant and a top complaint: nutrition, payments, branded app,
  automation each unbundled at **$8–45/mo** — real spend is 2–3× the "from $X" headline.
- Programs are **flat templates you clone-and-edit** — no real versioning/lineage.

### Gym/studio management tools (flat tier by member count)
Mindbody, Glofox, PushPress, Zen Planner, Wodify, GymMaster, Exercise.com, TeamUp, Gymdesk.

- **Model:** flat tier by active-member count; **payments processing is a major profit center.**
- **Effective spend:** **$150–400/mo** single-location; **$1,000–1,700/mo** for a 2-location
  boutique once processing + marketing add-ons are included.
- **Payments take-rate is the big hidden lever:** Mindbody charges ~2.75–3.5% + a **~20%
  marketplace commission** (capped) on app-driven bookings; PushPress/Glofox take smaller
  processing spreads; Gymdesk/Zen Planner/TeamUp lean closer to pure flat fee.
- Multi-location exists, but **structured/versioned programming is weak or absent.**

### Reference price points (July 2026, spot-checked)
- **Trainerize:** Free (1 client) → Pro bands $23–275/mo → Studio $248/mo/location. Raised
  prices twice in 20 months.
- **TrueCoach:** $26→$137/mo by client cap; **new 5% flat payments fee (Jan 2026).**
- **CoachRx / Gymdesk / TeamUp:** explicitly market **"no feature gating"** — a reaction to
  buyer fatigue with add-ons.
- **Mindbody:** ~$99–699/mo tiers + heavy payments take.
- **PushPress:** Free (4.19% processing) → $159 → $229/mo.
- **Consumer AI-coaching apps** (WTP signal): Fitbod $9.99/mo, Juggernaut AI $35/mo, Future
  (human+AI) $199/mo — pure-AI coaching prices at **$10–35/mo**.

## Positioning gap (our thesis)

No incumbent clearly occupies **all** of:
1. **Deep multi-tenant B2B2C** (org → branch → staff → member with real RBAC), *and*
2. **Structured, versioned programming** (immutable versions, review/approval, lineage), *and*
3. **Native AI bounded-adjustment/autoregulation** (validated, auditable), *and*
4. **Offline-first execution.**

Coaching tools have shallow multi-coach support and flat templates; gym-ops tools have
multi-location but no structured programming; AI features across the board are thin (draft
generators, not bounded adaptive coaching). **This convergence is the most defensible
differentiation** — but it is a *search/marketing-copy* conclusion. **Validate with live
competitor trial accounts before betting the roadmap on it.**

Secondary opportunities: **transparent all-in pricing** (against add-on fragmentation and
Glofox-style mid-contract hikes), and **low/no payments take-rate** as a wedge against
Mindbody's model (or deliberately adopt a moderate take-rate as expansion revenue — a
conscious choice, see [cost-analysis.md](cost-analysis.md)).

## Customer economics benchmarks (for the model)

- **Model B2B (gym/coach) churn separately from B2C (member) churn** — different actors.
  - Our customer = the gym/coach business → use **SMB SaaS churn ~5–7%** (monthly figure
    ambiguous in sources; treat as "high for SMB").
  - Member (end-user) churn is much higher (**~33.6%/yr**, HFA 2025) but that's the gym's
    problem, not our direct revenue — though it drives *their* willingness to pay for retention
    features (dunning, engagement).
- **CAC:** SMB SaaS median **$200–700**; 2026 median cost to acquire $1 of new ARR ≈ **$2.00**
  (rising).
- **LTV:CAC:** target **≥3:1**; strong SMB tools hit ~4:1 with ~6-month payback.
- **~30–40% of member cancellations are involuntary** (failed payment) — makes dunning/retry a
  real retention feature we can sell.

## Willingness to pay

- **Independent trainers:** typically **$50–99/mo** on pro software (budget options $10–19).
- **Gym/studio owners:** **$150–400/mo** subscription before processing; software as **3–8% of
  revenue** is the rough guardrail.
- At scale, **processing fees can rival or exceed** the software subscription — which is why
  incumbents monetize payments.

## Implications for pricing (feeds [cost-analysis.md](cost-analysis.md))

1. **Coaching layer:** ~**$2–4 / active client / mo** at scale, **$20–30/mo** solo entry.
2. **Gym-tenant layer:** **$100–250/mo** small/single-location → **$300–700/mo** multi-location.
3. **Payments:** decide deliberately — compete on low/no take-rate (Gymdesk/CoachRx) *or*
   monetize processing (Mindbody/PushPress). Given our Stripe Connect design
   ([ADR-0010](adr/0010-payments-and-billing.md)), a **modest application fee** is natural
   expansion revenue without Mindbody-level aggression.
4. **Avoid add-on fragmentation** as the headline model (differentiation), but keep a couple of
   genuine premium add-ons (branded app, AI tier) as expansion levers.
5. **AI as a tier, not a per-token meter** — our AI cost is near-zero (see cost-analysis), so
   bundling it into higher tiers is cleaner than metering it.
