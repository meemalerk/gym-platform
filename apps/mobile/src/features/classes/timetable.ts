/**
 * Shaping the class timetable for a screen.
 *
 * Pure on purpose, like `features/session/age` and `features/coaching/attention`:
 * the server sends a flat list of dated sittings and every dashboard wants a
 * different slice of it. Keeping the slicing here means three screens agree on
 * what "this week" and "today" mean, and it is testable without a renderer.
 *
 * **Dates are wall-clock strings (`YYYY-MM-DD`), never Date objects.** The
 * server sends the gym's own local dates; parsing them into a Date drags the
 * phone's timezone in and can shift a Monday class onto Sunday for a member
 * who happens to be travelling. Comparing the strings is both correct and
 * cheaper — ISO dates sort lexicographically.
 */

export type Sitting = {
  class_id: string;
  name: string;
  description?: string | null;
  instructor_id: string;
  instructor_name: string;
  weekday: number;
  weekday_name: string;
  starts_at: string;
  duration_minutes: number;
  on_date: string;
  capacity: number;
  booked: number;
  places_left: number;
  is_full: boolean;
  booked_by_me: boolean;
};

/** "18:00:00" -> "18:00". The seconds are never meaningful for a class. */
export function timeLabel(startsAt: string): string {
  return startsAt.slice(0, 5);
}

/** "45 min", "1h", "1h 30" — short enough to sit beside a time. */
export function durationLabel(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m === 0 ? `${h}h` : `${h}h ${m}`;
}

/**
 * "12/20", plus the word that matters when it matters.
 *
 * Deliberately not a percentage: standing in a gym you want to know whether
 * there is room, and "60%" makes you do arithmetic to find out.
 */
export function occupancyLabel(sitting: Pick<Sitting, 'booked' | 'capacity' | 'is_full'>): string {
  const base = `${sitting.booked}/${sitting.capacity}`;
  return sitting.is_full ? `${base} · full` : base;
}

/** Today's date as the gym's wall-clock string, from a local Date. */
export function todayLocal(now: Date): string {
  const y = now.getFullYear();
  const m = `${now.getMonth() + 1}`.padStart(2, '0');
  const d = `${now.getDate()}`.padStart(2, '0');
  return `${y}-${m}-${d}`;
}

/** `days` forward of `from`, inclusive — the window a dashboard asks for. */
export function windowEnd(from: string, days: number): string {
  const parts = from.split('-').map(Number);
  const y = parts[0] ?? 1970;
  const m = parts[1] ?? 1;
  const d = parts[2] ?? 1;
  // UTC arithmetic on a date-only value: no zone, so no DST edge to land on.
  const at = new Date(Date.UTC(y, m - 1, d + days));
  return at.toISOString().slice(0, 10);
}

/**
 * Chronological order: date, then start time, then name.
 *
 * The server already returns this order, but a dashboard that filters and
 * concatenates can lose it, and a timetable out of order is worse than no
 * timetable.
 */
export function inOrder(sittings: Sitting[]): Sitting[] {
  return [...sittings].sort(
    (a, b) =>
      a.on_date.localeCompare(b.on_date) ||
      a.starts_at.localeCompare(b.starts_at) ||
      a.name.localeCompare(b.name),
  );
}

/** Grouped by date, in order, for a sectioned list. */
export function byDay(sittings: Sitting[]): { date: string; label: string; sittings: Sitting[] }[] {
  const groups = new Map<string, Sitting[]>();
  for (const s of inOrder(sittings)) {
    const list = groups.get(s.on_date);
    if (list) list.push(s);
    else groups.set(s.on_date, [s]);
  }
  return [...groups.entries()].map(([date, list]) => ({
    date,
    // Every sitting on a date shares its weekday, so the first one names the
    // group. The map is only built from non-empty pushes, so there is always one.
    label: list[0]?.weekday_name ?? '',
    sittings: list,
  }));
}

/** Everything on one date. */
export function on(sittings: Sitting[], date: string): Sitting[] {
  return inOrder(sittings.filter((s) => s.on_date === date));
}

/** The sittings this member holds a place in. */
export function mine(sittings: Sitting[]): Sitting[] {
  return inOrder(sittings.filter((s) => s.booked_by_me));
}

/** The sittings one instructor teaches. */
export function taughtBy(sittings: Sitting[], instructorId: string): Sitting[] {
  return inOrder(sittings.filter((s) => s.instructor_id === instructorId));
}

/**
 * How full the timetable is overall, 0..1 — the owner's one-glance number.
 *
 * Total places taken over total places offered, NOT the mean of each class's
 * rate: a full 4-person class and an empty 40-person one is 9% occupancy, not
 * 50%. Averaging rates would flatter a gym whose big classes are the empty ones.
 */
export function occupancy(sittings: Sitting[]): number {
  const capacity = sittings.reduce((n, s) => n + s.capacity, 0);
  if (capacity === 0) return 0;
  const booked = sittings.reduce((n, s) => n + s.booked, 0);
  return Math.min(1, booked / capacity);
}

/**
 * The distinct classes on the timetable, by name — what "4 classes" counts.
 *
 * A weekly slot appearing in two requested weeks is one class, so counting
 * rows would tell an owner they run twice as many as they do.
 */
export function distinctClasses(sittings: Sitting[]): number {
  return new Set(sittings.map((s) => s.class_id)).size;
}
