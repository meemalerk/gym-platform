import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { addWorkout } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Screen, Section } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/** Day of the week, as a coach writes it on a whiteboard. */
const DAYS = [
  { n: 1, short: 'Mon' },
  { n: 2, short: 'Tue' },
  { n: 3, short: 'Wed' },
  { n: 4, short: 'Thu' },
  { n: 5, short: 'Fri' },
  { n: 6, short: 'Sat' },
  { n: 7, short: 'Sun' },
];

/**
 * Add a workout to a week.
 *
 * The day is a row of buttons rather than a number field, because "day 3" is
 * how the data is stored but "Wed" is how anyone writing a programme thinks.
 */
export default function AddWorkout() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const { week, version, weekLabel, taken } = useLocalSearchParams<{
    week: string;
    version: string;
    weekLabel?: string;
    taken?: string;
  }>();

  // Days already used in this week, so we can grey them out instead of letting
  // the server reject the save after the form has been filled in.
  const used = new Set((taken ?? '').split(',').filter(Boolean).map(Number));
  const firstFree = DAYS.find((d) => !used.has(d.n))?.n ?? 1;

  const [day, setDay] = useState(firstFree);
  const [name, setName] = useState('');
  const [notes, setNotes] = useState('');
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () => addWorkout(gymId!, week, { dayNumber: day, name, notes }),
    /*
      On to the exercise picker. A workout with nothing prescribed in it is not
      trainable — the review gate refuses it outright (ADR-0033) — so landing
      the author on the thing that fixes that is the only sensible next screen.

      `replace` for the same reason as `add-week`: back should reach the
      programme, not a submitted form that would add a second workout.
    */
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: ['program-version', gymId, version] });
      router.replace({
        pathname: '/(app)/prescribe',
        params: {
          workout: created.id,
          version,
          workoutName: name.trim() || 'this workout',
        },
      });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'That day already has a workout in this week.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : 'Could not add that workout. Please try again.',
      );
    },
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Add a workout' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Add a workout</Text>
        <Text style={s.lede}>
          {weekLabel ? `Going into ${weekLabel}. ` : ''}Name it the way you would call it out —
          &ldquo;Lower A&rdquo;, &ldquo;Push&rdquo;, &ldquo;Long run&rdquo;.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <View>
        <Section label="Which day" />
        <View style={s.days}>
          {DAYS.map((d) => {
            const on = day === d.n;
            const busy = used.has(d.n);
            return (
              <Pressable
                key={d.n}
                onPress={() => !busy && setDay(d.n)}
                disabled={busy}
                accessibilityRole="radio"
                accessibilityState={{ selected: on, disabled: busy }}
                accessibilityLabel={busy ? `${d.short}, already has a workout` : d.short}
                style={[s.day, on && s.dayOn, busy && s.dayBusy]}
              >
                <Text style={[s.dayText, on && s.dayTextOn, busy && s.dayTextBusy]}>
                  {d.short}
                </Text>
              </Pressable>
            );
          })}
        </View>
        {used.size > 0 ? (
          <Text style={s.hint}>Greyed-out days already have a workout in this week.</Text>
        ) : null}
      </View>

      <Field
        label="Name"
        value={name}
        onChangeText={setName}
        placeholder="Lower A"
        autoFocus
      />

      <Field
        label="Notes (optional)"
        value={notes}
        onChangeText={setNotes}
        multiline
        placeholder="Warm up thoroughly; leave two in the tank on the last set."
      />

      <Button
        label={save.isPending ? 'Adding…' : 'Add workout'}
        disabled={save.isPending || name.trim() === ''}
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
    days: { flexDirection: 'row', flexWrap: 'wrap', gap: 6, paddingTop: 14 },
    day: {
      borderRadius: t.radius.md,
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      minWidth: 52,
      paddingHorizontal: 10,
      paddingVertical: 11,
    },
    dayOn: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
    },
    dayBusy: { opacity: 0.4 },
    dayText: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.sm },
    dayTextOn: { color: t.color.ink },
    dayTextBusy: { color: t.color.mut },
    hint: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      paddingTop: 8,
    },
  });
