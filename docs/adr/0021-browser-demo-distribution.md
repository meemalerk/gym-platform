# ADR-0021: Distribute demos as a same-origin browser build behind a tunnel

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Areeb

## Context

The product is a phone app ([ADR-0009](0009-client-stack.md)). But people who need to *look* at
it — a prospective gym, an advisor, a collaborator — are not going to install a toolchain, and
several of them are not going to install anything at all.

Three constraints turned out to be binding:

1. **Expo Go is no longer a route onto an iPhone.** Expo's May-2026 changelog confirms the App
   Store build is pinned to **SDK 54** (55 still awaiting review) while this app is on SDK 57, and
   Expo's own review documentation now describes Expo Go as "a playground for students and
   learners… not useful for the review process of your app". `eas go` and development builds both
   require a paid Apple Developer account. Recipient-side Expo accounts do not help.
2. **Docker Desktop on Windows is a real hurdle** for a non-technical viewer: a ~600 MB download,
   WSL2, virtualization enabled in BIOS, and a reboot.
3. **`EXPO_PUBLIC_*` values are inlined at build time.** A web bundle built with
   `http://localhost:8211` works for whoever built it and for nobody else — every other viewer's
   browser dials *their own* machine.

## Decision

We will ship demos as a **static web export of the same React Native codebase, served
same-origin with the API, exposed on demand through an ephemeral tunnel**.

Concretely:

- `expo export --platform web` (react-native-web) produces a static bundle; nginx serves it.
- **nginx also proxies `/api` to the backend**, so the page and the API share an origin.
- The web build sets `EXPO_PUBLIC_API_URL=""` — an empty string meaning "this origin". The
  bundle therefore contains **no absolute API URL** and travels unchanged between localhost, a
  LAN address and a tunnel.
- `demo/share.sh` opens a **Cloudflare quick tunnel** (no account, no card) and prints a public
  `https://` link plus a QR code.
- The one-tap demo sign-in buttons, normally `__DEV__`-gated, are re-enabled for this build only
  via the explicit build-time flag `EXPO_PUBLIC_DEMO_ACCOUNTS=true`.

The browser build remains a **viewing surface, not a supported product surface**. ADR-0009's
decision — that the real browser client is a separate React + Vite app — is unchanged.

## Alternatives considered

- **Expo Go + EAS Update.** Ruled out by fact, not preference: the SDK gap makes it impossible on
  iOS today, and Expo has repositioned the product away from this use.
- **TestFlight.** Works, and is the correct answer for genuine beta testing. Costs $99/yr and
  puts an Apple review between you and showing someone the app. Revisit when there are real
  testers rather than viewers.
- **Ship the Docker zip only.** Already built and kept, but it does nothing for a phone and asks
  a lot of a Windows user.
- **Deploy to a real host.** The right answer for a *permanent* URL, and the hosting shape is
  already decided in [hosting-deployment.md](../hosting-deployment.md). Deferred because it costs
  money and an account for something currently needed only in bursts.
- **Two tunnels, one for the app and one for the API.** Rejected: the API's tunnel URL is random
  and only known at runtime, but the bundle needs it at build time. Same-origin dissolves the
  problem instead of sequencing around it.

## Consequences

- **Positive:** anyone with a browser can see the whole product — Windows, macOS, iPhone, Android
  — with nothing installed. On iOS, *Add to Home Screen* gives a full-screen app-like launch.
- **Positive:** same-origin removes CORS from the demo path entirely, and makes the artefact
  portable rather than machine-specific.
- **Positive:** the tunnel is ephemeral and unauthenticated-but-unguessable, which suits a demo
  and is honest about being unsuitable for anything else.
- **Negative:** the link lives only while the host machine is running, and its address changes on
  each restart.
- **Negative:** the demo build ships known credentials by design. This is acceptable *only*
  because the flag is build-time and opt-in; a release build sets nothing and dead-strips the
  branch.
- **Negative:** a few native affordances (haptics, the secure key store) silently no-op on web, so
  the browser build slightly flatters the app's tactility.
- **Follow-up:** if viewing becomes routine rather than occasional, deploy properly per
  [hosting-deployment.md](../hosting-deployment.md) and retire the tunnel for that audience.

## References

- [ADR-0009: client stack](0009-client-stack.md)
- [hosting-deployment.md](../hosting-deployment.md)
- Expo, *Expo Go and the App Store* (May 2026 changelog)
- Expo, *Distributing apps for review*
