# ADR-0028: A card gateway the deployment hosts itself

- **Status:** Accepted
- **Date:** 2026-08-24
- **Deciders:** Project author
- **Extends:** [ADR-0010](0010-payments-and-billing.md). Stripe remains the production
  choice and its adapter is untouched; this adds a second implementation behind the same
  seam, and makes the seam itself slightly better.

## Context

Paying an invoice was the one significant flow in the product with **no executable proof**,
in a codebase whose entire discipline is executable proof ([ADR-0019](0019-verification-first-development.md)).

The reason was structural rather than negligent: a Stripe secret key is a bearer credential
for a real commercial account. It cannot ship in a clone-and-run demo, it cannot sit in a
verification suite, and it cannot be committed. So the whole redirect → pay → return →
settle path — the one place in the system where money changes hands — was exercised only by
reading it.

The same gap made the product undemonstrable. `START-HERE.md` promises the whole thing runs
for someone with only Docker installed; "except the payment, which you'll have to imagine"
is a poor asterisk on a billing feature.

## Decision

**A second `PaymentGateway` implementation that serves its own hosted card page.**

`PAYMENT_GATEWAY=dummy` (the default in a debug build) mints a signed checkout session and
returns a URL to `/pay/{token}` on this same API. That page renders a card form, and
submitting it credits the invoice through **exactly the same service call the Stripe webhook
uses**.

### It is not a test double

This is the distinction the whole decision rests on. `DummyGateway` is a real
implementation of the port: it mints a session, hosts a checkout page, takes a card,
produces a provider reference, and confirms a payment. Everything downstream of the fake
bank — the `Payment` row, the settle-in-the-same-transaction rule, the part-payment
arithmetic, the idempotency key — **is the production path**, shared byte-for-byte with
Stripe. Only the bank is replaced, by a rule about which card numbers succeed.

That is why the resulting suite is worth having. `verify-payments.sh` proves that a
resubmitted form does not charge twice, that a part payment leaves an invoice due, that a
settled invoice cannot start another checkout, and that a checkout after a part payment
charges the *balance* — none of which are facts about the fake bank.

### A payment row must never be mistaken for a real one

`PaymentProvider::Dummy` is its own value rather than a reuse of `Stripe`, the note reads
*"Paid via the demo card page (no money moved)"*, and the page itself carries a **Demo
payment** badge. A row in the money table that cannot be told apart from a real one is a
liability, and the cost of preventing that is one enum variant.

The release-build default is `none`, not `dummy`: a shipped binary must never quietly
accept fake cards. With no gateway configured, card payment reports itself unavailable —
which is the honest answer and the pre-existing behaviour.

### Trust is the signed token, and only the signed token

`/pay/{token}` has no `TenantScope` and no bearer header, because the caller is a browser
following a redirect. Everything the page needs — gym, invoice, member, amount, return URL
— is **inside** the token, so it cannot be re-pointed at another invoice or another sum by
editing a URL, and the return URL cannot be turned into an open redirect.

The token carries a `purpose` claim, so an access token signed with the same secret cannot
be spent as a payment session. That is the same defence `checkin_pass` uses, for the same
reason, and the suite asserts it by posting a real access token at the pay route.

### Two improvements to the seam itself

Adding the second implementation exposed two things wrong with the first:

- **`create_checkout_session` returned only a URL.** The provider reference — the
  idempotency key — was discarded, so the only record that an attempt existed was whatever
  the processor later told us. It now returns `CheckoutSession { url, provider_ref }`, and
  the Stripe adapter reads the session id it was already being handed and throwing away.
- **`apply_stripe_payment` was named after one processor** while being the single
  convergence point for all of them. Renamed `apply_gateway_payment`, taking a
  `GatewayPaymentCommand` — a struct rather than eight positional arguments, because two
  `Uuid`-shaped ids and an amount next to each other is precisely what gets transposed at a
  call site, and named fields make that a compile error instead of a misapplied payment.

## Consequences

**Good.** The payment path is provable, demonstrable, and provable *repeatedly* — 36
assertions run in CI with no account anywhere. The demo shows a member paying a bill,
including the declined card, which is the more interesting half. Stripe is unchanged and
still selected by configuration.

**Cost.** Two HTML-serving routes in an otherwise JSON-only API, and a hand-written page
with inlined CSS. That is deliberate: pulling a framework in to render one form would mean
a build step, a bundle and a CSP conversation, and the demo's whole promise is that it runs
with nothing installed. The page repeats the design tokens from
`apps/mobile/src/ui/theme.ts` rather than importing them, which is real duplication —
accepted because a checkout page that looked nothing like the app it came from would read
as a phishing attempt, and because the alternative is a build pipeline for 40 lines of CSS.

**A standing risk worth naming.** A deployment that sets `PAYMENT_GATEWAY=dummy` in
production would accept fake cards and mark invoices paid. The mitigations are that the
release default is `none`, the server logs a warning naming the gateway at boot, and every
resulting row says so in the database. There is no way to make a demo gateway safe against
someone who deliberately configures it in production; there is a way to make it obvious,
and that is what has been done.

**Verified by** `scripts/verify-payments.sh` (36 assertions) and the card-evaluation and
token unit tests in `crates/infrastructure/src/dummy_gateway.rs`.
