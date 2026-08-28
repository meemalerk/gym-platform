/**
 * Verify the navigation manifest against every capacity combination.
 *
 * Navigation is the one place where a mistake is invisible in review and obvious
 * to a user: a member who can see an "Activity" tab taps it and gets a 403, and a
 * gym owner who cannot see "People" simply cannot run their gym. There are only
 * eight possible capacity sets since ADR-0036 cut the ladder to three rungs, so
 * there is no excuse for sampling — this checks all of them.
 *
 * The manifest is pure by design, so this needs no device, no renderer and no
 * running server.
 *
 *   node scripts/verify-nav.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { TAB_MANIFEST, MAX_TABS, tabsFor } = await import(
  '../apps/mobile/src/navigation/tabs.ts'
);
const { CAPACITIES, can } = await import('../apps/mobile/src/session/capabilities.ts');

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

/** All 2^5 subsets of the capacity set. */
const allCapacitySets = () => {
  const sets = [];
  for (let mask = 0; mask < 1 << CAPACITIES.length; mask += 1) {
    sets.push(CAPACITIES.filter((_, i) => mask & (1 << i)));
  }
  return sets;
};

const names = (capacities) => tabsFor(capacities).map((t) => t.name);

console.log('=== navigation manifest ===');

// ---------------------------------------------------------------- invariants

for (const capacities of allCapacitySets()) {
  const label = capacities.length ? capacities.join('+') : '(none)';
  const tabs = tabsFor(capacities);

  check(`${label}: at most ${MAX_TABS} tabs`, tabs.length <= MAX_TABS);

  // Below 44pt targets the bar stops being reliably tappable.
  check(`${label}: never zero tabs`, tabs.length > 0);

  // Someone must always be able to reach their account to sign out or switch
  // gyms. Losing this strands a person in a gym they cannot leave.
  check(`${label}: can always reach You`, names(capacities).includes('you'));

  check(`${label}: unique route names`, new Set(names(capacities)).size === tabs.length);

  // Order must follow the manifest, not filter order — the bar is muscle memory.
  const manifestOrder = TAB_MANIFEST.filter((t) => names(capacities).includes(t.name)).map(
    (t) => t.name,
  );
  check(`${label}: keeps manifest order`, JSON.stringify(names(capacities)) === JSON.stringify(manifestOrder));
}

// ------------------------------------------------- capability gating is real

// The two management tabs must track the server's `can_manage_gym` exactly.
for (const capacities of allCapacitySets()) {
  const label = capacities.length ? capacities.join('+') : '(none)';
  const shown = names(capacities);
  const manages = can.manageGym(capacities);

  // People follows can_coach — a trainer needs their client list. The screen's
  // *content* narrows to what they may see; the tab itself does not vanish.
  check(`${label}: People iff can coach`, shown.includes('people') === can.coach(capacities));

  // Activity stays on can_manage_gym, matching the server's guard in
  // crates/api/src/routes/audit.rs. An audit trail names who did what, so it is
  // deliberately narrower than "can coach".
  check(`${label}: Activity iff manages gym`, shown.includes('activity') === manages);

  // Billing follows the same rule as every billing write on the server.
  check(`${label}: Billing iff manages gym`, shown.includes('billing') === manages);

  // Library and Billing share the fifth slot. Managers trade the catalogue for
  // the money; everyone else keeps the catalogue. Never both, never neither.
  check(
    `${label}: exactly one of Library / Billing`,
    shown.includes('library') !== shown.includes('billing'),
  );

  // Dropping Library from the bar is only acceptable because Today keeps a
  // route to it. Pinned here so removing that quick action fails loudly.
  check(
    `${label}: a manager who loses Library still manages the catalogue`,
    !manages || can.manageCatalogue(capacities),
  );
}

// ------------------------------------------------------- the concrete personas

const personas = [
  { label: 'member only', capacities: ['member'], expect: ['index', 'library', 'you'] },
  {
    label: 'trainer',
    capacities: ['trainer'],
    expect: ['index', 'library', 'people', 'you'],
  },
  // Managers trade Library for Billing — the fifth slot is finite, and an
  // owner opens the money more often than the catalogue.
  {
    label: 'owner',
    capacities: ['owner'],
    expect: ['index', 'people', 'billing', 'activity', 'you'],
  },
  {
    label: 'owner + member (their own gym)',
    capacities: ['owner', 'member'],
    expect: ['index', 'people', 'billing', 'activity', 'you'],
  },
  {
    label: 'trainer + member (same gym)',
    capacities: ['trainer', 'member'],
    expect: ['index', 'library', 'people', 'you'],
  },
];

for (const { label, capacities, expect } of personas) {
  check(`${label}: ${expect.join(', ')}`, JSON.stringify(names(capacities)) === JSON.stringify(expect));
}

// The rungs ADR-0036 removed must grant nothing if one ever reaches a client.
// The server's `Capacity::parse` returns None for both, so a stale row grants
// no authority there; this pins that the CLIENT agrees rather than treating an
// unrecognised string as some default.
for (const gone of ['admin', 'head_coach']) {
  check(`'${gone}' grants no gym management`, !can.manageGym([gone]));
  check(`'${gone}' grants no catalogue`, !can.manageCatalogue([gone]));
  check(`'${gone}' grants no coaching`, !can.coach([gone]));
  check(
    `'${gone}' sees only a member's tabs`,
    JSON.stringify(names([gone])) === JSON.stringify(names([])),
  );
}

// A trainer coaches but must NOT own the catalogue or see the audit trail —
// since ADR-0036 that boundary is the only one left between staff rungs, so it
// carries all the weight the head-coach/admin split used to share.
check(
  'trainer coaches but does not run the gym',
  can.coach(['trainer']) && !can.manageCatalogue(['trainer']) && !can.manageGym(['trainer']),
);
check('trainer sees no Activity tab', !names(['trainer']).includes('activity'));

// --------------------------------------------------------------------- shape

for (const tab of TAB_MANIFEST) {
  check(`${tab.name}: has a label`, Boolean(tab.label));
  check(`${tab.name}: has an icon`, Boolean(tab.icon));
  check(`${tab.name}: has a screen-reader label`, Boolean(tab.a11yLabel));
  // A one-word label keeps the bar readable at small widths.
  check(`${tab.name}: label is one word`, !tab.label.trim().includes(' '));
}

console.log(`\n  PASSED: ${passed}    FAILED: ${failed}`);
process.exit(failed === 0 ? 0 : 1);
