/**
 * Verify progress metrics — the numbers a member's chart claims.
 *
 *   node scripts/verify-progress.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { barHeights, bestOf, estimated1Rm, sessionPoints, setScore, trendOf } = await import(
  '../apps/mobile/src/features/progress/metrics.ts'
);

let passed = 0;
let failed = 0;
const eq = (label, actual, expected) => {
  if (JSON.stringify(actual) === JSON.stringify(expected)) {
    passed += 1;
  } else {
    failed += 1;
    console.log(`  FAIL  ${label}\n        expected ${JSON.stringify(expected)}\n        got      ${JSON.stringify(actual)}`);
  }
};

console.log('=== progress metrics ===');

// -------------------------------------------------------------- Epley 1RM

eq('textbook epley', estimated1Rm(100, 5), 116.7);
eq('a single IS the max', estimated1Rm(100, 1), 103.3);
eq('rounds to one decimal', estimated1Rm(62.5, 8), 79.2);
// Beyond 12 reps the formula is fiction, and fiction must not chart.
eq('13+ reps refuse to estimate', estimated1Rm(60, 13), null);
eq('zero reps refuse', estimated1Rm(100, 0), null);
eq('zero weight refuses', estimated1Rm(0, 5), null);
eq('negative weight refuses', estimated1Rm(-60, 5), null);

// -------------------------------------------------------------- set scores

eq('strength set scores by est 1RM', setScore({ kind: 'repetitions', reps: 5, weight_kg: 100 }), 116.7);
eq('bodyweight reps have no comparable score', setScore({ kind: 'repetitions', reps: 12 }), null);
// A failed set is real history but carries nothing to chart.
eq('failed set (0 reps) has no score', setScore({ kind: 'repetitions', reps: 0, weight_kg: 100 }), null);
eq('holds score by seconds', setScore({ kind: 'duration', seconds: 75 }), 75);
eq('distance scores by metres', setScore({ kind: 'distance', metres: 2000 }), 2000);
eq('unknown kind scores null', setScore({ kind: 'telekinesis' }), null);

// ---------------------------------------------------------- session points

const entry = (when, sets) => ({ started_at: when, sets: sets.map((performed) => ({ performed })) });
const reps = (r, w) => ({ kind: 'repetitions', reps: r, weight_kg: w });

const history = [
  entry('2026-07-01', [reps(5, 60), reps(5, 60), reps(4, 60)]),
  entry('2026-07-08', [reps(5, 65), reps(3, 67.5)]),
  // A day of failure and bodyweight work: no comparable set at all.
  entry('2026-07-10', [reps(0, 70), { kind: 'repetitions', reps: 10 }]),
  entry('2026-07-15', [reps(5, 72.5)]),
];

const points = sessionPoints(history);
eq('one point per comparable session', points.length, 3);
eq('the no-score session dropped out, not charted as zero', points.map((p) => p.startedAt), ['2026-07-01', '2026-07-08', '2026-07-15']);
// The instructive case: 5 × 65 estimates HIGHER than 3 × 67.5 (75.8 vs 74.3).
// "Best set" means best estimated max, not heaviest bar — that distinction is
// the whole reason the metric exists.
eq('best set is by estimate, not by raw weight', points[1].score, estimated1Rm(65, 5));
eq('  and the heavier-bar set indeed scores lower', estimated1Rm(67.5, 3) < estimated1Rm(65, 5), true);
eq('the winning set rides along for the label', points[2].best, reps(5, 72.5));

// -------------------------------------------------------------- aggregates

const trend = trendOf(points);
eq('trend is last minus first', trend.delta, Math.round((points[2].score - points[0].score) * 10) / 10);
eq('a single point has no trend', trendOf(points.slice(0, 1)), null);
eq('best of all time', bestOf(points).startedAt, '2026-07-15');
eq('empty history has no best', bestOf([]), null);

// ---------------------------------------------------------------------- BMI

const { bmi } = await import('../apps/mobile/src/features/progress/metrics.ts');
eq('textbook bmi', bmi(81.4, 178), 25.7);
eq('another', bmi(70, 175), 22.9);
eq('zero weight refuses', bmi(0, 178), null);
eq('implausible height refuses', bmi(80, 30), null);
eq('missing height refuses', bmi(80, NaN), null);

// --------------------------------------------------------------- goal progress

const { goalProgress } = await import('../apps/mobile/src/features/progress/metrics.ts');
// A cut and a lift are the same question asked in opposite directions.
eq('a cut halfway down', goalProgress(82, 78, 80), 0.5);
eq('a lift part-way up', goalProgress(70, 100, 84.7), (84.7 - 70) / 30);
eq('overshooting a cut reads as done', goalProgress(82, 78, 77), 1);
eq('regressing reads as zero, not negative', goalProgress(82, 78, 83), 0);
eq('a degenerate goal cannot divide by zero', goalProgress(80, 80, 80), 0);

const heights = barHeights(points);
eq('tallest bar is exactly 1', Math.max(...heights), 1);
eq('bars scale to the max', heights.length, 3);
eq('empty history yields no bars', barHeights([]), []);

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
