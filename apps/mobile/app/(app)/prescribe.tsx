import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import { ApiError } from '@/api/client';
import { listExercises, prescribeExercise, type Exercise, type Prescription } from '@/api/gym';
import { prescriptionLabel } from '@/features/programs/format';
import { useSession } from '@/session/store';
import { Button, Centered, EmptyState, ErrorBanner, Field, Screen, Section } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Put an exercise into a workout.
 *
 * The form is driven by **how the chosen exercise is measured**, not by a
 * dropdown the coach picks: choose a squat and you are asked for sets and reps;
 * choose a row and you are asked for a distance. That mirrors the domain, where
 * the prescription is a modality-keyed union — "4×8 over 5 km" is not a thing
 * you can write down, here or in the database.
 *
 * The server re-checks it. This form exists so nobody has to find that out by
 * being refused.
 */
export default function Prescribe() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const { workout, version, workoutName } = useLocalSearchParams<{
    workout: string;
    version: string;
    workoutName?: string;
  }>();

  const [search, setSearch] = useState('');
  const [chosen, setChosen] = useState<Exercise | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Reps
  const [sets, setSets] = useState('3');
  const [minReps, setMinReps] = useState('8');
  const [maxReps, setMaxReps] = useState('8');
  const [rir, setRir] = useState('');
  // Time
  const [seconds, setSeconds] = useState('45');
  // Distance
  const [metres, setMetres] = useState('5000');

  const exercises = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId!),
    enabled: Boolean(gymId),
  });

  const matches = useMemo(() => {
    const all = exercises.data ?? [];
    const needle = search.trim().toLowerCase();
    if (needle === '') return all.slice(0, 30);
    return all.filter((e) => e.name.toLowerCase().includes(needle)).slice(0, 30);
  }, [exercises.data, search]);

  /** Build the union member the chosen exercise's modality demands. */
  const prescription = useMemo<Prescription | null>(() => {
    if (!chosen) return null;
    const n = (v: string) => Number.parseInt(v, 10);

    switch (chosen.modality.kind) {
      case 'repetitions': {
        const lo = n(minReps);
        const hi = n(maxReps);
        if (!Number.isFinite(n(sets)) || !Number.isFinite(lo) || !Number.isFinite(hi)) return null;
        return {
          kind: 'repetitions',
          sets: n(sets),
          target: { min: lo, max: hi },
          // `''` means "not specified"; 0 means "to failure". Conflating them
          // would turn a real instruction into a missing one.
          rir: rir.trim() === '' ? null : n(rir),
        };
      }
      case 'duration': {
        if (!Number.isFinite(n(sets)) || !Number.isFinite(n(seconds))) return null;
        return { kind: 'duration', sets: n(sets), seconds: n(seconds) };
      }
      case 'distance': {
        if (!Number.isFinite(n(metres))) return null;
        return { kind: 'distance', metres: n(metres), pace: null };
      }
      default:
        return null;
    }
  }, [chosen, sets, minReps, maxReps, rir, seconds, metres]);

  const save = useMutation({
    mutationFn: () =>
      prescribeExercise(gymId!, workout, {
        exerciseId: chosen!.id,
        prescription: prescription!,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['program-version', gymId, version] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? e.message
          : 'Could not add that exercise. Please try again.',
      );
    },
  });

  // ------------------------------------------------------------- picking one
  if (!chosen) {
    return (
      <Screen scroll edges={['bottom']}>
        <Stack.Screen options={{ title: 'Add an exercise' }} />

        <View style={s.intro}>
          <Text style={s.h1}>Add an exercise</Text>
          <Text style={s.lede}>
            {workoutName ? `Into ${workoutName}. ` : ''}Pick the movement first — how it is
            measured decides what you are asked for next.
          </Text>
        </View>

        <TextInput
          value={search}
          onChangeText={setSearch}
          placeholder="Search the catalogue…"
          placeholderTextColor={t.color.faint}
          autoCapitalize="none"
          autoCorrect={false}
          style={s.search}
          accessibilityLabel="Search the exercise catalogue"
        />

        {exercises.isLoading ? (
          <Centered>
            <ActivityIndicator color={t.color.accent} />
          </Centered>
        ) : matches.length === 0 ? (
          <EmptyState
            glyph="⌕"
            title={search ? 'Nothing matches' : 'No exercises yet'}
            hint={
              search
                ? 'Try a different word.'
                : 'Add movements to the catalogue first — a programme is built out of them.'
            }
          />
        ) : (
          <View>
            <Section label="Catalogue" count={matches.length} />
            {matches.map((e, i, arr) => (
              <Pressable
                key={e.id}
                onPress={() => setChosen(e)}
                accessibilityRole="button"
                accessibilityLabel={`Prescribe ${e.name}, measured in ${MEASURE[e.modality.kind]}`}
                style={[s.pick, i === arr.length - 1 && s.pickLast]}
              >
                <View style={s.pickBody}>
                  <Text style={s.pickName}>{e.name}</Text>
                  <Text style={s.pickMeta}>{MEASURE[e.modality.kind] ?? e.modality.kind}</Text>
                </View>
                <Text style={s.pickGo}>Choose</Text>
              </Pressable>
            ))}
          </View>
        )}
      </Screen>
    );
  }

  // --------------------------------------------------- prescribing that one
  const kind = chosen.modality.kind;

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: chosen.name }} />

      <View style={s.intro}>
        <Text style={s.h1}>{chosen.name}</Text>
        <Pressable onPress={() => setChosen(null)} accessibilityRole="button">
          <Text style={s.change}>Measured in {MEASURE[kind]} · Choose a different movement</Text>
        </Pressable>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {kind === 'repetitions' ? (
        <>
          <Field label="Sets" value={sets} onChangeText={setSets} keyboardType="number-pad" inputStyle={s.num} />
          <View style={s.row}>
            <View style={s.half}>
              <Field label="Reps from" value={minReps} onChangeText={setMinReps} keyboardType="number-pad" inputStyle={s.num} />
            </View>
            <View style={s.half}>
              <Field label="to" value={maxReps} onChangeText={setMaxReps} keyboardType="number-pad" inputStyle={s.num} />
            </View>
          </View>
          <Field
            label="Reps in reserve (optional)"
            value={rir}
            onChangeText={setRir}
            keyboardType="number-pad"
            placeholder="Leave blank if you are not prescribing it"
            inputStyle={s.num}
          />
          <Text style={s.note}>
            RIR 0 means train to failure — a real instruction, not an empty box. Leave it blank
            if you do not want to prescribe effort.
          </Text>
        </>
      ) : null}

      {kind === 'duration' ? (
        <>
          <Field label="Sets" value={sets} onChangeText={setSets} keyboardType="number-pad" inputStyle={s.num} />
          <Field label="Seconds each" value={seconds} onChangeText={setSeconds} keyboardType="number-pad" inputStyle={s.num} />
        </>
      ) : null}

      {kind === 'distance' ? (
        <Field label="Metres" value={metres} onChangeText={setMetres} keyboardType="number-pad" inputStyle={s.num} />
      ) : null}

      {prescription ? (
        <View style={s.preview}>
          <Text style={s.previewLabel}>Reads as</Text>
          <Text style={s.previewValue}>{prescriptionLabel(prescription)}</Text>
        </View>
      ) : null}

      <Button
        label={save.isPending ? 'Adding…' : 'Add to workout'}
        disabled={save.isPending || !prescription}
        onPress={() => {
          setError(null);
          save.mutate();
        }}
      />
    </Screen>
  );
}

const MEASURE: Record<string, string> = {
  repetitions: 'reps',
  duration: 'time',
  distance: 'distance',
};

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
    change: { color: t.color.accent, fontFamily: fonts.semibold, fontSize: t.font.xs },
    search: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      color: t.color.ink,
      fontFamily: fonts.regular,
      fontSize: t.font.md,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    pick: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: 12,
      paddingVertical: 13,
    },
    pickLast: { borderBottomWidth: 0 },
    pickBody: { flex: 1, gap: 2 },
    pickName: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    pickMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    pickGo: { color: t.color.accent, fontFamily: fonts.semibold, fontSize: t.font.xs },
    row: { flexDirection: 'row', gap: 12 },
    half: { flex: 1 },
    num: {
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    note: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.xs, lineHeight: 18 },
    preview: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
      gap: 4,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    previewLabel: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.xs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    previewValue: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      fontVariant: ['tabular-nums'],
    },
  });
