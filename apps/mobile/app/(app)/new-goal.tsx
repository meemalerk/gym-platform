import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  createGoal,
  getExerciseHistory,
  listExercises,
  listMeasurements,
  type Exercise,
  type GoalMetric,
} from '@/api/gym';
import { bestOf, sessionPoints } from '@/features/progress/metrics';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Screen, Section } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Set a goal.
 *
 * Two kinds only, and both are **observable** (ADR-0018): a number the app can
 * already measure from your own history. "Get fitter" is not offered, because a
 * goal whose progress cannot be computed is a note, and notes belong on your
 * profile.
 *
 * The baseline is read from YOUR OWN HISTORY here — the latest logged weight, or
 * the best estimated 1RM of that lift — and shown before you commit, because a
 * goal you cannot see the start of is a goal you cannot judge. Without history
 * there is no baseline and the goal is refused rather than started from zero,
 * which would show 400% progress on the first session.
 *
 * NOTE: the API takes the baseline as input, so it is this client that decides
 * it. Deriving it on the server is the stronger design and is not built yet —
 * see the gap noted in docs/feature-plan-2026-07.md.
 */
export default function NewGoal() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const userId = useSession((st) => st.user?.id);

  const [kind, setKind] = useState<'bodyweight' | 'lift'>('bodyweight');
  const [target, setTarget] = useState('');
  const [targetDate, setTargetDate] = useState('');
  const [exercise, setExercise] = useState<Exercise | null>(null);
  const [search, setSearch] = useState('');
  const [error, setError] = useState<string | null>(null);

  const exercises = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId!),
    enabled: Boolean(gymId) && kind === 'lift',
  });

  // Only movements measured in reps have an estimated 1RM to aim at — you
  // cannot have a one-rep max on a 5 km row.
  const liftable = useMemo(() => {
    const all = (exercises.data ?? []).filter((e) => e.modality.kind === 'repetitions');
    const needle = search.trim().toLowerCase();
    return (needle ? all.filter((e) => e.name.toLowerCase().includes(needle)) : all).slice(0, 20);
  }, [exercises.data, search]);

  // Where you are starting from, taken from what you have actually logged.
  const measurements = useQuery({
    queryKey: ['measurements'],
    queryFn: () => listMeasurements(),
    enabled: kind === 'bodyweight',
  });

  const liftHistory = useQuery({
    queryKey: ['exercise-history', gymId, exercise?.id, userId],
    queryFn: () => getExerciseHistory(gymId!, exercise!.id, userId!),
    enabled: kind === 'lift' && Boolean(gymId && exercise && userId),
  });

  const baseline = useMemo<number | null>(() => {
    if (kind === 'bodyweight') {
      // Newest first is not guaranteed by the API, so pick by date rather than
      // by position — an assumption about order is how the wrong start creeps in.
      const withWeight = (measurements.data ?? []).filter((m) => m.weight_kg != null);
      if (withWeight.length === 0) return null;
      const latest = withWeight.reduce((a, b) => (a.measured_on >= b.measured_on ? a : b));
      return latest.weight_kg ?? null;
    }
    const best = bestOf(sessionPoints(liftHistory.data ?? []));
    return best?.score ?? null;
  }, [kind, measurements.data, liftHistory.data]);

  const baselineLoading =
    (kind === 'bodyweight' && measurements.isLoading) ||
    (kind === 'lift' && Boolean(exercise) && liftHistory.isLoading);

  const targetKg = Number.parseFloat(target);
  const targetOk = Number.isFinite(targetKg) && targetKg > 0 && targetKg < 1000;
  const dateOk = targetDate === '' || /^\d{4}-\d{2}-\d{2}$/.test(targetDate);
  // A target equal to the baseline is not a goal; the domain refuses it too.
  const movesSomewhere = baseline != null && Math.abs(targetKg - baseline) >= 0.1;
  const ready =
    targetOk &&
    dateOk &&
    baseline != null &&
    movesSomewhere &&
    (kind === 'bodyweight' || exercise !== null);

  const metric = (): GoalMetric =>
    kind === 'bodyweight'
      ? ({ kind: 'bodyweight', baseline_kg: baseline!, target_kg: targetKg } as GoalMetric)
      : ({
          kind: 'exercise_est_1rm',
          exercise_id: exercise!.id,
          baseline_kg: baseline!,
          target_kg: targetKg,
        } as GoalMetric);

  const save = useMutation({
    mutationFn: () =>
      createGoal(gymId!, {
        athleteId: userId!,
        metric: metric(),
        targetDate: targetDate === '' ? null : targetDate,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['goals', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['recommendations', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? // The most common one by far: a bodyweight goal with no weight
            // logged yet, so there is nothing to measure from.
            e.message
          : e instanceof ApiError && e.code === 'resource.conflict'
            ? 'You already have an open goal for that.'
            : 'Could not set that goal. Please try again.',
      );
    },
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'New goal' }} />

      <View style={s.intro}>
        <Text style={s.h1}>New goal</Text>
        <Text style={s.lede}>
          Something the app can measure, so progress is computed from what you actually do rather
          than from how you feel about it.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <View>
        <Section label="What are you aiming at" />
        <View style={s.kinds}>
          {(
            [
              { key: 'bodyweight' as const, label: 'Bodyweight', hint: 'Up or down — from your logged weight' },
              { key: 'lift' as const, label: 'A lift', hint: 'Estimated 1RM from your sets' },
            ]
          ).map((option) => {
            const on = kind === option.key;
            return (
              <Pressable
                key={option.key}
                onPress={() => setKind(option.key)}
                accessibilityRole="radio"
                accessibilityState={{ selected: on }}
                accessibilityLabel={`${option.label}. ${option.hint}.`}
                style={[s.kind, on && s.kindOn]}
              >
                <Text style={s.kindLabel}>{option.label}</Text>
                <Text style={s.kindHint}>{option.hint}</Text>
              </Pressable>
            );
          })}
        </View>
      </View>

      {kind === 'lift' ? (
        <View>
          <Section label="Which lift" />
          <TextInput
            value={search}
            onChangeText={setSearch}
            placeholder="Search…"
            placeholderTextColor={t.color.faint}
            autoCapitalize="none"
            style={s.search}
            accessibilityLabel="Search for a lift"
          />
          <View style={s.chips}>
            {liftable.map((e) => {
              const on = exercise?.id === e.id;
              return (
                <Pressable
                  key={e.id}
                  onPress={() => setExercise(e)}
                  accessibilityRole="radio"
                  accessibilityState={{ selected: on }}
                  style={[s.chip, on && s.chipOn]}
                >
                  <Text style={[s.chipText, on && s.chipTextOn]}>{e.name}</Text>
                </Pressable>
              );
            })}
          </View>
          {liftable.length === 0 && !exercises.isLoading ? (
            <Text style={s.hint}>
              No rep-based movements in the catalogue yet — a 1RM goal needs one.
            </Text>
          ) : null}
        </View>
      ) : null}

      <Field
        label={kind === 'bodyweight' ? 'Target bodyweight · kg' : 'Target 1RM · kg'}
        value={target}
        onChangeText={setTarget}
        keyboardType="decimal-pad"
        placeholder={kind === 'bodyweight' ? '78' : '140'}
        inputStyle={s.num}
        error={target !== '' && !targetOk ? 'A weight in kilograms' : undefined}
      />

      <Field
        label="By when (optional)"
        value={targetDate}
        onChangeText={setTargetDate}
        placeholder="YYYY-MM-DD"
        autoCapitalize="none"
        error={dateOk ? undefined : 'A date like 2026-12-01'}
      />

      {baselineLoading ? (
        <Text style={s.note}>Looking up where you are starting from…</Text>
      ) : baseline != null ? (
        <View style={s.baseline}>
          <Text style={s.baselineLabel}>Starting from</Text>
          <Text style={s.baselineValue}>{baseline.toFixed(1)} kg</Text>
          <Text style={s.baselineHint}>
            {kind === 'bodyweight'
              ? 'Your most recent logged weight.'
              : `Your best estimated 1RM for ${exercise?.name ?? 'this lift'}.`}
          </Text>
        </View>
      ) : (kind === 'bodyweight' || exercise) ? (
        <Text style={s.warn}>
          {kind === 'bodyweight'
            ? 'Log your weight on the Body screen first — a goal needs a starting point to measure from.'
            : 'You have not logged a set of this lift yet. Train it once, then set the goal.'}
        </Text>
      ) : null}

      {baseline != null && targetOk && !movesSomewhere ? (
        <Text style={s.warn}>That is where you already are. Pick a different number.</Text>
      ) : null}

      <Button
        label={save.isPending ? 'Saving…' : 'Set goal'}
        disabled={save.isPending || !ready}
        onPress={() => {
          setError(null);
          save.mutate();
        }}
      />
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    intro: { gap: 6, paddingTop: t.space.sm },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
    },
    lede: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5, lineHeight: 20 },
    kinds: { flexDirection: 'row', gap: 10, paddingTop: 14 },
    kind: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      flex: 1,
      gap: 3,
      paddingHorizontal: 14,
      paddingVertical: 13,
    },
    kindOn: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
    },
    kindLabel: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    kindHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    search: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      color: t.color.ink,
      fontFamily: fonts.regular,
      fontSize: t.font.md,
      marginTop: 14,
      paddingHorizontal: 14,
      paddingVertical: 11,
    },
    chips: { flexDirection: 'row', flexWrap: 'wrap', gap: 6, paddingTop: 10 },
    chip: {
      borderRadius: t.radius.pill,
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      paddingHorizontal: 12,
      paddingVertical: 8,
    },
    chipOn: { backgroundColor: t.color.accent, borderColor: t.color.accent },
    chipText: { color: t.color.mut, fontFamily: fonts.semibold, fontSize: t.font.xs },
    chipTextOn: { color: t.color.onAccent },
    num: {
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    hint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs, paddingTop: 10 },
    note: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.xs, lineHeight: 18 },
    warn: { color: t.color.danger, fontFamily: fonts.regular, fontSize: t.font.xs, lineHeight: 18 },
    baseline: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      gap: 3,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    baselineLabel: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.xs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    baselineValue: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      fontVariant: ['tabular-nums'],
    },
    baselineHint: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.xs },
  });
