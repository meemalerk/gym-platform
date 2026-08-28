import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { createExercise, type Exercise } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Muted, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

const MODALITIES = ['repetitions', 'duration', 'distance'] as const;

const schema = z.object({
  name: z.string().trim().min(1, 'Name is required').max(120),
  modality: z.enum(MODALITIES),
  notes: z.string().max(1000).optional(),
});

type FormValues = z.infer<typeof schema>;

export default function NewExercise() {
  const t = useTokens();
  const styles = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((s) => (s.membership?.gymId ?? null));
  const [submitError, setSubmitError] = useState<string | null>(null);

  const { control, handleSubmit, formState } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: '', modality: 'repetitions', notes: '' },
  });

  const mutation = useMutation({
    mutationFn: (values: FormValues) =>
      createExercise(gymId!, {
        name: values.name,
        modality: values.modality,
        notes: values.notes,
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ['exercises', gymId] });
      router.back();
    },
    onError: (error) => {
      if (error instanceof ApiError) {
        if (error.code === 'resource.conflict') {
          setSubmitError('An exercise with that name already exists.');
          return;
        }
        if (error.code === 'auth.forbidden') {
          setSubmitError('Your role cannot add exercises.');
          return;
        }
      }
      setSubmitError('Could not save. Please try again.');
    },
  });

  const onSubmit = handleSubmit((values) => {
    setSubmitError(null);
    mutation.mutate(values);
  });

  return (
    <Screen>
      <Muted>Add an exercise to your gym&apos;s catalogue.</Muted>

      {submitError ? <ErrorBanner message={submitError} /> : null}

      <Controller
        control={control}
        name="name"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Name"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            placeholder="Back Squat"
            error={formState.errors.name?.message}
          />
        )}
      />

      <Controller
        control={control}
        name="modality"
        render={({ field: { onChange, value } }) => (
          <View style={styles.group}>
            <Text style={styles.label}>How is it measured?</Text>
            <View style={styles.options}>
              {MODALITIES.map((option) => {
                const selected = option === value;
                return (
                  <Pressable
                    key={option}
                    accessibilityRole="radio"
                    accessibilityState={{ selected }}
                    onPress={() => onChange(option)}
                    style={[styles.option, selected && styles.optionSelected]}
                  >
                    <Text style={[styles.optionText, selected && styles.optionTextSelected]}>
                      {option === 'repetitions' ? 'Reps' : option === 'duration' ? 'Time' : 'Distance'}
                    </Text>
                  </Pressable>
                );
              })}
            </View>
          </View>
        )}
      />

      <Controller
        control={control}
        name="notes"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Notes (optional)"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            multiline
            numberOfLines={3}
            placeholder="Coaching cues…"
            error={formState.errors.notes?.message}
          />
        )}
      />

      <Button label="Save exercise" onPress={onSubmit} busy={mutation.isPending} />
      <Button label="Cancel" variant="ghost" onPress={() => router.back()} />
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    group: { gap: t.space.sm },
    label: { color: t.color.mut2, fontFamily: fonts.semibold, fontSize: t.font.sm },
    // A two-up choice reads as a switch, so it is built as one: a recessed
    // trough with the chosen half raised out of it, matching Segmented.
    options: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      gap: 3,
      padding: 3,
    },
    option: {
      borderRadius: t.radius.sm,
      flex: 1,
      paddingVertical: t.space.md,
    },
    optionSelected: { backgroundColor: t.color.accent },
    optionText: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm + 0.5,
      textAlign: 'center',
    },
    optionTextSelected: { color: t.color.onAccent, fontFamily: fonts.bold },
  });

// Keeps the Exercise type referenced so a schema change surfaces here at compile time.
export type _CreatedExercise = Exercise;
