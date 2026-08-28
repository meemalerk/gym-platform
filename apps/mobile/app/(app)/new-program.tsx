import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { createProgram, type ProgramFocus } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Screen, Section } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Start a programme.
 *
 * Creates version 1 as a draft — nothing is assignable until it is published,
 * which is the point of the lifecycle rather than an obstacle in front of it.
 */
const schema = z.object({
  name: z.string().trim().min(1, 'Give it a name').max(120),
  summary: z.string().trim().max(300).optional(),
});

type FormValues = z.infer<typeof schema>;

/**
 * Focus is not decoration: it is what the recommender reads when matching a
 * programme to someone's goals. "General" recommends nothing, which is the
 * honest default until a coach says otherwise.
 */
const FOCUSES: { key: ProgramFocus; label: string; hint: string }[] = [
  { key: 'strength', label: 'Strength', hint: 'Heavy, low reps, long rests' },
  { key: 'hypertrophy', label: 'Hypertrophy', hint: 'Volume and time under tension' },
  { key: 'conditioning', label: 'Conditioning', hint: 'Work capacity and engine' },
  { key: 'general', label: 'General', hint: 'Suggested to nobody in particular' },
];

export default function NewProgram() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));

  const [focus, setFocus] = useState<ProgramFocus>('strength');
  const [error, setError] = useState<string | null>(null);

  const { control, handleSubmit, formState } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: '', summary: '' },
  });

  const save = useMutation({
    mutationFn: (values: FormValues) =>
      createProgram(gymId!, { name: values.name, summary: values.summary, focus }),
    onSuccess: (program) => {
      void queryClient.invalidateQueries({ queryKey: ['programs', gymId] });
      // Straight into the empty draft — the next thing anybody wants is to add
      // a week, not to look at a list with one item in it.
      router.replace({
        pathname: '/(app)/program/[version]',
        params: {
          version: program.latest_version.id,
          name: program.name,
          programId: program.id,
        },
      });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'A programme with that name already exists in this gym.'
          : e instanceof ApiError && e.code === 'auth.forbidden'
            ? 'Your role cannot write programmes.'
            : e instanceof ApiError && e.code === 'request.invalid'
              ? e.message
              : 'Could not create that programme. Please try again.',
      );
    },
  });

  const onSubmit = handleSubmit((values) => {
    setError(null);
    save.mutate(values);
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'New programme' }} />

      <View style={s.intro}>
        <Text style={s.h1}>New programme</Text>
        <Text style={s.lede}>
          You will start with an empty draft. Add weeks and workouts, then publish it — only a
          published version can be given to anyone.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <Controller
        control={control}
        name="name"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Name"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            autoFocus
            placeholder="Beginner Strength"
            error={formState.errors.name?.message}
          />
        )}
      />

      <View>
        <Section label="What is it for" />
        <View style={s.focuses}>
          {FOCUSES.map((option) => {
            const on = focus === option.key;
            return (
              <Pressable
                key={option.key}
                onPress={() => setFocus(option.key)}
                accessibilityRole="radio"
                accessibilityState={{ selected: on }}
                accessibilityLabel={`${option.label}. ${option.hint}.`}
                style={[s.focus, on && s.focusOn]}
              >
                <Text style={s.focusLabel}>{option.label}</Text>
                <Text style={s.focusHint}>{option.hint}</Text>
              </Pressable>
            );
          })}
        </View>
      </View>

      <Controller
        control={control}
        name="summary"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Summary (optional)"
            value={value ?? ''}
            onChangeText={onChange}
            onBlur={onBlur}
            multiline
            placeholder="Three days a week, barbell-led, for a first year of training."
            error={formState.errors.summary?.message}
          />
        )}
      />

      <Button
        label={save.isPending ? 'Creating…' : 'Create draft'}
        disabled={save.isPending}
        onPress={onSubmit}
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
    focuses: { gap: 8, paddingTop: 14 },
    focus: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      gap: 3,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    focusOn: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
    },
    focusLabel: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    focusHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
  });
