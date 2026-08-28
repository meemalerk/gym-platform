/**
 * Attendance and duration maths — the numbers a coach reads about a client.
 *
 * Calendar arithmetic is where this app has already been bitten once (see
 * verify-activity.mjs and its midnight cases), and attendance adds two more
 * traps worth pinning: the UTC-vs-local day boundary, and averaging over
 * sessions whose duration is unknown.
 *
 * Pure module, so this needs no device, no renderer and no server.
 *
 *   node scripts/verify-attendance.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const {
  attendanceCalendar,
  daysBetween,
  formatDuration,
  localDayKey,
  summarise,
  todayKey,
} = await import('../apps/mobile/src/features/coaching/attendance.ts');

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
    console.log(`  FAIL  ${label} — got ${a} want ${e}`);
  }
}

function ok(label, condition) {
  check(label, Boolean(condition), true);
}

/** A session that started at a given local time, lasting `minutes`. */
const session = (iso, minutes, extra = {}) => ({
  id: iso,
  started_at: iso,
  duration_seconds: minutes == null ? null : minutes * 60,
  status: { state: 'completed' },
  ...extra,
});

console.log('\n=== 1. the day an event belongs to is LOCAL ===');
{
  // The bug this guards: `toISOString().slice(0,10)` gives the UTC day, so a
  // late-evening session east of Greenwich lands on tomorrow and an early
  // morning one west of it lands on yesterday. Both are days the athlete
  // would swear they did not train.
  const late = new Date(2026, 2, 15, 23, 30); // 15 March, 23:30 local
  check('a late-evening session stays on its own day', localDayKey(late.toISOString()), '2026-03-15');

  const early = new Date(2026, 2, 15, 0, 30); // 15 March, 00:30 local
  check('and so does an early-morning one', localDayKey(early.toISOString()), '2026-03-15');

  check('an unparseable timestamp yields null, not a wrong day', localDayKey('not a date'), null);
}

console.log('\n=== 2. whole days between two dates ===');
{
  check('same day is zero', daysBetween('2026-03-15', '2026-03-15'), 0);
  check('consecutive days', daysBetween('2026-03-15', '2026-03-16'), 1);
  check('order does not matter', daysBetween('2026-03-16', '2026-03-15'), 1);
  check('across a month boundary', daysBetween('2026-01-31', '2026-02-01'), 1);
  check('across a year boundary', daysBetween('2026-12-31', '2027-01-01'), 1);
  check('a leap day counts once', daysBetween('2028-02-28', '2028-03-01'), 2);
  check('a non-leap February does not', daysBetween('2026-02-28', '2026-03-01'), 1);

  // Spring forward: a naive local-Date subtraction gives 0.958 days here and
  // floors to 0, reporting "trained today" for a session two days ago.
  check('a DST spring-forward is still one day', daysBetween('2026-03-28', '2026-03-29'), 1);
  check('a DST fall-back is still one day', daysBetween('2026-10-24', '2026-10-25'), 1);
}

console.log('\n=== 3. the calendar keeps its gaps ===');
{
  const now = new Date(2026, 2, 15, 12, 0);
  const cal = attendanceCalendar([], [], 7, now);

  check('every day in the window is present', cal.length, 7);
  check('oldest first', cal[0].date, '2026-03-09');
  check('ending today', cal.at(-1).date, '2026-03-15');
  ok('a quiet week is all gaps', cal.every((d) => !d.trained && !d.attended));

  // The gaps ARE the content: a coach is looking for the week somebody
  // stopped coming, and a list with the empty days removed cannot show it.
  const sparse = attendanceCalendar(
    [session(new Date(2026, 2, 9, 18, 0).toISOString(), 45),
     session(new Date(2026, 2, 15, 7, 0).toISOString(), 60)],
    [],
    7,
    now,
  );
  check('two sessions, five gaps between them', sparse.filter((d) => !d.trained).length, 5);
  check('the first day is marked', sparse[0].trained, true);
  check('the last day is marked', sparse.at(-1).trained, true);
}

console.log('\n=== 4. training and attending are different facts ===');
{
  const now = new Date(2026, 2, 15, 12, 0);
  const trainedOnly = new Date(2026, 2, 14, 18, 0).toISOString();
  const scannedOnly = new Date(2026, 2, 13, 18, 0).toISOString();

  const cal = attendanceCalendar(
    [session(trainedOnly, 50)],
    [{ scanned_at: scannedOnly, allowed: true }],
    7,
    now,
  );
  const byDate = Object.fromEntries(cal.map((d) => [d.date, d]));

  check('training at home is trained, not attended',
    [byDate['2026-03-14'].trained, byDate['2026-03-14'].attended], [true, false]);
  check('scanning in and leaving is attended, not trained',
    [byDate['2026-03-13'].trained, byDate['2026-03-13'].attended], [false, true]);

  // A refused scan is a real event at the door, but it is not attendance —
  // showing it as such would credit someone for being turned away.
  const refused = attendanceCalendar(
    [],
    [{ scanned_at: new Date(2026, 2, 12, 18, 0).toISOString(), allowed: false }],
    7,
    now,
  );
  ok('a refused scan is not attendance', refused.every((d) => !d.attended));
}

console.log('\n=== 5. durations ===');
{
  check('under an hour', formatDuration(48 * 60), '48m');
  check('over an hour, zero-padded', formatDuration(65 * 60), '1h 05m');
  check('exactly two hours', formatDuration(7200), '2h 00m');
  check('rounds to the nearest minute', formatDuration(89), '1m');
  check('unknown reads as a dash, not zero', formatDuration(null), '—');
  check('undefined too', formatDuration(undefined), '—');
  check('and zero, which means the same thing here', formatDuration(0), '—');
}

console.log('\n=== 6. the summary a coach reads first ===');
{
  const now = new Date(2026, 2, 15, 12, 0);
  const cal = attendanceCalendar(
    [
      session(new Date(2026, 2, 15, 7, 0).toISOString(), 60),
      session(new Date(2026, 2, 14, 7, 0).toISOString(), 40),
      session(new Date(2026, 2, 13, 7, 0).toISOString(), 50),
      session(new Date(2026, 2, 10, 7, 0).toISOString(), 30),
    ],
    [],
    14,
    now,
  );
  const s = summarise(cal, now);

  check('counts every session', s.sessions, 4);
  check('totals the time', s.seconds, (60 + 40 + 50 + 30) * 60);
  check('averages it', s.averageSeconds, 45 * 60);
  check('trained today, so zero days since', s.daysSinceLast, 0);
  check('longest streak is the run of three', s.longestStreak, 3);
}

console.log('\n=== 7. unknown durations do not drag the average down ===');
{
  // The trap: treating an unknown duration as zero. One 50-minute session and
  // one session of unknown length would report a 25-minute average, which is
  // not "roughly right" — it is a made-up number that gets worse the more old
  // history a gym has.
  const now = new Date(2026, 2, 15, 12, 0);
  const cal = attendanceCalendar(
    [
      session(new Date(2026, 2, 15, 7, 0).toISOString(), 50),
      session(new Date(2026, 2, 14, 7, 0).toISOString(), null),
    ],
    [],
    7,
    now,
  );
  const s = summarise(cal, now);

  check('both sessions are counted', s.sessions, 2);
  check('only the timed one contributes to the total', s.seconds, 50 * 60);
  check('and the average is over timed sessions only', s.averageSeconds, 50 * 60);
}

console.log('\n=== 8. nothing at all ===');
{
  const now = new Date(2026, 2, 15, 12, 0);
  const s = summarise(attendanceCalendar([], [], 30, now), now);

  check('no sessions', s.sessions, 0);
  check('no time', s.seconds, 0);
  // Null, not 0: "we have no idea" and "they averaged nothing" are different
  // claims, and only one of them is true.
  check('no average, rather than an average of zero', s.averageSeconds, null);
  check('never trained reads as null, not a huge number', s.daysSinceLast, null);
  check('no streak', s.longestStreak, 0);
}

console.log('\n=== 9. a gap a coach should act on ===');
{
  const now = new Date(2026, 2, 15, 12, 0);
  const cal = attendanceCalendar(
    [session(new Date(2026, 2, 1, 7, 0).toISOString(), 45)],
    [],
    30,
    now,
  );
  const s = summarise(cal, now);
  check('two weeks since they last trained', s.daysSinceLast, 14);
  check('and the streak was one day', s.longestStreak, 1);
}

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}\n`);
process.exit(failed === 0 ? 0 : 1);
