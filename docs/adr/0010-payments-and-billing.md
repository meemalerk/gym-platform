# ADR-0010: Payments & billing architecture

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author
- **Informed by:** [research-2026.md](../research-2026.md) §6 (July-2026 payments/app-store research)

## Context

The platform has three distinct billing relationships (see
[subscriptions-billing.md](../subscriptions-billing.md)): platform→gym SaaS, gym→member
memberships, and mobile digital purchases. The critical open question was whether selling
gym memberships / coaching through the mobile app triggers app-store IAP (a 15–30% cut and a
very different architecture) or can be charged directly.

## Decision

**Bill the core product (gym memberships + 1:1 coaching) via Stripe web checkout, exempt from
app-store IAP.** Research confirmed both stores' own policies exempt this category:

- **Apple Guideline 3.1.3(d) "Person-to-Person Services" explicitly names "fitness training"**;
  3.1.3(e) covers services consumed outside the app.
- **Google Play's payments policy lists "gym memberships" by name** as an exempt physical
  service, and exempts 1:1 health coaching that isn't recorded/replayable.
- This holds **independently of the ongoing Epic v. Apple litigation** (whose US external-link
  fee remains unsettled — SCOTUS cert granted, argued ~Oct 2026 term).

Concretely:
- **Platform→Gym SaaS:** **Stripe Billing** (per-seat/per-branch/tiered; Meters API for metered
  usage; Entitlements API for plan-gating).
- **Gym→Member:** **Stripe Connect, Accounts v2** — a gym is one account that is both a Customer
  (pays our SaaS fee) and a Merchant (collects from members); `application_fee_amount` is our
  platform cut; embedded/networked KYC onboarding.
- **Mobile:** Stripe web checkout for membership/coaching; **RevenueCat + IAP ONLY for any
  purely-digital, app-only SKU** (e.g. an on-demand video library), which *does* still require
  IAP under Apple 3.1.1. **SKUs must be segmented** physical-vs-digital.
- **Entitlements are the source of truth** for access — decoupled from payment source (Stripe /
  IAP / manual comp), synced via webhook into a local read model, never queried live per request.
- **Tax:** Stripe Tax for the SaaS side; **pair with a dedicated engine (e.g. Anrok) for the
  Connect/marketplace side** (Stripe Tax does not fully handle marketplace-facilitator rules).
- **PCI:** only Stripe Elements/Checkout — **the platform never touches card data** (SAQ A;
  note new PCI v4.0.1 Req 6.4.3 / 11.6.1 on payment-page scripts).

## Alternatives considered

- **All-IAP mobile billing** — unnecessary given the exemption; would forfeit 15–30% and add
  store-reconciliation complexity for no benefit on the core product.
- **RevenueCat as the primary billing system** — its own data shows native IAP out-converts web,
  but that's moot when the core product is IAP-exempt; keep RevenueCat scoped to digital SKUs.
- **Self-built billing engine** — reinventing dunning/proration/tax is not worth it early;
  Stripe covers it.

## Consequences

- **Positive:** avoids the store cut on core revenue; one entitlement model unifies all payment
  rails; Stripe handles dunning (Smart Retries), proration, SCA/3DS, and most tax.
- **Negative / costs:** must carefully segment physical vs digital SKUs if any digital-only
  content ships; marketplace-facilitator tax needs a second engine; app-store policy and US
  external-link fees are actively litigated — **re-verify before launch.**
- **Obligations:** build cancellation "as easy as signup" (FTC Section 5 / state laws) regardless
  of the vacated click-to-cancel rule; if digital SKUs ship, implement App Store Server
  Notifications V2 + Play RTDN reconciliation and adopt Play Billing Library v8+ (mandatory
  2026-08-31).

## References

- [subscriptions-billing.md](../subscriptions-billing.md), [research-2026.md](../research-2026.md) §6
- Compliance context in [research-2026.md](../research-2026.md) §5 (PCI, SCA, health-data rules).
