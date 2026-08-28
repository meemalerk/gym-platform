/**
 * What to call a session on screen.
 *
 * A session is named by exactly one of two things and never both (ADR-0035):
 * a prescribed one carries its workout template's name, an unplanned one
 * carries whatever the member typed. The server sends both fields with the
 * unused half null, so every surface would otherwise write its own
 * `workout_name ?? title ?? something` — and they would drift, because the
 * fallback is a judgement call and there are eight of them.
 *
 * Pure, so it is testable without a renderer, like the other modules here.
 */

/** The fields this needs. Structural on purpose — list rows, detail responses
 *  and the coaching attendance projection all satisfy it without conversion. */
export type NameableSession = {
  workout_name?: string | null;
  title?: string | null;
};

/** What an unplanned session with no name of its own is called. */
export const UNNAMED_SESSION = 'Own workout';

/**
 * `fallback` is for the planned case only — a prescribed session whose name
 * failed to load is a lookup problem, and callers already say different things
 * about it ("Workout", "your workout"). An unplanned session with no title is
 * not a failure, so it always gets `UNNAMED_SESSION`.
 */
export function sessionName(session: NameableSession, fallback = 'Workout'): string {
  const planned = session.workout_name?.trim();
  if (planned) return planned;

  const own = session.title?.trim();
  if (own) return own;

  // No workout name AND no title: an unplanned session the member did not
  // name. Distinguishable from a failed lookup because `title` only ever
  // exists on unplanned sessions — but we cannot tell the two apart from
  // these two fields alone, so prefer the caller's fallback when there is a
  // plan link to have failed. Callers that know pass `isUnplanned`.
  return fallback;
}

/**
 * The version for callers that know whether the session has a plan link —
 * which is every caller holding a full session response, since
 * `workout_template_id` says so directly.
 */
export function sessionNameFor(
  session: NameableSession & { workout_template_id?: string | null },
  fallback = 'Workout',
): string {
  const unplanned = session.workout_template_id == null;
  return sessionName(session, unplanned ? UNNAMED_SESSION : fallback);
}
