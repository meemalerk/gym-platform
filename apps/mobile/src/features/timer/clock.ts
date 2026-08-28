/**
 * Timer arithmetic, kept pure and anchored to wall-clock timestamps.
 *
 * The one rule: **a timer's truth is its end (or start) timestamp, never an
 * accumulating counter.** React Native pauses JS timers when the app is
 * backgrounded — a `count - 1` per tick silently loses every second the screen
 * was off, and a gym rest timer spends half its life in a pocket. Deriving the
 * remaining time from `endsAt - now` on every tick means backgrounding costs
 * nothing: the next tick simply tells the truth.
 */

/** Seconds left until `endsAt`. Never negative; rounds up so 0 means done. */
export function remainingSeconds(endsAtMs: number, nowMs: number): number {
  return Math.max(0, Math.ceil((endsAtMs - nowMs) / 1000));
}

/** Whole seconds since `startedAt`. Never negative (clock skew reads as 0). */
export function elapsedSeconds(startedAtMs: number, nowMs: number): number {
  return Math.max(0, Math.floor((nowMs - startedAtMs) / 1000));
}

/**
 * Timer-face format: always m:ss ("0:45", "1:30", "12:05").
 *
 * Deliberately different from `secondsLabel` in programs/format — a
 * prescription reads "45 s" in a sentence, but a ticking face that flips from
 * "1:00" to "59 s" looks broken.
 */
export function clockLabel(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return '0:00';
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = Math.floor(totalSeconds % 60);
  return `${minutes}:${`${seconds}`.padStart(2, '0')}`;
}

/** Rest presets offered in the timer bar. Seconds. */
export const REST_PRESETS = [60, 90, 120, 180] as const;

/** 0..1 progress of a rest that began `durationSeconds` before `endsAt`. */
export function restProgress(
  endsAtMs: number,
  durationSeconds: number,
  nowMs: number,
): number {
  if (durationSeconds <= 0) return 1;
  const remaining = remainingSeconds(endsAtMs, nowMs);
  return Math.min(1, Math.max(0, 1 - remaining / durationSeconds));
}
