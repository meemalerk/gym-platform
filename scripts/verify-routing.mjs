/**
 * Verify the landing redirect can never point at an unmounted route group.
 *
 * The bug this exists to stop, in the router's own words:
 *
 *     The action 'REPLACE' with payload {"name":"(app)",…} was not handled by
 *     any navigator. Do you have a route named '(app)'?
 *
 * `app/index.tsx` had a three-way split (signed out / no gym / the app) and the
 * root layout had grown a four-way guard — the fourth state being a member who
 * is in the gym and still owes it a membership decision. The layout unmounted
 * `(app)` for those people; index went on redirecting them into it. Both files
 * now read `@/session/routing`, and the property below is what makes drifting
 * apart again a failing check rather than a console error somebody hits on a
 * phone.
 *
 *   node scripts/verify-routing.mjs
 *
 * Needs Node 23.6+ (type stripping), like every other .mjs suite here.
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { groupOf, landingRoute, mountedGroup, needsPlanChoice } = await import(
  '../apps/mobile/src/session/routing.ts'
);

let passed = 0;
let failed = 0;

const check = (label, condition) => {
  if (condition) {
    passed += 1;
  } else {
    failed += 1;
    console.log(`  FAIL  ${label}`);
  }
};

const eq = (label, actual, expected) => {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (!ok) {
    console.log(`  FAIL  ${label}\n        got ${JSON.stringify(actual)}, want ${JSON.stringify(expected)}`);
  }
  ok ? (passed += 1) : (failed += 1);
};

const BOOLS = [false, true];
const ALL = [];
for (const signedIn of BOOLS)
  for (const hasGym of BOOLS)
    for (const needsPlan of BOOLS)
      for (const settling of BOOLS) ALL.push({ signedIn, hasGym, needsPlan, settling });

const show = (s) =>
  `signedIn=${+s.signedIn} hasGym=${+s.hasGym} needsPlan=${+s.needsPlan} settling=${+s.settling}`;

// ------------------------------------------------------- the whole property

// THE assertion. Everything else in this file is detail; this is the bug.
for (const s of ALL) {
  const href = landingRoute(s);
  if (href === null) continue; // "not yet" cannot be unreachable
  check(
    `reachable: ${show(s)} -> ${href} (layout mounts ${mountedGroup(s)})`,
    groupOf(href) === mountedGroup(s),
  );
}

// And the states with no landing route must be exactly the settling ones that
// got far enough to care — otherwise `null` is quietly swallowing a real case
// and index renders nothing forever.
for (const s of ALL) {
  const href = landingRoute(s);
  if (href === null) {
    check(`null only while settling: ${show(s)}`, s.settling && s.signedIn && s.hasGym);
  }
}

// -------------------------------------------------------------- the mapping

eq(
  'signed out goes to sign-in',
  landingRoute({ signedIn: false, hasGym: false, needsPlan: false, settling: false }),
  '/sign-in',
);
eq(
  'signed in with no gym joins one',
  landingRoute({ signedIn: true, hasGym: false, needsPlan: false, settling: false }),
  '/(onboarding)/start',
);
eq(
  'a gym but no plan picks one — the case that was broken',
  landingRoute({ signedIn: true, hasGym: true, needsPlan: true, settling: false }),
  '/(onboarding)/choose-plan',
);
eq(
  'a gym and a plan reaches the app',
  landingRoute({ signedIn: true, hasGym: true, needsPlan: false, settling: false }),
  '/(app)/(tabs)',
);
eq(
  'settling sends nowhere rather than guessing',
  landingRoute({ signedIn: true, hasGym: true, needsPlan: false, settling: true }),
  null,
);

// Signing out mid-flight must still leave, not stall: `settling` is only
// consulted after the two states that do not depend on the server's answer.
eq(
  'a settling query does not trap a signed-out account',
  landingRoute({ signedIn: false, hasGym: true, needsPlan: true, settling: true }),
  '/sign-in',
);
eq(
  'nor an account with no gym',
  landingRoute({ signedIn: true, hasGym: false, needsPlan: true, settling: true }),
  '/(onboarding)/start',
);

// ------------------------------------------------------- the mounted group

eq(
  'signed out mounts the auth screens',
  mountedGroup({ signedIn: false, hasGym: true, needsPlan: false, settling: false }),
  'auth',
);
eq(
  'no gym mounts onboarding',
  mountedGroup({ signedIn: true, hasGym: false, needsPlan: false, settling: false }),
  '(onboarding)',
);
eq(
  'owing a plan ALSO mounts onboarding',
  mountedGroup({ signedIn: true, hasGym: true, needsPlan: true, settling: false }),
  '(onboarding)',
);
eq(
  'settled and entitled mounts the app',
  mountedGroup({ signedIn: true, hasGym: true, needsPlan: false, settling: false }),
  '(app)',
);

// Exactly one group, always — the property above is only meaningful because
// the guards are mutually exclusive and exhaustive.
check(
  'every state mounts exactly one known group',
  ALL.every((s) => ['auth', '(onboarding)', '(app)'].includes(mountedGroup(s))),
);

// ------------------------------------- who is asked to choose a membership

/*
  The reported bug: "every time an already existing member tries to sign in they
  have to choose a membership".

  The gate used to be derived from entitlements — no `gym_access` meant "choose
  a plan" — which is equally true of a brand new account and of a member of
  three years whose subscription lapsed, was cancelled, or was never bought
  through the app at all. Those people met the plan picker on every sign-in with
  no way past it. It is now a marker set when an account JOINS and cleared when
  it chooses, so signing in never triggers it.
*/
const ALICE = 'user-alice';
const BOB = 'user-bob';

check(
  'an account mid-registration is asked',
  needsPlanChoice({ planPendingFor: ALICE, userId: ALICE }) === true,
);
check(
  'an existing member signing in is NOT asked',
  needsPlanChoice({ planPendingFor: null, userId: ALICE }) === false,
);
check(
  'a member whose subscription lapsed is still NOT asked',
  // No marker, because they registered long ago. Their billing state is the
  // Membership screen's business, not the router's.
  needsPlanChoice({ planPendingFor: null, userId: BOB }) === false,
);
check(
  "one account's half-finished sign-up does not follow another onto the device",
  needsPlanChoice({ planPendingFor: ALICE, userId: BOB }) === false,
);
check(
  'a marker with nobody signed in asks nobody',
  needsPlanChoice({ planPendingFor: ALICE, userId: null }) === false,
);
check(
  'no marker and nobody signed in asks nobody',
  needsPlanChoice({ planPendingFor: null, userId: null }) === false,
);

// And the marker must actually reach the router: a pending account lands on the
// plan screen, the same account without the marker lands in the app.
eq(
  'mid-registration lands on the plan picker',
  landingRoute({
    signedIn: true,
    hasGym: true,
    needsPlan: needsPlanChoice({ planPendingFor: ALICE, userId: ALICE }),
    settling: false,
  }),
  '/(onboarding)/choose-plan',
);
eq(
  'the same account, registered, lands in the app',
  landingRoute({
    signedIn: true,
    hasGym: true,
    needsPlan: needsPlanChoice({ planPendingFor: null, userId: ALICE }),
    settling: false,
  }),
  '/(app)/(tabs)',
);

// ------------------------------------------------------------------ groupOf

eq('groupOf reads the app', groupOf('/(app)/(tabs)'), '(app)');
eq('groupOf reads onboarding', groupOf('/(onboarding)/choose-plan'), '(onboarding)');
eq('groupOf reads auth', groupOf('/sign-in'), 'auth');

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
