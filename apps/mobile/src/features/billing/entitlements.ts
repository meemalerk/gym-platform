/**
 * Turning entitlements into words.
 *
 * The server answers "what may you use, and why" — the reason travels with the
 * answer so a screen never has to say "not permitted" and leave the member
 * guessing. This module is the other half of that: the phrasing. It is pure, so
 * the wording is checked by a node script rather than by squinting at a phone.
 *
 * The rule the phrasing follows: **name the plan, not the rule.** "Your
 * Coaching membership" is something a member can act on; "entitlement
 * gym_access granted by subscription" is not.
 */

export type Feature = 'gym_access' | 'coached_programming' | 'class_credits';

export type Source =
  | { kind: 'subscription'; plan_name: string }
  | { kind: 'not_billed' }
  | { kind: 'platform_tier' };

export type Entitlement = { feature: Feature; source: Source; because: string };

/** Short label for one feature. Sentence case — these sit inside sentences. */
export function featureLabel(feature: Feature): string {
  switch (feature) {
    case 'gym_access':
      return 'Gym access';
    case 'coached_programming':
      return 'Coached programming';
    case 'class_credits':
      return 'Class credits';
  }
}

/**
 * What a plan confers, as one phrase for a list row.
 *
 * Ordered by the ladder rather than by however the array arrived, so two plans
 * granting the same things read the same way. An empty list is stated plainly:
 * the server refuses to create one, but an older row could still exist.
 */
export function summariseGrants(grants: Feature[]): string {
  if (grants.length === 0) return 'Grants nothing';

  const order: Feature[] = ['gym_access', 'coached_programming', 'class_credits'];
  const sorted = order.filter((f) => grants.includes(f));

  if (sorted.length === 1) return featureLabel(sorted[0]!);

  const labels = sorted.map((f, i) => (i === 0 ? featureLabel(f) : featureLabel(f).toLowerCase()));
  const last = labels.pop()!;
  return `${labels.join(', ')} + ${last}`;
}

/** Does this set allow the feature? */
export function holds(held: Entitlement[], feature: Feature): boolean {
  return held.some((e) => e.feature === feature);
}

/** The line to print when the feature IS held — "Your Coaching membership". */
export function reasonFor(held: Entitlement[], feature: Feature): string | null {
  return held.find((e) => e.feature === feature)?.because ?? null;
}

/**
 * The line to print when a feature is **missing** — the useful half.
 *
 * Says which of the gym's plans would grant it, because "you need a membership"
 * is a dead end and "Coaching includes this" is a next step. Falls back to a
 * plain statement when nothing on sale confers it: telling someone to buy a
 * plan that does not exist is worse than saying nothing.
 */
export function missingReason(
  feature: Feature,
  offeredPlans: { name: string; grants: Feature[]; is_offered: boolean }[],
): string {
  const named = featureLabel(feature).toLowerCase();
  const candidates = offeredPlans.filter((p) => p.is_offered && p.grants.includes(feature));

  if (candidates.length === 0) {
    return `Your membership here does not include ${named}. Ask the gym.`;
  }
  if (candidates.length === 1) {
    return `${candidates[0]!.name} includes ${named}.`;
  }

  const names = candidates.map((p) => p.name);
  const last = names.pop()!;
  return `${names.join(', ')} and ${last} include ${named}.`;
}
