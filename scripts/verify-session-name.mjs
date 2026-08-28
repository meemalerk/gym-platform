/**
 * Verify how a session gets its name on screen.
 *
 * Two kinds of session, two sources for the name, and the server sends both
 * fields with the unused half null (ADR-0035). Eight surfaces used to spell
 * the fallback themselves and they were already saying three different things
 * ("Workout", "your workout", "Programme"); the point of the helper is that
 * they cannot drift, and the point of this script is that the helper does not
 * quietly start preferring the wrong half.
 *
 *   node scripts/verify-session-name.mjs
 *
 * Needs Node 23.6+, which strips TypeScript types itself — same as every other
 * .mjs suite here (on 22.x they all fail identically with
 * ERR_UNKNOWN_FILE_EXTENSION; `node --experimental-strip-types` is the escape
 * hatch). The loader note in scripts/lib/ts-alias-loader.mjs says the same.
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { sessionName, sessionNameFor, UNNAMED_SESSION } = await import(
  '../apps/mobile/src/features/session/name.ts'
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
  if (!ok) console.log(`  FAIL  ${label}\n        got ${JSON.stringify(actual)}, want ${JSON.stringify(expected)}`);
  ok ? (passed += 1) : (failed += 1);
};

// ------------------------------------------------------- a prescribed session

const planned = {
  workout_name: 'Upper A',
  title: null,
  workout_template_id: '018f-aaaa',
};

eq('a prescribed session is named by its workout', sessionNameFor(planned), 'Upper A');
eq('the plain form agrees', sessionName(planned), 'Upper A');

// The workout name is the one to prefer even if a title somehow arrived too —
// the server refuses that combination, so this pins which half wins if the
// contract is ever loosened rather than leaving it to argument order.
eq(
  'workout name wins over a title that should not exist',
  sessionNameFor({ ...planned, title: 'Push day' }),
  'Upper A',
);

// ------------------------------------------------------- an unplanned session

const unplanned = {
  workout_name: null,
  title: 'Push day',
  workout_template_id: null,
};

eq('an unplanned session is named by its title', sessionNameFor(unplanned), 'Push day');
eq(
  'an unplanned session with no title gets the standard name',
  sessionNameFor({ ...unplanned, title: null }),
  UNNAMED_SESSION,
);
eq(
  'a blank title is no title',
  sessionNameFor({ ...unplanned, title: '   ' }),
  UNNAMED_SESSION,
);
eq('names are trimmed', sessionNameFor({ ...unplanned, title: '  Leg day  ' }), 'Leg day');

// ------------------------------------------------------------- failed lookups

// A PLANNED session whose name did not load is a lookup problem, and the
// caller's own word for it must survive — that is the whole reason `fallback`
// is a parameter rather than a constant.
eq(
  'a planned session with no name falls back to the caller word',
  sessionNameFor({ workout_name: null, title: null, workout_template_id: '018f-aaaa' }),
  'Workout',
);
eq(
  'the caller can choose that word',
  sessionNameFor(
    { workout_name: null, title: null, workout_template_id: '018f-aaaa' },
    'your workout',
  ),
  'your workout',
);

// ...and it must NOT be confused with an unplanned session, which is not a
// failure at all. Getting these two the same way round is the bug this file
// exists to catch: "Workout" on an unnamed own-workout reads as a broken
// lookup, and UNNAMED_SESSION on a failed one hides a real problem.
check(
  'the two empty cases are told apart',
  sessionNameFor({ workout_name: null, title: null, workout_template_id: null }) !==
    sessionNameFor({ workout_name: null, title: null, workout_template_id: 'x' }),
);

// --------------------------------------------------------------- robustness

// Responses reach this from three different endpoints, and two of them omit
// fields rather than sending null. Neither may throw.
eq('missing fields entirely', sessionNameFor({}), UNNAMED_SESSION);
eq('undefined rather than null', sessionName({ workout_name: undefined, title: undefined }), 'Workout');

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
