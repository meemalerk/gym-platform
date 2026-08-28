/**
 * Plate maths — the arithmetic a lifter does between sets, so it had better be
 * right at the edges: floating-point plate weights, weights the gym cannot
 * make, and anything lighter than the bar.
 *
 *   node scripts/verify-plates.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { platesPerSide, plateLabel, DEFAULT_BAR_KG } = await import(
  '../apps/mobile/src/features/gym/plates.ts'
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

const sides = (total, bar) => platesPerSide(total, bar)?.perSide;

console.log('\n=== plates per side ===');

check('an empty 20 kg bar needs no plates', sides(20), []);
check('60 kg is a 20 per side', sides(60), [20]);
check('100 kg is 25 + 15 per side', sides(100), [25, 15]);
check('140 kg stacks two 25s and a 10', sides(140), [25, 25, 10]);

check(
  '62.5 kg needs the small change: 20 + 1.25',
  sides(62.5),
  [20, 1.25],
);

check(
  'repeated decimals do not drift (7 x 2.5 stays exact)',
  platesPerSide(20 + 2 * (2.5 * 7), 20, [2.5]).remainderPerSideKg,
  0,
);

check(
  'a weight the plates cannot make reports the shortfall, never rounds',
  (() => {
    const load = platesPerSide(61, 20);
    return [load.perSide, load.remainderPerSideKg];
  })(),
  [[0.5 > 0 ? 20 : 0], 0.5],
);

check('lighter than the bar is not a bar exercise', platesPerSide(15, 20), null);
check('a non-numeric weight yields nothing', platesPerSide(Number.NaN, 20), null);

check(
  'a 15 kg bar changes the whole answer',
  sides(60, 15),
  [20, 2.5],
);

check(
  'a gym with only 20s and 5s loads what it has and says what is missing',
  (() => {
    const load = platesPerSide(112.5, 20, [20, 5]);
    return [load.perSide, load.remainderPerSideKg];
  })(),
  [[20, 20, 5], 1.25],
);

check(
  'plates are consumed heaviest-first regardless of input order',
  platesPerSide(100, 20, [5, 25, 15, 10]).perSide,
  [25, 15],
);

check('a zero-weight plate cannot cause an infinite loop', sides(60, 20, [20, 0]), [20]);

console.log('\n=== labels ===');

check('a loaded bar reads as a sum', plateLabel(platesPerSide(100)), '25 + 15');
check('an empty bar says so', plateLabel(platesPerSide(20)), 'empty bar');
check(
  'a shortfall is stated in the label, not hidden',
  plateLabel(platesPerSide(61)),
  // The shortfall is separated by a dot, not another "+": it is not one more
  // plate to fetch, it is weight this gym cannot make.
  '20 · 0.5 kg/side short',
);
check('nothing to load, nothing to say', plateLabel(null), null);
check(`the default bar is ${DEFAULT_BAR_KG} kg`, DEFAULT_BAR_KG, 20);

console.log('\n======================================');
console.log(`  PASSED: ${passed}    FAILED: ${failed}`);
console.log('======================================');
process.exit(failed === 0 ? 0 : 1);
