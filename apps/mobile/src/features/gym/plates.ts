/**
 * What to actually put on the bar.
 *
 * "70 kg" is the number you log; "20 + 10 + 5 per side" is the thing you do
 * standing in front of a rack. Every lifter does this arithmetic in their head
 * between sets and some of them get it wrong when tired, which is a loaded-bar
 * problem, not a rounding problem.
 *
 * Greedy from the heaviest plate is optimal for the standard gym set, because
 * each denomination is at least twice the next one down except 2.5/1.25 (which
 * are the last two, so the greedy choice is forced anyway).
 *
 * Pure, and deliberately honest about failure: a weight the plates cannot make
 * reports the leftover instead of silently rounding to something you did not
 * lift.
 */

/** An Olympic barbell. Gyms with 15 kg or 7.5 kg bars pass their own. */
export const DEFAULT_BAR_KG = 20;

/** One side's worth of what a normal commercial gym racks, heaviest first. */
export const DEFAULT_PLATES_KG = [25, 20, 15, 10, 5, 2.5, 1.25] as const;

export type PlateLoad = {
  /** Plates for ONE side, heaviest first. Empty means an empty bar. */
  perSide: number[];
  /** Kilos per side the available plates could not make. 0 when exact. */
  remainderPerSideKg: number;
  barKg: number;
};

export function platesPerSide(
  totalKg: number,
  barKg: number = DEFAULT_BAR_KG,
  available: readonly number[] = DEFAULT_PLATES_KG,
): PlateLoad | null {
  if (!Number.isFinite(totalKg) || !Number.isFinite(barKg)) return null;
  // Lighter than the bar is not a loading problem — it is a different
  // implement (dumbbells, machine), and guessing would be worse than silence.
  if (totalKg < barKg) return null;

  // Work in grams: 0.1 + 0.2 arithmetic on 2.5 kg plates drifts into
  // "0.30000000000000004 kg left over" and prints nonsense.
  let remainingG = Math.round(((totalKg - barKg) / 2) * 1000);
  const perSide: number[] = [];

  for (const plate of [...available].sort((a, b) => b - a)) {
    const plateG = Math.round(plate * 1000);
    if (plateG <= 0) continue;
    while (remainingG >= plateG) {
      perSide.push(plate);
      remainingG -= plateG;
    }
  }

  return { perSide, remainderPerSideKg: remainingG / 1000, barKg };
}

/** "25 + 10 + 5" — or "empty bar", or a note about what will not fit. */
export function plateLabel(load: PlateLoad | null): string | null {
  if (!load) return null;
  const base = load.perSide.length === 0 ? 'empty bar' : load.perSide.join(' + ');
  if (load.remainderPerSideKg > 0) {
    return `${base} · ${load.remainderPerSideKg} kg/side short`;
  }
  return base;
}
