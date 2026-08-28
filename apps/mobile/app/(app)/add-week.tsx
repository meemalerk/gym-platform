import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { addWeek } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Add a week to a draft.
 *
 * The number is pre-filled with the next one rather than left blank: weeks are
 * almost always added in order, and typing "4" after adding three is the kind of
 * friction that produces a programme with two week 3s.
 */
export default function AddWeek() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const { version, next } = useLocalSearchParams<{ version: string; next?: string }>();

  const [number, setNumber] = useState(next ?? '1');
  const [label, setLabel] = useState('');
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () =>
      addWeek(gymId!, version, { weekNumber: Number.parseInt(number, 10), label }),
    /*
      Hand straight on to the next step instead of returning to the version
      screen.

      A programme is weeks → workouts → exercises, and each of those used to end
      in `router.back()`. So somebody who had just created a programme was
      offered "Add the first week", did it, landed back on a screen whose only
      visible action was the same lifecycle button — with no indication that a
      WORKOUT was the next thing, and exercises another level below that. The
      reasonable conclusion was that you cannot add exercises at all.

      `replace`, not `push`: the form has been submitted, so going back from the
      next step should return to the programme, never to a filled-in form that
      would create a second week.
    */
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: ['program-version', gymId, version] });
      router.replace({
        pathname: '/(app)/add-workout',
        params: {
          week: created.id,
          version,
          weekLabel: label.trim() ? `Week ${number} · ${label.trim()}` : `Week ${number}`,
          taken: '',
        },
      });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? `This programme already has a week ${number}.`
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : e instanceof ApiError && e.status === 409
              ? 'This version is published and can no longer be edited.'
              : 'Could not add that week. Please try again.',
      );
    },
  });

  const parsed = Number.parseInt(number, 10);
  const valid = Number.isFinite(parsed) && parsed >= 1 && parsed <= 52;

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Add a week' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Add a week</Text>
        <Text style={s.lede}>
          A week holds the workouts for those seven days. Most programmes run four to twelve.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <Field
        label="Week number"
        value={number}
        onChangeText={setNumber}
        keyboardType="number-pad"
        autoFocus
        placeholder="1"
        inputStyle={s.number}
        error={number !== '' && !valid ? 'A week number from 1 to 52' : undefined}
      />

      <Field
        label="Label (optional)"
        value={label}
        onChangeText={setLabel}
        placeholder="Accumulation"
      />

      <Button
        label={save.isPending ? 'Adding…' : 'Add week'}
        disabled={save.isPending || !valid}
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
    number: {
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
  });
