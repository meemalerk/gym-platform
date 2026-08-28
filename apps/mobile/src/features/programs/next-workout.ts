/**
 * "What should I do next?" — answered deterministically from the pinned
 * programme version and the athlete's own completed sessions.
 *
 * The reference design's Today shows the *programme* and leaves the member to
 * open it, find the week, find the day and press Start. That is three taps to
 * answer a question the system can already answer. This computes the answer,
 * with a rule simple enough to state on screen:
 *
 *   the first workout, in week/day order, with no completed session against it.
 *
 * When every workout has been done the programme has come round again, so it
 * returns the first one and says so. Pure — no dates, no guessing, no ranking.
 */

export type WeekLike = { id: string; week_number: number; label?: string | null };
export type WorkoutLike = { id: string; week_id: string; day_number: number; name: string };

export type NextWorkout = {
  workoutId: string;
  name: string;
  weekNumber: number;
  weekLabel: string | null;
  dayNumber: number;
  /** True when every workout in the version already has a completed session. */
  cycleRestarted: boolean;
  /** How many of the version's workouts are done — for "3 of 12". */
  completed: number;
  total: number;
};

export function nextWorkout(
  weeks: WeekLike[],
  workouts: WorkoutLike[],
  completedTemplateIds: readonly string[],
): NextWorkout | null {
  const weekById = new Map(weeks.map((w) => [w.id, w]));

  const ordered = workouts
    .filter((w) => weekById.has(w.week_id))
    .map((w) => ({ workout: w, week: weekById.get(w.week_id)! }))
    .sort(
      (a, b) =>
        a.week.week_number - b.week.week_number || a.workout.day_number - b.workout.day_number,
    );

  if (ordered.length === 0) return null;

  const done = new Set(completedTemplateIds);
  const completed = ordered.filter((entry) => done.has(entry.workout.id)).length;
  const pending = ordered.find((entry) => !done.has(entry.workout.id));
  const chosen = pending ?? ordered[0]!;

  return {
    workoutId: chosen.workout.id,
    name: chosen.workout.name,
    weekNumber: chosen.week.week_number,
    weekLabel: chosen.week.label ?? null,
    dayNumber: chosen.workout.day_number,
    cycleRestarted: pending == null,
    completed,
    total: ordered.length,
  };
}
