/**
 * Which clients need the coach today — computed, ordered, and each row carries
 * the reason it is there.
 *
 * Same rule as everywhere else in this product: no screen shows a number it
 * cannot explain (feature-plan §9). A coach seeing "2 need attention" must be
 * able to tap through and read *why* each one is on the list, and the why is a
 * fact about their data, never a score.
 *
 * Pure: takes plain rows and a clock, returns rows. No fetching, no dates
 * parsed from the ambient present.
 */

export type ClientLike = { athleteId: string; athleteName: string };
export type AssignmentLike = { athlete_id: string; is_active: boolean };
export type SessionLike = { athlete_id: string; started_at: string; is_open: boolean };

export type AttentionReason =
  | { kind: 'no_programme' }
  | { kind: 'idle'; days: number }
  | { kind: 'never_trained' };

export type AttentionItem = {
  athleteId: string;
  athleteName: string;
  reason: AttentionReason;
  /** Rendered sentence — the UI must not invent its own wording. */
  because: string;
};

/** Days of silence before a coach should notice. A week: one missed cycle. */
export const IDLE_DAYS = 7;

const DAY_MS = 86_400_000;

/** Whole days between two instants, floored — 6.9 days is still six. */
export function daysSince(iso: string, now: Date): number | null {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return null;
  return Math.floor((now.getTime() - then.getTime()) / DAY_MS);
}

export function clientsNeedingAttention(
  clients: readonly ClientLike[],
  assignments: readonly AssignmentLike[],
  sessions: readonly SessionLike[],
  now: Date,
): AttentionItem[] {
  const items: AttentionItem[] = [];

  for (const client of clients) {
    const hasProgramme = assignments.some(
      (a) => a.athlete_id === client.athleteId && a.is_active,
    );

    if (!hasProgramme) {
      items.push({
        athleteId: client.athleteId,
        athleteName: client.athleteName,
        reason: { kind: 'no_programme' },
        because: 'No programme assigned',
      });
      continue;
    }

    // An open session counts as training now, not as silence.
    const theirs = sessions.filter((s) => s.athlete_id === client.athleteId);
    if (theirs.some((s) => s.is_open)) continue;

    if (theirs.length === 0) {
      items.push({
        athleteId: client.athleteId,
        athleteName: client.athleteName,
        reason: { kind: 'never_trained' },
        because: 'Has a programme but has never trained',
      });
      continue;
    }

    const gaps = theirs
      .map((s) => daysSince(s.started_at, now))
      .filter((d): d is number => d != null);
    if (gaps.length === 0) continue;

    const idle = Math.min(...gaps);
    if (idle >= IDLE_DAYS) {
      items.push({
        athleteId: client.athleteId,
        athleteName: client.athleteName,
        reason: { kind: 'idle', days: idle },
        because: `No session in ${idle} days`,
      });
    }
  }

  // Worst first: unassigned, then never trained, then longest silence.
  const rank = (r: AttentionReason) =>
    r.kind === 'no_programme' ? 0 : r.kind === 'never_trained' ? 1 : 2;

  return items.sort((a, b) => {
    const byRank = rank(a.reason) - rank(b.reason);
    if (byRank !== 0) return byRank;
    const aDays = a.reason.kind === 'idle' ? a.reason.days : 0;
    const bDays = b.reason.kind === 'idle' ? b.reason.days : 0;
    if (bDays !== aDays) return bDays - aDays;
    return a.athleteName.localeCompare(b.athleteName);
  });
}

/** Everyone with a session open right now — "training now" on a coach's Today. */
export function trainingNow(
  clients: readonly ClientLike[],
  sessions: readonly SessionLike[],
): { athleteId: string; athleteName: string }[] {
  return clients.filter((c) => sessions.some((s) => s.athlete_id === c.athleteId && s.is_open));
}
