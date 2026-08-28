import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { Controller, useForm } from 'react-hook-form';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { createClass, listMembers } from '@/api/gym';
import { can, useSession } from '@/session/store';
import {
  Button,
  Callout,
  ErrorBanner,
  Field,
  Screen,
  Section,
  Segmented,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Put a class on the timetable.
 *
 * A class is a WEEKLY SLOT, not a one-off booking — "Zumba, Mondays at 18:00"
 * — so this form asks for a weekday and a time, never a date. Every Monday is
 * then derived from it, which is why there is nothing here about how many weeks
 * it runs for: it runs until somebody takes it off.
 */
const schema = z.object({
  name: z.string().trim().min(1, 'Give it a name').max(80),
  description: z.string().trim().max(300).optional(),
  // A time, typed. 24-hour because a gym timetable is written that way, and
  // because am/pm is one more thing to get wrong at 6 in the morning.
  time: z
    .string()
    .trim()
    .regex(/^([01]\d|2[0-3]):[0-5]\d$/, 'A time like 18:00'),
  duration: z
    .string()
    .trim()
    .regex(/^\d{1,3}$/, 'Minutes, e.g. 45')
    .refine((v) => Number(v) >= 5 && Number(v) <= 300, 'Between 5 and 300 minutes'),
  capacity: z
    .string()
    .trim()
    .regex(/^\d{1,3}$/, 'How many people fit')
    .refine((v) => Number(v) >= 1 && Number(v) <= 500, 'Between 1 and 500'),
});

type FormValues = z.infer<typeof schema>;

/** 0 = Sunday, matching the server, the database and JavaScript `getDay()`. */
const DAYS = [
  { key: '1' as const, label: 'Mon' },
  { key: '2' as const, label: 'Tue' },
  { key: '3' as const, label: 'Wed' },
  { key: '4' as const, label: 'Thu' },
  { key: '5' as const, label: 'Fri' },
  { key: '6' as const, label: 'Sat' },
  { key: '0' as const, label: 'Sun' },
];

export default function NewClassScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const capacities = useSession((st) => st.membership?.capacities ?? []);
  const manages = can.manageGym(capacities);

  const [weekday, setWeekday] = useState<(typeof DAYS)[number]['key']>('1');
  const [instructorId, setInstructorId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const {
    control,
    handleSubmit,
    formState: { errors },
  } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: '', description: '', time: '18:00', duration: '45', capacity: '20' },
  });

  // Anyone who can coach can teach a class. Members cannot, so the picker is
  // filtered rather than trusting the person to pick sensibly.
  const members = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId) && manages,
  });
  const instructors = (members.data ?? []).filter((m) => can.coach(m.capacities));

  const create = useMutation({
    mutationFn: (values: FormValues) =>
      createClass(gymId!, {
        name: values.name,
        instructorId: instructorId!,
        weekday: Number(weekday),
        // The API takes seconds; a timetable never means anything but :00.
        startsAt: `${values.time}:00`,
        durationMinutes: Number(values.duration),
        capacity: Number(values.capacity),
        description: values.description ?? null,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['classes', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError
          ? e.message
          : 'Could not add that class. Please try again.',
      );
    },
  });

  if (!manages) {
    return (
      <Screen scroll>
        <Stack.Screen options={{ title: 'Add a class' }} />
        <ErrorBanner message="Only an owner or admin can change the timetable." />
      </Screen>
    );
  }

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Add a class' }} />
      <View style={s.content}>
        {error ? <ErrorBanner message={error} /> : null}

        <Controller
          control={control}
          name="name"
          render={({ field }) => (
            <Field
              label="Name"
              placeholder="Zumba"
              value={field.value}
              onChangeText={field.onChange}
              onBlur={field.onBlur}
              error={errors.name?.message}
              autoCapitalize="words"
            />
          )}
        />

        <Controller
          control={control}
          name="description"
          render={({ field }) => (
            <Field
              label="Description"
              placeholder="Latin dance cardio. No experience needed."
              value={field.value ?? ''}
              onChangeText={field.onChange}
              onBlur={field.onBlur}
              error={errors.description?.message}
              hint="Optional — one line members see beside the class."
              multiline
            />
          )}
        />

        <View style={s.group}>
          <Section label="When" />
          <Segmented
            label="Day of the week"
            options={DAYS}
            value={weekday}
            onChange={setWeekday}
          />
          <View style={s.pairRow}>
            <View style={s.pairCell}>
              <Controller
                control={control}
                name="time"
                render={({ field }) => (
                  <Field
                    label="Starts"
                    placeholder="18:00"
                    value={field.value}
                    onChangeText={field.onChange}
                    onBlur={field.onBlur}
                    error={errors.time?.message}
                    keyboardType="numbers-and-punctuation"
                  />
                )}
              />
            </View>
            <View style={s.pairCell}>
              <Controller
                control={control}
                name="duration"
                render={({ field }) => (
                  <Field
                    label="Minutes"
                    placeholder="45"
                    value={field.value}
                    onChangeText={field.onChange}
                    onBlur={field.onBlur}
                    error={errors.duration?.message}
                    keyboardType="number-pad"
                  />
                )}
              />
            </View>
          </View>
          <Callout tone="accent">
            This repeats every week. It runs until somebody takes it off the
            timetable — there is no end date to set.
          </Callout>
        </View>

        <View style={s.group}>
          <Section label="Who teaches it" />
          {instructors.length === 0 ? (
            <Text style={s.note}>
              Nobody here can teach yet. Give somebody the trainer or head-coach
              standing from People first.
            </Text>
          ) : (
            <View style={s.chipWrap}>
              {instructors.map((m) => {
                const on = m.user_id === instructorId;
                return (
                  <Pressable
                    key={m.user_id}
                    onPress={() => setInstructorId(m.user_id)}
                    accessibilityRole="radio"
                    accessibilityState={{ selected: on }}
                    accessibilityLabel={`${m.display_name} teaches it`}
                    style={[s.chip, on && s.chipOn]}
                  >
                    <Text style={[s.chipText, on && s.chipTextOn]}>{m.display_name}</Text>
                  </Pressable>
                );
              })}
            </View>
          )}
        </View>

        <View style={s.group}>
          <Section label="How many fit" />
          <Controller
            control={control}
            name="capacity"
            render={({ field }) => (
              <Field
                label="Places"
                placeholder="20"
                value={field.value}
                onChangeText={field.onChange}
                onBlur={field.onBlur}
                error={errors.capacity?.message}
                hint="Mats, bikes, floor space — whatever actually limits it."
                keyboardType="number-pad"
              />
            )}
          />
        </View>

        <Button
          label={create.isPending ? 'Adding…' : 'Add to the timetable'}
          disabled={create.isPending || instructorId == null}
          onPress={handleSubmit((values) => {
            setError(null);
            create.mutate(values);
          })}
        />
        {instructorId == null && instructors.length > 0 ? (
          <Text style={s.note}>Pick who teaches it to carry on.</Text>
        ) : null}
      </View>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    content: { gap: t.space.lg, paddingBottom: t.space.huge },
    group: { gap: t.space.sm },
    pairRow: { flexDirection: 'row', gap: t.space.md },
    pairCell: { flex: 1 },
    note: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.sm, lineHeight: 18 },
    chipWrap: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.sm },
    chip: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.pill,
      paddingHorizontal: t.space.md,
      paddingVertical: 9,
    },
    chipOn: { backgroundColor: t.color.accent },
    chipText: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.sm },
    chipTextOn: { color: t.color.onAccent },
  });
