/**
 * Attendance: which days an athlete showed up, and how long they trained.
 *
 * Pure on purpose — no React, no fetching — so `scripts/verify-attendance.mjs`
 * can exercise the date arithmetic without a device. That matters more here
 * than almost anywhere else in the app, because calendar maths has a long tail
 * of cases (month boundaries, DST, "today" in a different timezone from the
 * server) that are invisible until someone's Sunday session lands on Monday.
 *
 * **Two different truths, deliberately kept apart.** A gym check-in says
 * somebody came through the door. A workout session says somebody trained.
 * They are not the same event and merging them would quietly misreport
 * adherence in both directions — a member who scans in and leaves would look
 * like they trained, and one who trains at home would look absent. So a day
 * carries both flags and the UI shows both.
 */

/** One session, in the shape the API returns. */
export type AttendanceSession = {
  id: string;
  started_at: string;
  is_open?: boolean;
  /** Null while open, and for history recorded before end times were kept. */
  duration_seconds?: number | null;
  status: { state: string };
  workout_name?: string | null;
  program_name?: string | null;
  set_count?: number | null;
};

/** One door scan. */
export type AttendanceCheckIn = {
  scanned_at: string;
  allowed: boolean;
};

export type AttendanceDay = {
  /** `YYYY-MM-DD` in local time — the athlete's day, not UTC's. */
  date: string;
  /** They logged training on this day. */
  trained: boolean;
  /** They came through the door on this day (and were let in). */
  attended: boolean;
  /** Total seconds trained, across every session that day. */
  seconds: number;
  /** Sessions started on this day, newest first. */
  sessions: AttendanceSession[];
};

/**
 * Local calendar day for an ISO instant.
 *
 * `toISOString().slice(0, 10)` is the tempting one-liner and it is wrong: it
 * gives the UTC day, so a 9pm session in UTC+5 lands on tomorrow and a 1am one
 * in UTC-5 lands on yesterday. Both are dates the athlete would swear they did
 * not train. Build the key from the local parts instead.
 */
export function localDayKey(iso: string): string | null {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  const month = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${month}-${day}`;
}

/** Whole days between two day-keys. Order-independent. */
export function daysBetween(a: string, b: string): number {
  // UTC midnights: the difference between two calendar days must not depend on
  // whether a DST boundary sits between them. Using local Date objects here
  // yields 0.958… days across a spring-forward and floors to the wrong answer.
  const left = utcMidnight(a);
  const right = utcMidnight(b);
  return Math.round(Math.abs(right - left) / 86_400_000);
}

/** `YYYY-MM-DD` → the UTC-midnight instant for that calendar day. */
function utcMidnight(key: string): number {
  const parts = key.split('-');
  return Date.UTC(Number(parts[0]), Number(parts[1]) - 1, Number(parts[2]));
}

/** Today as a day-key, in local time. */
export function todayKey(now: Date = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  return `${now.getFullYear()}-${month}-${day}`;
}

/**
 * The last `days` calendar days, oldest first, each marked with what happened.
 *
 * Every day is present, including empty ones — a calendar with the gaps removed
 * is a list, and the gaps are the point: a coach is looking for the week
 * somebody stopped coming.
 */
export function attendanceCalendar(
  sessions: AttendanceSession[],
  checkIns: AttendanceCheckIn[],
  days: number,
  now: Date = new Date(),
): AttendanceDay[] {
  const byDay = new Map<string, AttendanceDay>();

  // Seed every day in the window so gaps survive into the output.
  for (let i = days - 1; i >= 0; i -= 1) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
    const key = todayKey(d);
    byDay.set(key, { date: key, trained: false, attended: false, seconds: 0, sessions: [] });
  }

  for (const session of sessions) {
    const key = localDayKey(session.started_at);
    const day = key && byDay.get(key);
    if (!day) continue;
    day.trained = true;
    day.seconds += Math.max(0, session.duration_seconds ?? 0);
    day.sessions.push(session);
  }

  for (const checkIn of checkIns) {
    // A refused scan is not attendance. It is a real event worth recording at
    // the door, but "they were turned away" must not read as "they came in".
    if (!checkIn.allowed) continue;
    const key = localDayKey(checkIn.scanned_at);
    const day = key && byDay.get(key);
    if (day) day.attended = true;
  }

  for (const day of byDay.values()) {
    day.sessions.sort((a, b) => b.started_at.localeCompare(a.started_at));
  }

  return [...byDay.values()];
}

/** "1h 05m" / "48m" / "—". Compact, because it sits in a dense list. */
export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || seconds <= 0) return '—';
  const total = Math.round(seconds / 60);
  const hours = Math.floor(total / 60);
  const minutes = total % 60;
  if (hours === 0) return `${minutes}m`;
  return `${hours}h ${String(minutes).padStart(2, '0')}m`;
}

export type AttendanceSummary = {
  /** Days trained in the window. */
  sessions: number;
  /** Total time trained, in seconds. */
  seconds: number;
  /** Mean session length, in seconds. Null with nothing to average. */
  averageSeconds: number | null;
  /** Days since the most recent session. Null if they never trained. */
  daysSinceLast: number | null;
  /** Longest run of consecutive days with a session. */
  longestStreak: number;
};

/**
 * The numbers a coach reads first.
 *
 * Sessions with no known duration are counted but contribute nothing to the
 * total — and, importantly, are excluded from the average's denominator too.
 * Averaging a 50-minute session with an unknown one as if the unknown were zero
 * would report 25 minutes, which is worse than saying less.
 */
export function summarise(calendar: AttendanceDay[], now: Date = new Date()): AttendanceSummary {
  const all = calendar.flatMap((d) => d.sessions);
  const timed = all.filter((s) => (s.duration_seconds ?? 0) > 0);
  const seconds = timed.reduce((sum, s) => sum + (s.duration_seconds ?? 0), 0);

  const trainedDays = calendar.filter((d) => d.trained);
  const last = trainedDays.at(-1);

  let longestStreak = 0;
  let run = 0;
  for (const day of calendar) {
    run = day.trained ? run + 1 : 0;
    if (run > longestStreak) longestStreak = run;
  }

  return {
    sessions: all.length,
    seconds,
    averageSeconds: timed.length > 0 ? Math.round(seconds / timed.length) : null,
    daysSinceLast: last ? daysBetween(last.date, todayKey(now)) : null,
    longestStreak,
  };
}
