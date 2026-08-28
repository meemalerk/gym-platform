# ADR-0009: Client stack — RN+Expo mobile, React+TanStack web

- **Status:** Accepted
- **Date:** 2026-07-14
- **Deciders:** Project author

## Context

There are (at least) three product surfaces with different needs: an offline-capable member
mobile app, a dense desktop-first instructor workspace, and a role-sensitive gym admin
surface. The backend is Rust ([ADR-0002](0002-backend-language-rust.md)), so there is no
shared-language type reuse for free — but types can be shared via a generated SDK.

## Decision

- **Member mobile app:** React Native + Expo (development builds; EAS for production/store),
  Expo Router, TypeScript, TanStack Query, Zustand, React Hook Form + Zod, Reanimated,
  FlashList, Expo SQLite (offline + operation log), NativeWind + a custom design system.
- **Instructor & admin web:** React + Vite, TanStack Router / Query / Table, dnd-kit, TipTap,
  Zod, ECharts (or Recharts). **Desktop-first.** The gym admin surface shares this shell with
  role-sensitive navigation.
- **Type sharing:** generate a **TypeScript SDK from the backend's OpenAPI spec** for both
  clients. This replaces the "shared language" advantage a TS backend would have offered.
- **Not Next.js** for the web app — no meaningful public SEO surface; the authenticated
  workspace gains little from React Server Components.

## Alternatives considered

- **Next.js web app** — worthwhile only if we later need a substantial public/SEO site;
  overhead now for an authenticated workspace.
- **Mobile-first instructor UI** — rejected; serious programme authoring needs a dense,
  keyboard-friendly desktop workflow (spreadsheet + calendar + document hybrid), not a stack
  of mobile modal forms.
- **Flutter / native** — loses the React/TypeScript + generated-SDK cohesion across surfaces.

## Consequences

- **Positive:** cohesive React/TypeScript client ecosystem; strong offline story on mobile;
  desktop-grade authoring on web; type safety across the boundary via generated SDK.
- **Negative / costs:** two distinct client apps to maintain; a design system to build;
  OpenAPI-to-SDK generation must be kept in the build pipeline.
- **Follow-ups:** wire OpenAPI → TS SDK generation early ([roadmap.md](../roadmap.md)
  Phase 0/1).

## References

- [tech-stack.md](../tech-stack.md), [architecture.md](../architecture.md)
