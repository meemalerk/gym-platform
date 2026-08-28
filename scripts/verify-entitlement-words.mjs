/**
 * The words a refusal uses.
 *
 * A gate that cannot explain itself is a support ticket, so the phrasing is
 * treated as behaviour and checked here: which plan to name, what to say when
 * no plan on sale would help, and the ordering that keeps two identical plans
 * reading identically.
 *
 *   node scripts/verify-entitlement-words.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { featureLabel, summariseGrants, holds, reasonFor, missingReason } = await import(
  '../apps/mobile/src/features/billing/entitlements.ts'
);

let passed = 0;
let failed = 0;

function check(label, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    passed += 1;
    console.log(`  ok    ${label}`);
  } else {
    failed += 1;
    console.log(`  FAIL  ${label}\n          got  ${a}\n          want ${e}`);
  }
}

const plan = (name, grants, is_offered = true) => ({ name, grants, is_offered });

console.log('\n=== feature labels ===');

check('gym access', featureLabel('gym_access'), 'Gym access');
check('coached programming', featureLabel('coached_programming'), 'Coached programming');
check('class credits', featureLabel('class_credits'), 'Class credits');

console.log('\n=== what a plan confers, as one phrase ===');

check('one grant is just its label', summariseGrants(['gym_access']), 'Gym access');
check(
  'two read as a ladder',
  summariseGrants(['gym_access', 'coached_programming']),
  'Gym access + coached programming',
);
check(
  'three keep the ladder order, not the array order',
  summariseGrants(['class_credits', 'coached_programming', 'gym_access']),
  'Gym access, coached programming + class credits',
);
check(
  'so two plans granting the same things read identically',
  summariseGrants(['coached_programming', 'gym_access']),
  summariseGrants(['gym_access', 'coached_programming']),
);
check(
  'a coaching-only add-on does not pretend to include the floor',
  summariseGrants(['coached_programming']),
  'Coached programming',
);
check('an empty plan says so plainly', summariseGrants([]), 'Grants nothing');
check(
  'an unknown feature is dropped, not printed raw',
  summariseGrants(['gym_access', 'teleportation']),
  'Gym access',
);

console.log('\n=== reading a held set ===');

const held = [
  { feature: 'gym_access', source: { kind: 'not_billed' }, because: 'This gym does not bill' },
  {
    feature: 'coached_programming',
    source: { kind: 'subscription', plan_name: 'Coaching' },
    because: 'Your Coaching membership',
  },
];

check('holds what it holds', holds(held, 'gym_access'), true);
check('and not what it does not', holds(held, 'class_credits'), false);
check('the reason comes from the server, verbatim', reasonFor(held, 'coached_programming'), 'Your Coaching membership');
check('a feature not held has no reason', reasonFor(held, 'class_credits'), null);
check('an empty set holds nothing', holds([], 'gym_access'), false);

console.log('\n=== what to say when it is missing ===');

check(
  'name the one plan that would help',
  missingReason('gym_access', [plan('Open Gym', ['gym_access'])]),
  'Open Gym includes gym access.',
);
check(
  'name all of them when several would',
  missingReason('gym_access', [
    plan('Open Gym', ['gym_access']),
    plan('Coaching', ['gym_access', 'coached_programming']),
  ]),
  'Open Gym and Coaching include gym access.',
);
check(
  'three read as a list',
  missingReason('gym_access', [
    plan('Open Gym', ['gym_access']),
    plan('Coaching', ['gym_access']),
    plan('Drop-in', ['gym_access']),
  ]),
  'Open Gym, Coaching and Drop-in include gym access.',
);
check(
  'an archived plan is never suggested — it cannot be bought',
  missingReason('gym_access', [plan('Old Deal', ['gym_access'], false)]),
  'Your membership here does not include gym access. Ask the gym.',
);
check(
  'nor is a plan that does not confer the thing',
  missingReason('coached_programming', [plan('Open Gym', ['gym_access'])]),
  'Your membership here does not include coached programming. Ask the gym.',
);
check(
  'a gym with no plans at all gets the plain line',
  missingReason('class_credits', []),
  'Your membership here does not include class credits. Ask the gym.',
);
check(
  'and the plain line never invents a purchase',
  missingReason('class_credits', [plan('Coaching', ['gym_access', 'coached_programming'])]).includes(
    'Coaching',
  ),
  false,
);

console.log(`\n  PASSED: ${passed}   FAILED: ${failed}`);
process.exit(failed > 0 ? 1 : 0);
