import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import * as Crypto from 'expo-crypto';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { listExercises, startSession, type Exercise, type ModalityKind } from '@/api/gym';
import { useSession } from '@/session/store';
import {
  Button,
  Callout,
  Centered,
  EmptyState,
  ErrorBanner,
  Field,
  Screen,
  Section,
  Touchable,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** How an exercise is measured, in the words a member would use. */
const MEASURED_IN: Record<ModalityKind, string> = {
  repetitions: 'Reps and load',
  duration: 'Timed',
  distance: 'Distance',
};

/**
 * Build your own workout.
 *
 * This is the screen an Open Gym member starts from: they hold `gym_access`
 * and nothing else, which means no coach, which means nobody is going to
 * prescribe them anything (ADR-0034). Before this existed the app tracked
 * precisely nothing for the membership the gym sells most of.
 *
 * **Nothing on this screen is saved.** The name goes onto the session at the
 * moment it starts, and the exercises are handed to the logging screen as a
 * starting list, not a plan — there is no table for "exercises I intend to do",
 * and adding one would be a second, weaker copy of what `performed_sets`
 * records the instant a set is logged. So everything here is skippable: Start
 * with nothing chosen and add exercises as you go.
 */
export default function NewSessionScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => st.membership?.gymId ?? null);

  const [title, setTitle] = useState('');
  const [search, setSearch] = useState('');
  const [chosen, setChosen] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const catalogue = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId!),
    enabled: Boolean(gymId),
  });

  const matches = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return catalogue.data ?? [];
    return (catalogue.data ?? []).filter((e) => e.name.toLowerCase().includes(needle));
  }, [catalogue.data, search]);

  const toggle = (id: string) =>
    setChosen((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));

  const start = useMutation({
    mutationFn: () =>
      startSession(gymId!, {
        // Minted here, on the device (ADR-0008): retrying after a timeout
        // replays the same id and cannot double-start.
        id: Crypto.randomUUID(),
        title: title.trim() || null,
      }),
    onSuccess: (session) => {
      void queryClient.invalidateQueries({ queryKey: ['sessions', gymId] });
      // `replace`, not `push`: going "back" to a start screen for a session
      // that has already started is how people end up with two of them.
      router.replace({
        pathname: '/(app)/session/[id]',
        params: { id: session.id, picked: chosen.join(',') },
      });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.status === 403
          ? 'Your membership does not include gym access at the moment. Membership shows what it covers.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : 'Could not start the workout. It is safe to try again.',
      );
    },
  });

  if (catalogue.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  const empty = (catalogue.data ?? []).length === 0;

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Your own workout' }} />
      <View style={s.content}>
        {error ? <ErrorBanner message={error} /> : null}

        <Field
          label="Call it something"
          value={title}
          onChangeText={setTitle}
          placeholder="Push day"
          maxLength={80}
          hint="Optional — it is only what this shows up as in your history."
          autoCapitalize="sentences"
        />

        {catalogue.isError ? (
          <ErrorBanner message="Could not load the exercise catalogue." />
        ) : empty ? (
          <EmptyState
            glyph="◍"
            title="No exercises in the catalogue"
            hint="The gym has not added any movements yet, so there is nothing to pick from."
          />
        ) : (
          <>
            <Field
              label="Search"
              value={search}
              onChangeText={setSearch}
              placeholder="Bench, squat, row…"
              autoCapitalize="none"
              autoCorrect={false}
            />

            <View>
              <Section
                label="Pick what you are doing"
                count={matches.length}
                meta={chosen.length > 0 ? `${chosen.length} chosen` : undefined}
              />
              {matches.length === 0 ? (
                <Text style={s.note}>Nothing matches that.</Text>
              ) : (
                matches.slice(0, 60).map((exercise: Exercise, i, arr) => {
                  const picked = chosen.includes(exercise.id);
                  return (
                    <Touchable
                      key={exercise.id}
                      onPress={() => toggle(exercise.id)}
                      accessibilityLabel={`${picked ? 'Remove' : 'Add'} ${exercise.name}, ${MEASURED_IN[exercise.modality.kind]}`}
                      accessibilityState={{ selected: picked }}
                      style={[s.row, i === arr.length - 1 && s.rowLast]}
                    >
                      <View style={s.rowBody}>
                        <Text style={[s.rowName, picked && s.rowNamePicked]} numberOfLines={1}>
                          {exercise.name}
                        </Text>
                        <Text style={s.rowMeta} numberOfLines={1}>
                          {MEASURED_IN[exercise.modality.kind] ?? exercise.modality.kind}
                        </Text>
                      </View>
                      <View style={[s.tick, picked && s.tickOn]}>
                        {picked ? <Text style={s.tickMark}>✓</Text> : null}
                      </View>
                    </Touchable>
                  );
                })
              )}
              {matches.length > 60 ? (
                <Text style={s.note}>
                  {`${matches.length - 60} more — search to narrow it down.`}
                </Text>
              ) : null}
            </View>
          </>
        )}

        <Callout tone="accent">
          None of this is fixed. You can add exercises while you train, and skip
          any of these without it counting against you — this is your workout,
          not a plan somebody set.
        </Callout>

        <Button
          label={start.isPending ? 'Starting…' : 'Start workout'}
          disabled={start.isPending || !gymId}
          onPress={() => {
            setError(null);
            start.mutate();
          }}
        />
      </View>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    content: { gap: t.space.lg, paddingBottom: t.space.huge },

    row: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: t.space.md,
    },
    rowLast: { borderBottomWidth: 0 },
    rowBody: { flex: 1, gap: 2 },
    rowName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    rowNamePicked: { color: t.color.accent },
    rowMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },

    tick: {
      alignItems: 'center',
      borderColor: t.color.rule,
      borderRadius: t.radius.sm,
      borderWidth: 1,
      height: 24,
      justifyContent: 'center',
      width: 24,
    },
    tickOn: { backgroundColor: t.color.accent, borderColor: t.color.accent },
    tickMark: { color: t.color.onAccent, fontFamily: fonts.bold, fontSize: t.font.sm },

    note: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      paddingVertical: t.space.sm,
    },
  });
