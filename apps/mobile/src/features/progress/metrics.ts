/**
 * Progress metrics, computed at the edge from raw history.
 *
 * Deliberately client-side (docs: "progress is computed, never stored") — the
 * server returns performed sets, and every number a member sees is derived
 * fresh from them. A stored "estimated max" would eventually disagree with the
 * sets it came from; a computed one cannot.
 *
 * Estimated 1RM uses Epley: `w × (1 + reps/30)`. It is an estimate for a chart,
 * not a claim — capped at 12 reps because rep-max formulas drift into fiction
 * beyond that, and a 20-rep set simply does not predict a single.
 */

export type PerformedLike =
  | { kind: 'repetitions'; reps: number; weight_kg?: number | null }
  | { kind: 'duration'; seconds: number }
  | { kind: 'distance'; metres: number };

export type HistoryEntryLike = {
  started_at: string;
  sets: { performed: PerformedLike }[];
};

const EPLEY_MAX_REPS = 12;

/** Epley estimate, or null when the maths would be fiction. */
export function estimated1Rm(weightKg: number, reps: number): number | null {
  if (!Number.isFinite(weightKg) || weightKg <= 0) return null;
  if (!Number.isInteger(reps) || reps < 1 || reps > EPLEY_MAX_REPS) return null;
  // One decimal is plenty for a chart label.
  return Math.round(weightKg * (1 + reps / 30) * 10) / 10;
}

/**
 * The single number that summarises one set for progress purposes:
 * strength work → estimated 1RM; holds → seconds; distance → metres.
 * Null when the set carries nothing comparable (bodyweight reps, failed set).
 */
export function setScore(performed: PerformedLike): number | null {
  switch (performed.kind) {
    case 'repetitions':
      return performed.weight_kg != null
        ? estimated1Rm(performed.weight_kg, performed.reps)
        : null;
    case 'duration':
      return performed.seconds > 0 ? performed.seconds : null;
    case 'distance':
      return performed.metres > 0 ? performed.metres : null;
    default:
      return null;
  }
}

export type SessionPoint = {
  startedAt: string;
  /** Best comparable set of the session, by score. */
  score: number;
  /** The set behind the score, for the label ("5 × 72.5 kg"). */
  best: PerformedLike;
};

/**
 * One point per session: its best comparable set. Sessions where nothing is
 * comparable (all bodyweight, all failed) drop out rather than charting as
 * zero — a zero on a strength chart reads as an injury, not a data gap.
 */
export function sessionPoints(entries: HistoryEntryLike[]): SessionPoint[] {
  const points: SessionPoint[] = [];
  for (const entry of entries) {
    let best: { score: number; performed: PerformedLike } | null = null;
    for (const set of entry.sets) {
      const score = setScore(set.performed);
      if (score != null && (best == null || score > best.score)) {
        best = { score, performed: set.performed };
      }
    }
    if (best != null) {
      points.push({ startedAt: entry.started_at, score: best.score, best: best.performed });
    }
  }
  return points;
}

export type Trend = {
  first: number;
  last: number;
  /** Signed change, last − first, rounded to one decimal. */
  delta: number;
};

/** Overall movement across the points. Needs two to say anything. */
export function trendOf(points: SessionPoint[]): Trend | null {
  const first = points[0];
  const last = points[points.length - 1];
  if (points.length < 2 || first == null || last == null) return null;
  return {
    first: first.score,
    last: last.score,
    delta: Math.round((last.score - first.score) * 10) / 10,
  };
}

/** The all-time best point, for the headline stat. */
export function bestOf(points: SessionPoint[]): SessionPoint | null {
  let best: SessionPoint | null = null;
  for (const p of points) {
    if (best == null || p.score > best.score) best = p;
  }
  return best;
}

/**
 * BMI: weight ÷ height². Computed here and shown as a number with its formula —
 * never stored, never editorialised into categories. It is a rough population
 * metric, and the app says exactly that next to it.
 */
export function bmi(weightKg: number, heightCm: number): number | null {
  if (!Number.isFinite(weightKg) || weightKg <= 0) return null;
  if (!Number.isFinite(heightCm) || heightCm < 50 || heightCm > 280) return null;
  const metres = heightCm / 100;
  return Math.round((weightKg / (metres * metres)) * 10) / 10;
}

/**
 * 0..1 progress from baseline toward target, direction-agnostic: a cut from 82
 * to 78 and a lift from 70 to 100 both read "how much of the distance is
 * covered". Clamped — overshooting reads as done, regressing reads as zero,
 * and neither produces a bar outside its track.
 */
export function goalProgress(baseline: number, target: number, current: number): number {
  const span = target - baseline;
  if (!Number.isFinite(span) || span === 0 || !Number.isFinite(current)) return 0;
  return Math.min(1, Math.max(0, (current - baseline) / span));
}

/** 0..1 bar heights for a chart, scaled to the running maximum. */
export function barHeights(points: SessionPoint[]): number[] {
  const max = points.reduce((m, p) => Math.max(m, p.score), 0);
  if (max <= 0) return points.map(() => 0);
  return points.map((p) => p.score / max);
}
