/**
 * How old is an open session, and is that number still worth showing?
 *
 * A workout session is opened by a person and closed by a person, so some get
 * left open — the phone dies, the gym closes, life happens. Counting up from
 * there produces "158 h 30 elapsed", which is not a stopwatch, it is a bug
 * report rendered as a feature. Past a threshold the honest thing is to stop
 * counting and say when it was opened, so the member can resume or discard it
 * knowingly.
 *
 * Pure, clock-injected: the same instant in, the same string out.
 */

/** Beyond this, an "in progress" workout is a leftover, not a workout. */
export const STALE_HOURS = 12;

export type SessionAge = {
  /** Whole minutes since it opened; null when the timestamp is unusable. */
  minutes: number | null;
  /** "23 min", "3 h 05", or "opened Tue 21 Jul" once stale. */
  label: string;
  /** True once it has been open long enough that elapsed time is meaningless. */
  stale: boolean;
};

export function sessionAge(startedAt: string, now: Date): SessionAge {
  const started = new Date(startedAt);
  if (Number.isNaN(started.getTime())) {
    return { minutes: null, label: '', stale: false };
  }

  const minutes = Math.max(0, Math.floor((now.getTime() - started.getTime()) / 60_000));

  if (minutes >= STALE_HOURS * 60) {
    const when = started.toLocaleDateString(undefined, {
      weekday: 'short',
      day: 'numeric',
      month: 'short',
    });
    return { minutes, label: `opened ${when}`, stale: true };
  }

  if (minutes < 60) return { minutes, label: `${minutes} min`, stale: false };

  const hours = Math.floor(minutes / 60);
  return {
    minutes,
    label: `${hours} h ${String(minutes % 60).padStart(2, '0')}`,
    stale: false,
  };
}
