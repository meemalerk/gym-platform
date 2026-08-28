/**
 * The navigation manifest.
 *
 * One person, a set of capacities in the gym — so there is no single "app". A
 * member gets a member's app, an owner gets an owner's app, and someone who is
 * both a trainer and a member gets both, from the same account
 * ([ADR-0014](../../../docs/adr/0014-identity-capacities-and-profiles.md)).
 *
 * That logic lives **here**, as data, rather than as `{can.x && <Tab/>}` sprinkled
 * through a layout, because:
 *  - it is the answer to "what can this person reach?", which is worth being able
 *    to read in one screenful
 *  - it is pure, so `scripts/verify-nav.mjs` can assert every capacity combination
 *    without a renderer or a device
 *  - adding a tab later is a line of data, not a new conditional branch
 *
 * Visibility here is **cosmetic only**. Every route re-checks on the server, and
 * `Tabs.Protected` in the layout unmounts hidden routes so they cannot be reached
 * by typing a URL either.
 */

import type Feather from '@expo/vector-icons/Feather';
import type { ComponentProps } from 'react';

import { can } from '@/session/capabilities';

/**
 * Feather glyph names, borrowed as a type so a typo fails at compile time.
 *
 * Both imports above are `import type` on purpose: they erase completely, which
 * is what lets a plain Node process load this module to test it.
 */
type IconName = ComponentProps<typeof Feather>['name'];

/** Route file names inside `app/(app)/(tabs)`. */
export type TabName = 'index' | 'library' | 'people' | 'billing' | 'activity' | 'you';

export type TabDefinition = {
  name: TabName;
  /** Shown under the icon. One word — two wrap badly at small widths. */
  label: string;
  icon: IconName;
  /** Read aloud by screen readers in place of the terse label. */
  a11yLabel: string;
  /** Whether someone holding `capacities` in the active gym sees this tab. */
  visible: (capacities: string[]) => boolean;
};

const everyone = () => true;

/**
 * Order matters: it is the order in the bar. Most-used first, self last —
 * "You" sits on the right because that is where every phone OS puts it, and
 * fighting a convention that strong costs more than it gains.
 */
export const TAB_MANIFEST: readonly TabDefinition[] = [
  {
    name: 'index',
    label: 'Today',
    icon: 'home',
    a11yLabel: 'Today',
    visible: everyone,
  },
  {
    name: 'library',
    label: 'Library',
    icon: 'grid',
    a11yLabel: 'Exercise library',
    // Members and coaches see the catalogue — reading it is how you look up a
    // movement, and only *editing* is gated inside the screen.
    //
    // Managers trade this slot for Billing. Five tabs is a hard ceiling (see
    // MAX_TABS), an owner opens billing far more often than the catalogue, and
    // the catalogue stays one tap away in Today's quick actions — which exist
    // for exactly this reason. Dropping a tab without leaving that route would
    // strand the one person who can author exercises.
    visible: (c) => !can.manageGym(c),
  },
  {
    name: 'people',
    label: 'People',
    icon: 'users',
    a11yLabel: 'People you coach, and invitations',
    // Anyone who coaches, not just managers. A trainer's client list is the
    // screen they live in; hiding it from them would make the tab useless to the
    // people who need it most. The *content* narrows instead — a trainer sees
    // their own clients, a manager sees the roster and pending invitations.
    //
    // This still fits the five-tab ceiling: a manager already satisfies
    // `can.coach`, so no capacity set gains a sixth tab. `scripts/verify-nav.mjs`
    // asserts that across all 32 combinations.
    visible: can.coach,
  },
  {
    name: 'billing',
    label: 'Billing',
    icon: 'credit-card',
    a11yLabel: 'Plans, invoices and payments',
    // Money is managed by managers, matching the server: every billing write
    // is `can_manage_gym` (crates/application/src/billing.rs).
    visible: can.manageGym,
  },
  {
    name: 'activity',
    label: 'Activity',
    icon: 'activity',
    a11yLabel: 'Audit trail',
    visible: can.readAudit,
  },
  {
    name: 'you',
    label: 'You',
    icon: 'user',
    a11yLabel: 'Your account and gyms',
    visible: everyone,
  },
];

/**
 * Five is the ceiling, and it is not arbitrary: past five, targets fall below the
 * 44pt minimum on a small phone and the bar stops being tappable. If a sixth tab
 * is ever wanted, something has to move behind the command palette (UX-3) instead.
 */
export const MAX_TABS = 5;

/** The tabs visible to someone holding `capacities` in the gym they are in. */
export function tabsFor(capacities: string[]): TabDefinition[] {
  return TAB_MANIFEST.filter((tab) => tab.visible(capacities));
}
