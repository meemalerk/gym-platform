/**
 * Verify the class-timetable shaping — what three dashboards each show.
 *
 *   node scripts/verify-timetable.mjs
 *
 * The load-bearing claims: dates stay wall-clock STRINGS (parsing them drags
 * the phone's timezone in and can move a Monday class onto Sunday), occupancy
 * is places-over-places rather than a mean of rates, and a weekly slot seen in
 * two weeks is still one class.
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const {
  byDay,
  distinctClasses,
  durationLabel,
  inOrder,
  mine,
  occupancy,
  occupancyLabel,
  on,
  taughtBy,
  timeLabel,
  todayLocal,
  windowEnd,
} = await import('../apps/mobile/src/features/classes/timetable.ts');

let passed = 0;
let failed = 0;
const eq = (label, actual, expected) => {
  if (JSON.stringify(actual) === JSON.stringify(expected)) {
    passed += 1;
  } else {
    failed += 1;
    console.log(
      `  FAIL  ${label}\n        expected ${JSON.stringify(expected)}\n        got      ${JSON.stringify(actual)}`,
    );
  }
};

console.log('=== class timetable ===');

// ------------------------------------------------------------------ labels

eq('a start time loses its seconds', timeLabel('18:00:00'), '18:00');
eq('an early class too', timeLabel('07:00:00'), '07:00');

eq('under an hour reads in minutes', durationLabel(45), '45 min');
eq('exactly an hour reads as an hour', durationLabel(60), '1h');
eq('longer reads as hours and minutes', durationLabel(90), '1h 30');
eq('a short class still reads', durationLabel(5), '5 min');

// Places, not percentages: standing in a gym you want to know if there is room.
eq('occupancy reads as places', occupancyLabel({ booked: 12, capacity: 20, is_full: false }), '12/20');
eq('a full class says so', occupancyLabel({ booked: 20, capacity: 20, is_full: true }), '20/20 · full');

// -------------------------------------------------------------- date windows

// No Date parsing of the incoming strings, so no zone can shift a day.
eq('today is formatted from local parts', todayLocal(new Date(2026, 7, 26, 23, 30)), '2026-08-26');
eq('  and pads single digits', todayLocal(new Date(2026, 0, 5, 0, 1)), '2026-01-05');

eq('a six-day window ends on the seventh day', windowEnd('2026-08-26', 6), '2026-09-01');
eq('  and crosses a month end', windowEnd('2026-08-31', 1), '2026-09-01');
eq('  and a leap day', windowEnd('2028-02-28', 1), '2028-02-29');
eq('  and a year end', windowEnd('2026-12-31', 1), '2027-01-01');

// ------------------------------------------------------------------ fixtures

const sit = (over) => ({
  class_id: 'c1',
  name: 'Zumba',
  description: null,
  instructor_id: 't1',
  instructor_name: 'Trainer',
  weekday: 1,
  weekday_name: 'Monday',
  starts_at: '18:00:00',
  duration_minutes: 45,
  on_date: '2026-08-31',
  capacity: 20,
  booked: 0,
  places_left: 20,
  is_full: false,
  booked_by_me: false,
  ...over,
});

const week = [
  sit({ class_id: 'yoga', name: 'Yoga', on_date: '2026-08-26', weekday_name: 'Wednesday', starts_at: '19:00:00', capacity: 15, booked: 4, places_left: 11, instructor_id: 'owner', instructor_name: 'Owner' }),
  sit({ class_id: 'pil', name: 'Pilates', on_date: '2026-08-27', weekday_name: 'Thursday', starts_at: '18:30:00', capacity: 12, booked: 9, places_left: 3, booked_by_me: true }),
  sit({ class_id: 'zum', name: 'Zumba', on_date: '2026-08-31', weekday_name: 'Monday', starts_at: '18:00:00', capacity: 20, booked: 12, places_left: 8 }),
  sit({ class_id: 'hiit', name: 'High Intensity Cardio', on_date: '2026-09-01', weekday_name: 'Tuesday', starts_at: '07:00:00', capacity: 20, booked: 20, places_left: 0, is_full: true, booked_by_me: true }),
];

// ------------------------------------------------------------------ ordering

eq(
  'chronological by date then time',
  inOrder([...week].reverse()).map((s) => s.class_id),
  ['yoga', 'pil', 'zum', 'hiit'],
);

// Two classes on the same day sort by start time, not by name.
const sameDay = [
  sit({ class_id: 'evening', name: 'Aardvark', on_date: '2026-08-31', starts_at: '18:00:00' }),
  sit({ class_id: 'morning', name: 'Zebra', on_date: '2026-08-31', starts_at: '06:00:00' }),
];
eq('same day sorts by time, not name', inOrder(sameDay).map((s) => s.class_id), ['morning', 'evening']);

// Same day AND same time falls back to name, so the order is at least stable.
const sameSlot = [
  sit({ class_id: 'b', name: 'Pilates', on_date: '2026-08-31', starts_at: '18:00:00' }),
  sit({ class_id: 'a', name: 'Boxing', on_date: '2026-08-31', starts_at: '18:00:00' }),
];
eq('an identical slot falls back to name', inOrder(sameSlot).map((s) => s.name), ['Boxing', 'Pilates']);

// -------------------------------------------------------------- the slices

eq('grouped by day, in order', byDay(week).map((g) => g.date), [
  '2026-08-26',
  '2026-08-27',
  '2026-08-31',
  '2026-09-01',
]);
eq('  each group carries its weekday name', byDay(week).map((g) => g.label), [
  'Wednesday',
  'Thursday',
  'Monday',
  'Tuesday',
]);
eq('  and its own sittings', byDay(week).map((g) => g.sittings.length), [1, 1, 1, 1]);

eq('one day at a time', on(week, '2026-08-31').map((s) => s.class_id), ['zum']);
eq('  a day with nothing on is empty', on(week, '2026-08-30'), []);

// The member's own places — what "Your bookings: 2" counts.
eq('my bookings', mine(week).map((s) => s.class_id), ['pil', 'hiit']);

// The instructor's own classes — "your classes today".
eq('one instructor sees their own', taughtBy(week, 't1').map((s) => s.class_id), ['pil', 'zum', 'hiit']);
eq('  and not somebody else\'s', taughtBy(week, 'owner').map((s) => s.class_id), ['yoga']);
eq('  an instructor with none gets none', taughtBy(week, 'nobody'), []);

// ------------------------------------------------------------- occupancy

// 45 booked of 67 places offered.
eq('occupancy is places over places', Math.round(occupancy(week) * 100), 67);

// The trap: averaging each class's RATE flatters a gym whose big classes are
// the empty ones. A full 4-seater beside an empty 40-seater is 9%, not 50%.
const lopsided = [
  sit({ class_id: 'tiny', capacity: 4, booked: 4, is_full: true }),
  sit({ class_id: 'big', capacity: 40, booked: 0 }),
];
eq('a full small class does not flatter an empty big one', Math.round(occupancy(lopsided) * 100), 9);
eq('an empty timetable is 0, not NaN', occupancy([]), 0);
eq('a timetable of zero-capacity rows is 0, not NaN', occupancy([sit({ capacity: 0, booked: 0 })]), 0);

// A weekly slot seen twice is ONE class — counting rows would double it.
const twoWeeks = [
  sit({ class_id: 'zum', on_date: '2026-08-31' }),
  sit({ class_id: 'zum', on_date: '2026-09-07' }),
  sit({ class_id: 'yoga', on_date: '2026-09-02' }),
];
eq('a slot in two weeks is one class', distinctClasses(twoWeeks), 2);
eq('  and its rows are still two sittings', twoWeeks.filter((s) => s.class_id === 'zum').length, 2);
eq('nothing on is no classes', distinctClasses([]), 0);

console.log('');
console.log('======================================');
console.log(`  PASSED: ${passed}   FAILED: ${failed}`);
console.log('======================================');
process.exit(failed === 0 ? 0 : 1);
