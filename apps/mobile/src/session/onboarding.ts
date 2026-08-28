/**
 * Who still owes a membership choice *from signing up*.
 *
 * **This used to be derived from entitlements, and that was wrong.** The rule
 * was "no `gym_access` → send them to choose a plan", which is true of a brand
 * new account and equally true of a long-standing member whose subscription
 * lapsed, was cancelled, or never existed because the gym signed them up at the
 * desk. Those people were shown the plan picker *every time they signed in*,
 * with no way past it — a wall in front of members who had already joined,
 * derived from a billing fact rather than from anything they had done.
 *
 * Choosing a plan is a step in the **registration journey**, so it is marked
 * when that journey reaches it and cleared when it finishes. Signing in is not
 * that journey and no longer consults it.
 *
 * **Keyed by user id, not a bare boolean.** Two accounts share a device more
 * often than one might think — a member and the owner testing something, a
 * shared tablet at the desk — and a bare flag left by one of them would put the
 * other in onboarding they never started.
 *
 * **Persisted**, so force-quitting on the plan screen resumes there rather than
 * dropping a half-registered member into an app where everything they tap is
 * refused. It rides in the same secure store as the refresh token — not because
 * a user id is a secret, but because that is the storage this app already has,
 * and `AsyncStorage` is not something to add for one string.
 */

import { secureStorage } from '@/session/secure-storage';

const KEY = 'gym.plan_choice_pending_for';

/** Mark this account as mid-registration, owing a plan choice. */
export async function markPlanChoicePending(userId: string): Promise<void> {
  try {
    await secureStorage.set(KEY, userId);
  } catch {
    // Storage being unavailable must not break joining a gym. The cost is that
    // a force-quit on the plan screen lands in the app instead of resuming —
    // recoverable from Membership, unlike a failed join.
  }
}

/** The account that still owes a choice, if any. */
export async function readPlanChoicePending(): Promise<string | null> {
  try {
    return await secureStorage.get(KEY);
  } catch {
    return null;
  }
}

/** They chose (or the journey ended some other way). */
export async function clearPlanChoicePending(): Promise<void> {
  try {
    await secureStorage.remove(KEY);
  } catch {
    // Nothing useful to do. The guard reads the store's in-memory copy, which
    // the caller clears too, so this only affects the next cold start.
  }
}
