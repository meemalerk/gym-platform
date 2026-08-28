/**
 * Verify prescription rendering — the line a member actually reads.
 *
 * The cases pinned here are the ones eyeballing misses: RIR 0 versus absent RIR
 * (to-failure is an instruction, not a missing value), fixed targets not
 * rendering "5–5", and 90 seconds being "1:30" rather than a rep count.
 *
 *   node scripts/verify-program-format.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { metresLabel, prescriptionLabel, secondsLabel, weekTitle, workoutTitle } = await import(
  '../apps/mobile/src/features/programs/format.ts'
);

let passed = 0;
let failed = 0;

const eq = (label, actual, expected) => {
  if (actual === expected) {
    passed += 1;
  } else {
    failed += 1;
    console.log(`  FAIL  ${label}\n        expected ${JSON.stringify(expected)}\n        got      ${JSON.stringify(actual)}`);
  }
};

console.log('=== prescription rendering ===');

// ------------------------------------------------------------- repetitions

const reps = (sets, min, max, rir) => ({ kind: 'repetitions', sets, target: { min, max }, rir });

eq('range', prescriptionLabel(reps(4, 6, 8, 2)), '4 × 6–8 · RIR 2');
eq('fixed target never reads as a range', prescriptionLabel(reps(3, 5, 5)), '3 × 5');
// The distinction the domain went out of its way to keep.
eq('RIR 0 is to-failure, not absent', prescriptionLabel(reps(3, 8, 8, 0)), '3 × 8 · RIR 0');
eq('absent RIR says nothing', prescriptionLabel(reps(5, 3, 3, null)), '5 × 3');
eq('undefined RIR says nothing', prescriptionLabel(reps(5, 3, 3, undefined)), '5 × 3');

// ---------------------------------------------------------------- duration

const dur = (sets, seconds) => ({ kind: 'duration', sets, seconds });

eq('short holds stay in seconds', prescriptionLabel(dur(3, 45)), '3 × 45 s');
eq('90 s is a time, not a rep count', prescriptionLabel(dur(3, 90)), '3 × 1:30');
eq('whole minutes', prescriptionLabel(dur(2, 600)), '2 × 10:00');
eq('over an hour', secondsLabel(3900), '1:05:00');
eq('seconds pad correctly', secondsLabel(61), '1:01');

// ---------------------------------------------------------------- distance

const dist = (metres, pace) => ({ kind: 'distance', metres, pace });

eq('short distances in metres', prescriptionLabel(dist(800)), '800 m');
eq('kilometres trim the trailing zero', prescriptionLabel(dist(5000)), '5 km');
eq('half kilometres keep one decimal', metresLabel(2500), '2.5 km');
eq(
  'pace band',
  prescriptionLabel(dist(2000, { min_seconds_per_km: 300, max_seconds_per_km: 340 })),
  '2 km @ 5:00–5:40 /km',
);
eq(
  'fixed pace collapses the band',
  prescriptionLabel(dist(5000, { min_seconds_per_km: 330, max_seconds_per_km: 330 })),
  '5 km @ 5:30 /km',
);

// ------------------------------------------------------------------ titles

eq('workout title', workoutTitle(1, 'Upper A'), 'Day 1 · Upper A');
eq('week with label', weekTitle(2, 'Accumulation'), 'Week 2 — Accumulation');
eq('week without label', weekTitle(3, null), 'Week 3');
eq('blank label is no label', weekTitle(3, '   '), 'Week 3');

// ------------------------------------------------------------- timer clock

const { clockLabel, elapsedSeconds, remainingSeconds, restProgress } = await import(
  '../apps/mobile/src/features/timer/clock.ts'
);

// The backgrounding case the whole module exists for: a timer is its END
// TIMESTAMP, so "the app slept for 40 of the 90 seconds" changes nothing.
const t0 = 1_000_000;
eq('remaining is derived, not counted', remainingSeconds(t0 + 90_000, t0 + 40_000), 50);
eq('remaining never goes negative', remainingSeconds(t0, t0 + 5_000), 0);
eq('875ms left still shows 1 second', remainingSeconds(t0 + 875, t0), 1);
eq('elapsed floors', elapsedSeconds(t0, t0 + 61_900), 61);
eq('clock skew reads as zero', elapsedSeconds(t0 + 5_000, t0), 0);

eq('timer face pads seconds', clockLabel(90), '1:30');
eq('timer face under a minute stays m:ss', clockLabel(45), '0:45');
eq('timer face at zero', clockLabel(0), '0:00');
eq('timer face double digits', clockLabel(725), '12:05');
eq('garbage clamps to 0:00', clockLabel(-3), '0:00');

eq('rest progress halfway', restProgress(t0 + 45_000, 90, t0), 0.5);
eq('rest progress complete', restProgress(t0, 90, t0 + 1), 1);
eq('zero-duration rest is already done', restProgress(t0, 0, t0), 1);

// ----------------------------------------------------------- forward compat

eq(
  'an unknown kind names the gap instead of rendering blank',
  prescriptionLabel({ kind: 'telekinesis', sets: 3 }),
  'Unsupported prescription',
);

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
