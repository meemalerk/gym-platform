/**
 * Where a signed-in-or-not account belongs, as one pure decision.
 *
 * **Why this is a module rather than two `if` ladders.** The root layout
 * decides which route group to MOUNT; `app/index.tsx` decides where to SEND a
 * cold start. Those are the same question asked twice, and when the answers
 * drifted the router said so:
 *
 *     The action 'REPLACE' with payload {"name":"(app)",…} was not handled by
 *     any navigator. Do you have a route named '(app)'?
 *
 * A `Redirect` into a group the layout has left unmounted is not a no-op. The
 * layout had grown a fourth state — signed in, in a gym, still owing a
 * membership decision — and index still had the three-way split from before
 * it, so it sent those people into `(app)` while the layout was busy not
 * mounting `(app)` for exactly them.
 *
 * Both files now read the functions below, and `verify-routing.mjs` asserts
 * over every combination that the landing route is always inside a mounted
 * group. Drifting again would have to be done deliberately, in one file, past
 * a failing check.
 */

/** The three top-level areas. `auth` is the un-grouped sign-in screens. */
export type RouteGroup = 'auth' | '(onboarding)' | '(app)';

/**
 * The landing screens, spelled out.
 *
 * A union of literals rather than `string`, so expo-router's typed routes
 * still check them: a route renamed or mistyped here fails `tsc` instead of
 * failing at runtime as another unhandled REPLACE, which is the exact class of
 * bug this module exists to close.
 */
export type LandingRoute =
  | '/sign-in'
  | '/(onboarding)/start'
  | '/(onboarding)/choose-plan'
  | '/(app)/(tabs)';

/**
 * Everything the decision turns on.
 *
 * `needsPlan` means **this account is mid-registration and has not chosen a
 * membership yet** — a marker set when it joined the gym and cleared when it
 * picks (see `@/session/onboarding`). It deliberately does NOT mean "holds no
 * `gym_access` entitlement". It used to, and that put the plan picker in front
 * of every existing member whose subscription had lapsed, been cancelled, or
 * never been bought through the app, on every single sign-in, with no way past
 * it. Signing in is not registering.
 *
 * `settling` is the session still being restored from storage. It is NOT the
 * same as `needsPlan: false`, and the difference is the whole reason it is a
 * separate field: unknown means "do not move yet", not "let them in".
 */
export type SessionShape = {
  signedIn: boolean;
  hasGym: boolean;
  needsPlan: boolean;
  settling: boolean;
};

/**
 * Which group the root layout mounts.
 *
 * Exactly one, always — the three conditions are mutually exclusive and
 * exhaustive, which is what makes "the landing route is reachable" a property
 * worth asserting rather than a coincidence.
 */
export function mountedGroup(s: SessionShape): RouteGroup {
  if (!s.signedIn) return 'auth';
  if (!s.hasGym || s.needsPlan) return '(onboarding)';
  return '(app)';
}

/**
 * Where `app/index.tsx` sends a cold start — or `null` for "not yet".
 *
 * `null` while settling is deliberate. In practice the layout holds a loader
 * and index is not even mounted, but "in practice" is what the old three-way
 * split relied on too. Rendering nothing for a frame costs nothing; a redirect
 * into an unmounted group costs a console error and a dead end.
 */
export function landingRoute(s: SessionShape): LandingRoute | null {
  if (!s.signedIn) return '/sign-in';
  // Bootstrapping the one gym is an ops action (ADR-0023), so an account in no
  // gym is here to JOIN one through the open door (ADR-0026/0031).
  if (!s.hasGym) return '/(onboarding)/start';
  if (s.settling) return null;
  // Mid-registration only. The plans ARE the "solo or coached" question — see
  // choose-plan.tsx — but an established member is never asked again.
  if (s.needsPlan) return '/(onboarding)/choose-plan';
  return '/(app)/(tabs)';
}

/**
 * Is this account mid-registration, still owing a membership choice?
 *
 * The marker is keyed by user id (see `@/session/onboarding`), so this compares
 * rather than coercing to a boolean: a device two people have signed into must
 * not hand one of them the other's half-finished sign-up.
 *
 * **What this is not.** It is not "holds no `gym_access`". That was the old
 * rule, and it meant an existing member whose subscription had lapsed, been
 * cancelled, or never been bought through the app was shown the plan picker on
 * every sign-in with no way past it. Signing in is not registering; a member
 * with no active plan belongs on Today, with the Membership screen telling them
 * what they are missing.
 */
export function needsPlanChoice(args: {
  planPendingFor: string | null;
  userId: string | null;
}): boolean {
  return args.planPendingFor !== null && args.planPendingFor === args.userId;
}

/** Which group an href lands in. Used to check the two agree. */
export function groupOf(href: LandingRoute): RouteGroup {
  if (href.startsWith('/(app)')) return '(app)';
  if (href.startsWith('/(onboarding)')) return '(onboarding)';
  return 'auth';
}
