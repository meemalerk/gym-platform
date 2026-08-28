/**
 * Turning an audit trail into something a person reads.
 *
 * All of this is pure and takes `now` as an argument rather than calling
 * `Date.now()` internally. That is deliberate: "is this entry from today?" is
 * exactly the kind of logic that works all afternoon and breaks at midnight, or
 * for a user in a different timezone, and you cannot test it at all if it reads
 * the clock itself. `scripts/verify-activity.mjs` pins the boundaries.
 */

export type ActivityEntry = {
  id: string;
  action: string;
  actor_name?: string | null;
  entity_type: string;
  metadata: unknown;
  occurred_at: string;
};

/**
 * Broad groupings for filtering. The server's actions are `entity.verb`, so the
 * entity half is the natural axis — someone scanning the log is asking "what
 * happened to my catalogue?", not "what got created?".
 */
export const CATEGORIES = ['all', 'catalogue', 'people', 'gym'] as const;

export type Category = (typeof CATEGORIES)[number];

export const CATEGORY_LABEL: Record<Category, string> = {
  all: 'All',
  catalogue: 'Catalogue',
  people: 'People',
  gym: 'Gym',
};

/** Which filter an action falls under. Unknown actions land in `gym` rather than vanishing. */
export function categoryOf(action: string): Exclude<Category, 'all'> {
  // Assigning is recorded as `program.assigned`, but it is a fact about a
  // person, not about the catalogue — filed with its withdrawal counterpart.
  if (action === 'program.assigned') return 'people';

  const entity = action.split('.')[0];
  if (entity === 'exercise' || entity === 'program' || entity === 'program_version') {
    return 'catalogue';
  }
  if (
    entity === 'invitation' ||
    entity === 'capacity' ||
    entity === 'membership' ||
    // Who coaches whom and who is on what programme are facts about people,
    // even though a programme is involved.
    entity === 'coach_relationship' ||
    entity === 'program_assignment' ||
    entity === 'goal' ||
    // A workout happened to a person; it is their history, not the gym's config.
    entity === 'workout_session'
  ) {
    return 'people';
  }
  return 'gym';
}

/** `metadata` is `unknown` in the generated schema, so every read has to be defensive. */
export function metaString(entry: ActivityEntry, key: string): string | null {
  if (typeof entry.metadata !== 'object' || entry.metadata === null) return null;
  const value = (entry.metadata as Record<string, unknown>)[key];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** Local calendar day, as `YYYY-MM-DD`. Local, not UTC — "today" means the user's today. */
export function dayKey(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return 'unknown';
  const month = `${d.getMonth() + 1}`.padStart(2, '0');
  const day = `${d.getDate()}`.padStart(2, '0');
  return `${d.getFullYear()}-${month}-${day}`;
}

/**
 * Whole calendar days between two instants — not a division of elapsed
 * milliseconds. 11pm and 1am are seven hours apart but on different days, and
 * a log that calls that "today" is wrong in the way people notice.
 */
function calendarDaysBetween(then: Date, now: Date): number {
  const a = new Date(then.getFullYear(), then.getMonth(), then.getDate());
  const b = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((b.getTime() - a.getTime()) / 86_400_000);
}

/** Heading for a day group: "Today", "Yesterday", a weekday, or a date. */
export function dayLabel(iso: string, now: Date): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return 'Unknown date';

  const days = calendarDaysBetween(d, now);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  // Inside a week a weekday name is easier to place than a date.
  if (days < 7) return d.toLocaleDateString(undefined, { weekday: 'long' });
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString(undefined, { day: 'numeric', month: 'long' });
  }
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' });
}

/** Short relative time for an individual row. */
export function timeAgo(iso: string, now: Date): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return '';

  const ms = now.getTime() - then.getTime();
  if (ms < 0) return 'just now'; // Clock skew between phone and server.

  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;

  return then.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

export type DaySection = { key: string; title: string; data: ActivityEntry[] };

/**
 * Group entries into day sections, newest first.
 *
 * Sorts rather than trusting the server's order: a list that is *mostly* sorted
 * produces duplicate day headings, which looks like a rendering bug.
 */
export function groupByDay(entries: ActivityEntry[], now: Date): DaySection[] {
  const sections = new Map<string, ActivityEntry[]>();

  for (const entry of [...entries].sort(
    (a, b) => new Date(b.occurred_at).getTime() - new Date(a.occurred_at).getTime(),
  )) {
    const key = dayKey(entry.occurred_at);
    const existing = sections.get(key);
    if (existing) existing.push(entry);
    else sections.set(key, [entry]);
  }

  return [...sections.entries()].flatMap(([key, data]) => {
    const first = data[0];
    // A key only exists because an entry created it, so `data` is never empty.
    // `flatMap` states that totally instead of asserting it away.
    if (!first) return [];
    return [{ key, title: dayLabel(first.occurred_at, now), data }];
  });
}

/** Filter by category. `all` is the identity, so callers need no special case. */
export function filterByCategory(entries: ActivityEntry[], category: Category): ActivityEntry[] {
  if (category === 'all') return entries;
  return entries.filter((entry) => categoryOf(entry.action) === category);
}

/** How many entries each filter would show, for the counts on the chips. */
export function countsByCategory(entries: ActivityEntry[]): Record<Category, number> {
  const counts: Record<Category, number> = { all: entries.length, catalogue: 0, people: 0, gym: 0 };
  for (const entry of entries) counts[categoryOf(entry.action)] += 1;
  return counts;
}
