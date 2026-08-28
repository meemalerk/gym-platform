# Subscriptions, Memberships & Billing

> This was a gap in the initial plan. This doc defines the billing domain. Provider choices
> (Stripe products, RevenueCat, app-store rules) are being finalized from July-2026 research
> and will be locked in an ADR — see the "Provider decisions (pending research)" section and
> [research-2026.md](research-2026.md).
>
> **⚠ Flagged 2026-08-23** ([ADR-0023](adr/0023-single-gym-deployment.md)): relationship 1
> below (Platform → Gym SaaS billing) presumes many gym-org customers on a shared platform.
> Each deployment now serves a single gym. Relationship 2 (Gym → Member) is unaffected — it
> was always scoped to the one gym a member belongs to. Not reconciled here; see the matching
> flag in [market-analysis.md](market-analysis.md).

## Three distinct billing relationships

This is a B2B2C platform. Do not conflate these — they have different payers, providers,
and rules.

```
1. Platform  →  charges  →  Gym organisations      (our SaaS revenue)
      per-seat / per-branch / tiered plans, metered add-ons

2. Gym       →  charges  →  its Members             (gym's revenue; we facilitate)
      recurring memberships, coaching packages, class packs
      → platform takes an application fee / uses a marketplace model

3. Member app (mobile)  →  digital subscriptions    (subject to app-store rules)
      if/where a purchase is "digital", store IAP rules may apply
```

Each relationship maps to different infrastructure (see pending ADR). The critical
architectural fork is **relationship 2 & 3**: whether a gym membership counts as a *physical
service* (no app-store IAP required, charge directly) or a *digital good* (store IAP may be
required). The payments research resolves the current 2026 rules; this materially affects
the mobile app architecture.

## Core concept: entitlements, not raw subscriptions

The backend's source of truth for "what can this actor access right now" is an
**entitlement**, decoupled from where the payment came from (Stripe, Apple, Google, manual).

```
Payment event (Stripe / App Store / Play / manual)
        ↓  reconcile
Entitlement  ( subject, feature/tier, status, valid_from, valid_until, source )
        ↓
Authorization + feature gating read entitlements, never payment provider state directly
```

This keeps the domain provider-agnostic and lets a gym membership be granted by a card
charge, an in-app purchase, or an admin comp — uniformly.

## Domain model

### Platform ↔ Gym (SaaS billing)
```
plans                  -- platform SaaS tiers (e.g. Starter/Pro/Enterprise)
plan_prices            -- currency, interval, per-seat/per-branch/flat, metered components
gym_subscriptions      -- a gym's subscription to a plan (status, current_period, quantities)
gym_invoices           -- issued invoices
usage_records          -- metered usage (e.g. active members, AI runs) for billing
gym_entitlements       -- resolved feature access for the gym (seats, branches, AI level caps)
```

### Gym ↔ Member (membership billing)
```
membership_products    -- gym-defined offerings: recurring membership, coaching package,
                          class pack, drop-in, family plan, trial
membership_prices      -- price points, intervals, class-pack sizes, trial terms
member_subscriptions   -- a member's active membership (status, period, freezes)
member_subscription_events -- pause/freeze/resume/cancel/upgrade history (immutable)
class_pack_balances    -- remaining credits for pack-based products
member_invoices        -- charges to members
member_entitlements    -- resolved access (gym access, assigned coaching, class credits)
payment_methods        -- tokenized references only; NEVER raw card data
```

### Cross-cutting
```
billing_accounts       -- links a subject (gym or member) to provider customer IDs
provider_events        -- raw inbound webhooks (Stripe/Apple/Google), idempotency keyed
reconciliation_log     -- how a provider event mapped to an entitlement change
refunds / credits / adjustments
tax_records            -- tax calculation/reporting artifacts
```

## Membership lifecycle (gym members)

```
Trialing → Active → PastDue → (Recovered → Active | Canceled)
Active → Paused/Frozen → Active            (gym memberships commonly freeze)
Active → Canceled (end-of-period | immediate)
Active → Upgraded/Downgraded (proration)
```

Gym-specific behaviours the model must support (from domain requirements — confirmed against
research):
- **Freezes/pauses** (holiday, injury) — pause billing and access without losing history.
- **Class packs** — decrement credits on booking/attendance; expiry rules.
- **Family / group plans** — one payer, multiple member entitlements.
- **Trials** — time- or usage-limited, converting to paid.
- **Failed-payment recovery (dunning)** — retry schedule, grace period, then suspend.
- **Cancellation flows** — end-of-period vs immediate; retention/pause offers.

## Reconciliation & webhooks

- All provider webhooks land in `provider_events` **idempotently** (keyed on the provider's
  event ID) before any domain change — never mutate entitlements directly from a webhook
  handler without recording the raw event first.
- A processor maps events → entitlement changes via the transactional outbox pattern (see
  [architecture.md](architecture.md)), logging each mapping in `reconciliation_log`.
- Entitlement is the read model for authorization/feature-gating; billing provider state is
  never queried on the hot path.

## Hard rules

1. **The platform never touches raw card data.** Tokenized references only; PCI scope stays
   with the provider. (Consistent with the prohibited-actions policy: we don't enter
   financial credentials.)
2. **Idempotent everywhere** — webhooks, sync, retries. Money bugs are the worst bugs.
3. **Entitlements decouple access from payment source** so store-IAP, direct charges, and
   comps are uniform.
4. **Every billing state change is an immutable event** (dunning, freeze, upgrade) for
   audit and dispute resolution — reuse the audit/outbox machinery.
5. **Tax and compliance handled by the provider layer** (e.g. Stripe Tax / Anrok) where
   possible; minimize what we compute ourselves.

## Feature gating from entitlements

Gym SaaS tier gates platform features (e.g. max branches, seats, **AI authority level caps**
— ties into [ai-authority-model.md](ai-authority-model.md)); member entitlements gate member
app access and assigned coaching. Both read from resolved `*_entitlements`, checked in the
application layer alongside authorization (distinct from it — access-right vs access-permission).

## Provider decisions (resolved by July-2026 research → [ADR-0010](adr/0010-payments-and-billing.md))

The July-2026 payments research ([research-2026.md](research-2026.md) §6) resolved the key
open question. **The load-bearing finding:**

> **Gym memberships and 1:1 coaching are exempt from app-store IAP on BOTH platforms** —
> Apple Guideline 3.1.3(d) explicitly names *"fitness training"*; Google Play's payments
> policy lists *"gym memberships"* by name. This holds **independently of the ongoing Epic
> litigation.** So we bill the core product via **Stripe web checkout and avoid the 15–30%
> store cut.**

Locked decisions (see [ADR-0010](adr/0010-payments-and-billing.md)):
- **Platform→Gym SaaS:** **Stripe Billing** (per-seat/per-branch/tiered; **Meters API** for
  metered usage; Entitlements/Features API for plan-gating — *verify GA vs preview*).
- **Gym→Member:** **Stripe Connect (Accounts v2)** — a gym is one account that is both a
  Customer (pays our SaaS fee) and a Merchant (collects from members), single KYC;
  `application_fee_amount` is our platform cut. Embedded onboarding + networked KYC for
  multi-location chains.
- **Mobile:** **Stripe web checkout for membership/coaching** (IAP-exempt). **IAP via
  RevenueCat ONLY for any purely-digital, app-only SKU** (e.g. on-demand video library) —
  which *does* still require IAP (Apple 3.1.1). **Segment SKUs accordingly.**
- **Tax:** **Stripe Tax** for the SaaS side; **pair with a dedicated engine (Anrok) for the
  Connect/marketplace side** — Stripe Tax does *not* fully handle marketplace-facilitator
  sales-tax rules. This is a real gap, not a default.
- **IAP↔entitlement reconciliation** (only if digital SKUs ship): Apple App Store Server
  Notifications V2 + Google Play RTDN, normalized into our one entitlement object. Play
  Billing Library v8+ mandatory by 2026-08-31.

The **entitlement read model above was the right call** — it lets Stripe web, IAP, and manual
comps all resolve to uniform access. **Build cancellation "as easy as signup"** regardless of
the vacated FTC click-to-cancel rule (states + FTC Section 5 still enforce the substance).

## Roadmap placement

Billing is **Phase 6** in [roadmap.md](roadmap.md) (scale & extensions) — but the
`gym_subscriptions` / `gym_entitlements` and `member_entitlements` read models should be
stubbed earlier so feature-gating and AI-level caps have something to read. Do not build the
full billing engine before there are paying gyms.
