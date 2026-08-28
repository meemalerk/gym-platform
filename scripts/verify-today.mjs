/**
 * The two decisions Today makes, tested without a renderer.
 *
 *   nextWorkout            — "what should I train next?"
 *   clientsNeedingAttention — "who needs me today?"
 *
 * Both are pure, and both are the kind of logic that is quietly wrong for
 * months: an off-by-one in week ordering just shows the wrong day, and a
 * timezone-naive idle count nags a coach about someone who trained yesterday.
 *
 *   node scripts/verify-today.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { nextWorkout } = await import('../apps/mobile/src/features/programs/next-workout.ts');
const { clientsNeedingAttention, trainingNow, daysSince, IDLE_DAYS } = await import(
  '../apps/mobile/src/features/coaching/attention.ts'
);
const { sessionAge, STALE_HOURS } = await import('../apps/mobile/src/features/session/age.ts');

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

// ---------------------------------------------------------------- fixtures

const weeks = [
  { id: 'w2', week_number: 2, label: 'Volume, heavier' },
  { id: 'w1', week_number: 1, label: 'Volume' },
];
const workouts = [
  { id: 'w1d3', week_id: 'w1', day_number: 3, name: 'Pull' },
  { id: 'w1d1', week_id: 'w1', day_number: 1, name: 'Push' },
  { id: 'w2d1', week_id: 'w2', day_number: 1, name: 'Push' },
];

console.log('\n=== next workout ===');

check(
  'nothing done -> week 1, day 1',
  nextWorkout(weeks, workouts, [])?.workoutId,
  'w1d1',
);

check(
  'input order does not matter (weeks and days are sorted)',
  nextWorkout(weeks, workouts, [])?.weekNumber,
  1,
);

check(
  'first done -> the next day in the same week, not the next week',
  nextWorkout(weeks, workouts, ['w1d1'])?.workoutId,
  'w1d3',
);

check(
  'week 1 finished -> crosses into week 2',
  nextWorkout(weeks, workouts, ['w1d1', 'w1d3'])?.workoutId,
  'w2d1',
);

check(
  'progress counts completed against the whole version',
  (() => {
    const n = nextWorkout(weeks, workouts, ['w1d1']);
    return [n?.completed, n?.total];
  })(),
  [1, 3],
);

check(
  'everything done -> starts the cycle again, and says so',
  (() => {
    const n = nextWorkout(weeks, workouts, ['w1d1', 'w1d3', 'w2d1']);
    return [n?.workoutId, n?.cycleRestarted];
  })(),
  ['w1d1', true],
);

check(
  'a pending workout is not a restarted cycle',
  nextWorkout(weeks, workouts, ['w1d1'])?.cycleRestarted,
  false,
);

check('an empty version has no next workout', nextWorkout([], [], []), null);

check(
  'a workout whose week is missing is ignored, not crashed on',
  nextWorkout([{ id: 'w1', week_number: 1 }], [{ id: 'orphan', week_id: 'gone', day_number: 1, name: 'X' }], []),
  null,
);

check(
  'a completed id that is not in this version is simply ignored',
  nextWorkout(weeks, workouts, ['some-other-programme'])?.workoutId,
  'w1d1',
);

// ------------------------------------------------------------- attention

console.log('\n=== clients needing attention ===');

const NOW = new Date('2026-07-25T09:00:00Z');
const daysAgo = (n) => new Date(NOW.getTime() - n * 86_400_000).toISOString();

const clients = [
  { athleteId: 'a', athleteName: 'Ana Ruiz' },
  { athleteId: 'b', athleteName: 'Ben Cole' },
  { athleteId: 'c', athleteName: 'Cy Novak' },
  { athleteId: 'd', athleteName: 'Dee Park' },
];

const assignments = [
  { athlete_id: 'a', is_active: true },
  { athlete_id: 'b', is_active: true },
  { athlete_id: 'c', is_active: true },
  // 'd' has an assignment, but a withdrawn one — that is not cover.
  { athlete_id: 'd', is_active: false },
];

const sessions = [
  { athlete_id: 'a', started_at: daysAgo(1), is_open: false },
  { athlete_id: 'b', started_at: daysAgo(12), is_open: false },
  { athlete_id: 'b', started_at: daysAgo(30), is_open: false },
  // 'c' has a programme and has never logged anything.
];

const attention = clientsNeedingAttention(clients, assignments, sessions, NOW);

check(
  'flags only those who need it',
  attention.map((i) => i.athleteId),
  ['d', 'c', 'b'],
);

check(
  'a client who trained yesterday is not nagged about',
  attention.some((i) => i.athleteId === 'a'),
  false,
);

check(
  'a withdrawn assignment counts as no programme',
  attention.find((i) => i.athleteId === 'd')?.because,
  'No programme assigned',
);

check(
  'never-trained is its own reason, not a huge idle count',
  attention.find((i) => i.athleteId === 'c')?.because,
  'Has a programme but has never trained',
);

check(
  'idle is measured from the MOST RECENT session, not the oldest',
  attention.find((i) => i.athleteId === 'b')?.because,
  'No session in 12 days',
);

check(
  `the threshold is ${IDLE_DAYS} days: exactly at it flags`,
  clientsNeedingAttention(
    [{ athleteId: 'x', athleteName: 'Xu' }],
    [{ athlete_id: 'x', is_active: true }],
    [{ athlete_id: 'x', started_at: daysAgo(IDLE_DAYS), is_open: false }],
    NOW,
  ).length,
  1,
);

check(
  'one day under the threshold does not flag',
  clientsNeedingAttention(
    [{ athleteId: 'x', athleteName: 'Xu' }],
    [{ athlete_id: 'x', is_active: true }],
    [{ athlete_id: 'x', started_at: daysAgo(IDLE_DAYS - 1), is_open: false }],
    NOW,
  ).length,
  0,
);

check(
  'someone mid-workout is training, not idle — even after a long gap',
  clientsNeedingAttention(
    [{ athleteId: 'x', athleteName: 'Xu' }],
    [{ athlete_id: 'x', is_active: true }],
    [
      { athlete_id: 'x', started_at: daysAgo(40), is_open: false },
      { athlete_id: 'x', started_at: daysAgo(0), is_open: true },
    ],
    NOW,
  ).length,
  0,
);

check(
  'a malformed timestamp is skipped rather than becoming NaN days',
  clientsNeedingAttention(
    [{ athleteId: 'x', athleteName: 'Xu' }],
    [{ athlete_id: 'x', is_active: true }],
    [{ athlete_id: 'x', started_at: 'not-a-date', is_open: false }],
    NOW,
  ).length,
  0,
);

check('daysSince rejects a malformed timestamp', daysSince('nope', NOW), null);

check(
  'a partial day does not round up: 6.9 days is 6',
  daysSince(new Date(NOW.getTime() - 6.9 * 86_400_000).toISOString(), NOW),
  6,
);

check(
  'training now lists only open sessions',
  trainingNow(clients, [
    { athlete_id: 'a', started_at: daysAgo(0), is_open: true },
    { athlete_id: 'b', started_at: daysAgo(1), is_open: false },
  ]).map((c) => c.athleteName),
  ['Ana Ruiz'],
);

// ------------------------------------------------------------ session age

console.log('\n=== open-session age ===');

const minsAgo = (n) => new Date(NOW.getTime() - n * 60_000).toISOString();

check('under an hour reads in minutes', sessionAge(minsAgo(23), NOW).label, '23 min');
check('just opened is zero, not negative', sessionAge(minsAgo(0), NOW).label, '0 min');

check(
  'a clock skewed into the future clamps to zero rather than counting down',
  sessionAge(new Date(NOW.getTime() + 5 * 60_000).toISOString(), NOW).minutes,
  0,
);

check('over an hour reads h + zero-padded minutes', sessionAge(minsAgo(185), NOW).label, '3 h 05');
check('an hour exactly switches format', sessionAge(minsAgo(60), NOW).label, '1 h 00');

check(
  `under ${STALE_HOURS}h is still a live workout`,
  sessionAge(minsAgo(STALE_HOURS * 60 - 1), NOW).stale,
  false,
);

check(
  `at ${STALE_HOURS}h it stops counting and says when instead`,
  (() => {
    const a = sessionAge(minsAgo(STALE_HOURS * 60), NOW);
    return [a.stale, a.label.startsWith('opened ')];
  })(),
  [true, true],
);

check(
  'a six-day leftover never renders an absurd elapsed time',
  /^\d+ (min|h)/.test(sessionAge(minsAgo(6 * 24 * 60), NOW).label),
  false,
);

check('a malformed timestamp yields no number and no claim', sessionAge('nope', NOW), {
  minutes: null,
  label: '',
  stale: false,
});

console.log('\n======================================');
console.log(`  PASSED: ${passed}    FAILED: ${failed}`);
console.log('======================================');
process.exit(failed === 0 ? 0 : 1);
