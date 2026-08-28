import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { createPlan, type Feature } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, ErrorBanner, Field, Screen, Section } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Add a plan.
 *
 * The price is typed in major units because that is how a gym thinks and says
 * it ("one twenty a month"), and converted to minor units here — the API and
 * the database only ever see integers, so nothing downstream can round.
 */
const schema = z.object({
  name: z.string().trim().min(1, 'Give it a name').max(80),
  description: z.string().trim().max(300).optional(),
  price: z
    .string()
    .trim()
    .regex(/^\d+(\.\d{1,2})?$/, 'A price like 120 or 49.50'),
});

type FormValues = z.infer<typeof schema>;

const INTERVALS = [
  { key: 'monthly' as const, label: 'Monthly', hint: 'Recurring membership' },
  { key: 'once' as const, label: 'One-off', hint: 'A drop-in or a single session' },
];

/**
 * What a plan can confer. Stated on the plan rather than inferred from its
 * price, because "the dearer one includes coaching" is a rule the gym sets, not
 * one the platform should guess.
 */
const GRANTS: { key: Feature; label: string; hint: string }[] = [
  { key: 'gym_access', label: 'Gym access', hint: 'Train here and log sessions' },
  {
    key: 'coached_programming',
    label: 'Coached programming',
    hint: 'An assigned coach and programme',
  },
  { key: 'class_credits', label: 'Class credits', hint: 'Draws on a class pack' },
];

export default function NewPlan() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));

  const [interval, setInterval] = useState<'monthly' | 'once'>('monthly');
  // Gym access is on by default because it is what nearly every plan sells;
  // it is still a toggle, so a coaching-only add-on can turn it off.
  const [grants, setGrants] = useState<Feature[]>(['gym_access']);
  const [error, setError] = useState<string | null>(null);

  const toggleGrant = (key: Feature) =>
    setGrants((held) => (held.includes(key) ? held.filter((g) => g !== key) : [...held, key]));

  const { control, handleSubmit, formState } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { name: '', description: '', price: '' },
  });

  const save = useMutation({
    mutationFn: (values: FormValues) =>
      createPlan(gymId!, {
        name: values.name,
        description: values.description,
        // Major → minor. Rounding here, once, on a value the user typed —
        // never on a value that has already been through arithmetic.
        priceMinor: Math.round(Number.parseFloat(values.price) * 100),
        currency: 'GBP',
        interval,
        grants,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['plans', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? e.message
          : e instanceof ApiError && e.code === 'resource.conflict'
            ? 'A plan with that name already exists in this gym.'
            : 'Could not create that plan. Please try again.',
      );
    },
  });

  const onSubmit = handleSubmit((values) => {
    // The server refuses this too; catching it here saves a round trip and
    // points at the section that is wrong.
    if (grants.length === 0) {
      setError('Choose at least one thing this plan includes.');
      return;
    }
    setError(null);
    save.mutate(values);
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'New plan' }} />

      <View style={s.intro}>
        <Text style={s.h1}>New plan</Text>
        <Text style={s.lede}>
          Price changes apply from the next cycle. Issued invoices are never rewritten.
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
            placeholder="Coaching"
            error={formState.errors.name?.message}
          />
        )}
      />

      <Controller
        control={control}
        name="price"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Price · £"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            keyboardType="decimal-pad"
            placeholder="120"
            inputStyle={s.price}
            error={formState.errors.price?.message}
          />
        )}
      />

      <View>
        <Section label="Billed" />
        <View style={s.intervals}>
          {INTERVALS.map((option) => {
            const on = interval === option.key;
            return (
              <Pressable
                key={option.key}
                onPress={() => setInterval(option.key)}
                accessibilityRole="radio"
                accessibilityState={{ selected: on }}
                accessibilityLabel={`${option.label}. ${option.hint}.`}
                style={[s.interval, on && s.intervalOn]}
              >
                <Text style={[s.intervalLabel, on && s.intervalLabelOn]}>{option.label}</Text>
                <Text style={s.intervalHint}>{option.hint}</Text>
              </Pressable>
            );
          })}
        </View>
      </View>

      <View>
        <Section label="What it unlocks" />
        <View style={s.grants}>
          {GRANTS.map((option) => {
            const on = grants.includes(option.key);
            return (
              <Pressable
                key={option.key}
                onPress={() => toggleGrant(option.key)}
                accessibilityRole="checkbox"
                accessibilityState={{ checked: on }}
                accessibilityLabel={`${option.label}. ${option.hint}.`}
                style={[s.grant, on && s.grantOn]}
              >
                <View style={[s.tick, on && s.tickOn]}>
                  {on ? <Text style={s.tickMark}>✓</Text> : null}
                </View>
                <View style={s.grantText}>
                  <Text style={s.grantLabel}>{option.label}</Text>
                  <Text style={s.grantHint}>{option.hint}</Text>
                </View>
              </Pressable>
            );
          })}
        </View>
      </View>

      <Controller
        control={control}
        name="description"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="How you describe it"
            value={value ?? ''}
            onChangeText={onChange}
            onBlur={onBlur}
            multiline
            placeholder="Floor access, open hours"
            error={formState.errors.description?.message}
          />
        )}
      />

      <Button
        label={save.isPending ? 'Creating…' : 'Create plan'}
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
    price: {
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      fontVariant: ['tabular-nums'],
    },
    intervals: { flexDirection: 'row', gap: 10, paddingTop: 14 },
    interval: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      flex: 1,
      gap: 3,
      paddingHorizontal: 14,
      paddingVertical: 13,
    },
    intervalOn: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
    },
    intervalLabel: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    intervalLabelOn: { color: t.color.ink },
    intervalHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    grants: { gap: 8, paddingTop: 14 },
    grant: {
      borderRadius: t.radius.md,
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      gap: 12,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    grantOn: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
    },
    grantText: { flex: 1, gap: 2 },
    grantLabel: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    grantHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    tick: {
      borderRadius: t.radius.md,
      alignItems: 'center',
      borderColor: t.color.line,
      borderWidth: t.border.ink,
      height: 22,
      justifyContent: 'center',
      width: 22,
    },
    tickOn: { backgroundColor: t.color.accent, borderColor: t.color.accent },
    tickMark: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      lineHeight: 16,
    },
  });
