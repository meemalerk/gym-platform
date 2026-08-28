# Gym Console

The owner and head-coach web app — [ADR-0009](../../docs/adr/0009-client-stack.md)'s
instructor/admin half, finally built.

## Why a second client

Owners and head coaches do the work a phone is worst at: reading a billing ledger,
scanning a roster for who has stopped coming, comparing a proposed exercise against four
hundred existing ones. The five-tab ceiling in the app already forced Billing to displace
Library for managers — that is a symptom of running a back office on a phone.

The mobile app is not deprecated. It stays the member and floor-trainer surface, which is
the right shape for logging a set between rounds.

## What is shared, and what is not

**Shared:** the API contract and the design language. `src/lib/schema.d.ts` is generated
from the same OpenAPI document as the app's, so backend drift breaks this build too;
`src/tokens.css` is **generated** from `apps/mobile/src/ui/theme.ts`, so there is one
palette, verified once by `scripts/verify-contrast.mjs`.

**Not shared:** the API client. The two have genuinely different needs — the phone keeps
its refresh token in the OS keychain and de-duplicates refreshes across a backgrounded
app; a browser tab has neither problem and cannot use `expo-secure-store` anyway. A shared
client would be a pile of `if (platform)`.

## Running it

    # the API, on the port vite proxies to
    cargo run --bin server            # :8080

    npm install
    npm run dev                       # :5174, proxying /api to :8080

Same-origin in development and in the demo (nginx serves the build and proxies `/api`), so
there is no absolute API URL anywhere and no CORS entry to keep in step.

## Keeping it honest

    npm run typecheck                 # part of scripts/all-check.sh
    npm run tokens                    # regenerate tokens.css after a palette change
    npm run codegen:api               # regenerate schema.d.ts against a running API

`all-check.sh` regenerates the tokens and fails if the checked-in file differs — a
hand-edited `tokens.css` is a second, unverified palette, and this is what stops one
appearing.

## Known gap

The refresh token is kept in `localStorage`, so an XSS on this origin is an account
takeover. The better answer is an HttpOnly cookie set by the API
([docs/research-2026.md](../../docs/research-2026.md) §5), which needs a server-side
session endpoint and CSRF handling that do not exist yet. Written down in
`src/lib/api.ts` rather than quietly shipped.
