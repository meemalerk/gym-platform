/**
 * Rendering prescriptions the way a coach would say them.
 *
 * "4 × 6–8 · RIR 2" is the difference between a plan and a database row. All
 * pure, all pinned by `scripts/verify-program-format.mjs` — because the edge
 * cases here are exactly the ones that slip through eyeballing: RIR 0 means
 * *train to failure* and must never render like an absent RIR; a fixed target
 * must not read "5–5"; 90 seconds is "1:30", not "90 s" pretending to be a rep.
 */

/** Mirrors the server's tagged union. Kept structural so plain Node can test it. */
export type Prescription =
  | { kind: 'repetitions'; sets: number; target: { min: number; max: number }; rir?: number | null }
  | { kind: 'duration'; sets: number; seconds: number }
  | { kind: 'distance'; metres: number; pace?: { min_seconds_per_km: number; max_seconds_per_km: number } | null };

/** "6–8" for a range, "5" for a fixed target — never "5–5". */
function repsLabel(target: { min: number; max: number }): string {
  return target.min === target.max ? `${target.min}` : `${target.min}–${target.max}`;
}

/** Seconds as a human duration: 45 → "45 s", 90 → "1:30", 3900 → "1:05:00". */
export function secondsLabel(total: number): string {
  if (!Number.isFinite(total) || total <= 0) return '0 s';
  if (total < 60) return `${Math.round(total)} s`;

  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = Math.round(total % 60);
  const mm = (n: number) => `${n}`.padStart(2, '0');

  return hours > 0 ? `${hours}:${mm(minutes)}:${mm(seconds)}` : `${minutes}:${mm(seconds)}`;
}

/** Metres as a distance: 800 → "800 m", 5000 → "5 km", 2500 → "2.5 km". */
export function metresLabel(metres: number): string {
  if (!Number.isFinite(metres) || metres <= 0) return '0 m';
  if (metres < 1000) return `${Math.round(metres)} m`;

  const km = metres / 1000;
  // One decimal, with a trailing ".0" trimmed — "5 km", not "5.0 km".
  const rounded = Math.round(km * 10) / 10;
  return `${Number.isInteger(rounded) ? rounded.toFixed(0) : rounded} km`;
}

/** Pace in seconds/km as "5:00 /km"; a band collapses when min equals max. */
function paceLabel(pace: { min_seconds_per_km: number; max_seconds_per_km: number }): string {
  const one = (s: number) => {
    const minutes = Math.floor(s / 60);
    const seconds = Math.round(s % 60);
    return `${minutes}:${`${seconds}`.padStart(2, '0')}`;
  };
  const band =
    pace.min_seconds_per_km === pace.max_seconds_per_km
      ? one(pace.min_seconds_per_km)
      : `${one(pace.min_seconds_per_km)}–${one(pace.max_seconds_per_km)}`;
  return `${band} /km`;
}

/** The one-line summary a plan shows beside an exercise name. */
export function prescriptionLabel(p: Prescription): string {
  switch (p.kind) {
    case 'repetitions': {
      const base = `${p.sets} × ${repsLabel(p.target)}`;
      // `!= null` and not truthiness: RIR 0 is "to failure", a real instruction.
      return p.rir != null ? `${base} · RIR ${p.rir}` : base;
    }
    case 'duration':
      return `${p.sets} × ${secondsLabel(p.seconds)}`;
    case 'distance':
      return p.pace != null
        ? `${metresLabel(p.metres)} @ ${paceLabel(p.pace)}`
        : metresLabel(p.metres);
    default:
      // A kind this build has never seen (older app, newer server). A blank
      // line reads as broken; naming the gap reads as an update prompt.
      return 'Unsupported prescription';
  }
}

/** "Day 3 · Upper A" — the workout's line in a week list. */
export function workoutTitle(dayNumber: number, name: string): string {
  return `Day ${dayNumber} · ${name}`;
}

/** "Week 2 — Accumulation", or just "Week 2". */
export function weekTitle(weekNumber: number, label?: string | null): string {
  return label && label.trim().length > 0 ? `Week ${weekNumber} — ${label}` : `Week ${weekNumber}`;
}
