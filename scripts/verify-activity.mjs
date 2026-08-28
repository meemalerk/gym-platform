/**
 * Verify the activity-hub formatting logic.
 *
 * Date grouping is the classic thing that passes every manual test during the
 * afternoon you write it, then labels an entry "Today" at 00:05 because someone
 * divided elapsed milliseconds by 86,400,000. These helpers take `now` as an
 * argument precisely so the boundaries can be pinned here.
 *
 *   node scripts/verify-activity.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const {
  categoryOf,
  countsByCategory,
  dayKey,
  dayLabel,
  filterByCategory,
  groupByDay,
  metaString,
  timeAgo,
} = await import('../apps/mobile/src/features/activity/format.ts');

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
  if (!ok) console.log(`  FAIL  ${label}\n        expected ${JSON.stringify(expected)}\n        got      ${JSON.stringify(actual)}`);
  ok ? (passed += 1) : (failed += 1);
};

const entry = (id, action, occurred_at, metadata = {}) => ({
  id,
  action,
  actor_name: 'Someone',
  entity_type: action.split('.')[0],
  metadata,
  occurred_at,
});

console.log('=== activity formatting ===');

// -------------------------------------------------------------- categories

eq('exercise → catalogue', categoryOf('exercise.created'), 'catalogue');
eq('program → catalogue', categoryOf('program.created'), 'catalogue');
eq('program_version → catalogue', categoryOf('program_version.published'), 'catalogue');
eq('invitation → people', categoryOf('invitation.created'), 'people');
eq('capacity → people', categoryOf('capacity.granted'), 'people');
eq('coach_relationship → people', categoryOf('coach_relationship.created'), 'people');
// Assigning is recorded as program.assigned but is a fact about a person; it
// must be filed with its withdrawal counterpart, not with the catalogue.
eq('program.assigned → people', categoryOf('program.assigned'), 'people');
eq('program_assignment → people', categoryOf('program_assignment.withdrawn'), 'people');
eq('workout_session → people', categoryOf('workout_session.completed'), 'people');
eq('goal → people', categoryOf('goal.achieved'), 'people');
eq('gym → gym', categoryOf('gym.created'), 'gym');
// An action we have never seen must still land somewhere visible.
eq('unknown action → gym', categoryOf('widget.frobnicated'), 'gym');
eq('malformed action → gym', categoryOf('nonsense'), 'gym');

// ------------------------------------------------------- the midnight cases

// 00:05 on the 2nd. An entry at 23:00 on the 1st is SEVEN HOURS old but is
// yesterday. Elapsed-time arithmetic gets this wrong; calendar arithmetic does not.
const justAfterMidnight = new Date(2026, 6, 2, 0, 5);
eq(
  'late last night is Yesterday, not Today',
  dayLabel(new Date(2026, 6, 1, 23, 0).toISOString(), justAfterMidnight),
  'Yesterday',
);
eq(
  'earlier this morning is Today',
  dayLabel(new Date(2026, 6, 2, 0, 1).toISOString(), justAfterMidnight),
  'Today',
);

// The reverse trap: 23:55, an entry from 00:05 the same day is nearly 24h old
// but is still today.
const lateEvening = new Date(2026, 6, 2, 23, 55);
eq(
  'this morning is still Today late at night',
  dayLabel(new Date(2026, 6, 2, 0, 5).toISOString(), lateEvening),
  'Today',
);

const now = new Date(2026, 6, 18, 12, 0);
eq('two days ago is a weekday name', dayLabel(new Date(2026, 6, 16, 9, 0).toISOString(), now), 'Thursday');
check(
  'a fortnight ago is a date, not a weekday',
  !['Today', 'Yesterday'].includes(dayLabel(new Date(2026, 6, 4, 9, 0).toISOString(), now)) &&
    dayLabel(new Date(2026, 6, 4, 9, 0).toISOString(), now).includes('4'),
);
check('invalid date does not crash', dayLabel('not-a-date', now) === 'Unknown date');
check('invalid date key does not crash', dayKey('not-a-date') === 'unknown');

// ------------------------------------------------------------------ timeAgo

eq('under a minute', timeAgo(new Date(now.getTime() - 30_000).toISOString(), now), 'just now');
eq('minutes', timeAgo(new Date(now.getTime() - 5 * 60_000).toISOString(), now), '5m ago');
eq('hours', timeAgo(new Date(now.getTime() - 3 * 3_600_000).toISOString(), now), '3h ago');
// A phone clock behind the server must not render "-4m ago".
eq('future timestamps (clock skew)', timeAgo(new Date(now.getTime() + 240_000).toISOString(), now), 'just now');
eq('invalid input', timeAgo('nope', now), '');

// ----------------------------------------------------------------- grouping

const entries = [
  entry('a', 'exercise.created', new Date(2026, 6, 18, 9, 0).toISOString()),
  entry('b', 'invitation.created', new Date(2026, 6, 17, 9, 0).toISOString()),
  entry('c', 'exercise.created', new Date(2026, 6, 18, 11, 0).toISOString()),
  entry('d', 'gym.created', new Date(2026, 6, 1, 9, 0).toISOString()),
];

const sections = groupByDay(entries, now);
eq('one section per calendar day', sections.length, 3);
// Only the relative labels are asserted literally. The older one is formatted
// by the device locale — "1 July" here, "July 1" there — so pinning a string
// would only assert which machine the test ran on.
eq('sections are newest first', sections.slice(0, 2).map((s) => s.title), ['Today', 'Yesterday']);
check('older section is a date', /1/.test(sections[2].title) && !/Today|Yesterday/.test(sections[2].title));
eq('newest entry first within a day', sections[0].data.map((e) => e.id), ['c', 'a']);

// Deliberately shuffled input: a server that returns near-sorted data would
// otherwise produce two "Today" headings, which reads as a rendering bug.
const shuffled = [entries[1], entries[3], entries[2], entries[0]];
eq(
  'unsorted input still yields one heading per day',
  groupByDay(shuffled, now).map((s) => s.title),
  sections.map((s) => s.title),
);
eq('empty input yields no sections', groupByDay([], now), []);

// ---------------------------------------------------------------- filtering

eq('all is the identity', filterByCategory(entries, 'all').length, 4);
eq('catalogue filter', filterByCategory(entries, 'catalogue').map((e) => e.id), ['a', 'c']);
eq('people filter', filterByCategory(entries, 'people').map((e) => e.id), ['b']);
eq('counts', countsByCategory(entries), { all: 4, catalogue: 2, people: 1, gym: 1 });
// Counts must add up, or a chip is lying about what it will show.
const counts = countsByCategory(entries);
check(
  'per-category counts sum to the total',
  counts.catalogue + counts.people + counts.gym === counts.all,
);

// ---------------------------------------------------------------- metadata

const withMeta = entry('e', 'exercise.created', now.toISOString(), { name: 'Back Squat' });
eq('reads a string', metaString(withMeta, 'name'), 'Back Squat');
eq('missing key', metaString(withMeta, 'email'), null);
// The schema types metadata as `unknown`, so these must not throw.
eq('null metadata', metaString({ ...withMeta, metadata: null }, 'name'), null);
eq('non-object metadata', metaString({ ...withMeta, metadata: 'oops' }, 'name'), null);
eq('non-string value', metaString({ ...withMeta, metadata: { name: 42 } }, 'name'), null);
eq('empty string counts as absent', metaString({ ...withMeta, metadata: { name: '' } }, 'name'), null);

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
