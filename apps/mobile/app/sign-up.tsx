import { zodResolver } from '@hookform/resolvers/zod';
import { useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { signUp } from '@/api/gym';
import { Appear, Button, ErrorBanner, Field, Kicker, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Account creation only — three fields, no role question.
 *
 * Asking "are you an owner, a trainer or a member?" here would be a false
 * choice: a person can hold several capacities at once — owner, trainer AND
 * member — and gain more over time by invitation (ADR-0014). Joining the gym
 * comes after, and stays reversible.
 */
const schema = z.object({
  displayName: z.string().trim().min(1, 'Your name is required').max(50),
  email: z.string().trim().min(1, 'Email is required').email('Enter a valid email'),
  password: z.string().min(12, 'Use at least 12 characters'),
});

type FormValues = z.infer<typeof schema>;

export default function SignUp() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const { control, handleSubmit, formState } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { displayName: '', email: '', password: '' },
  });

  const onSubmit = handleSubmit(async (values) => {
    setSubmitError(null);
    setBusy(true);
    try {
      await signUp(values);
      // No navigation: the account now exists with no gym, and the root
      // layout's guard moves us into onboarding automatically.
    } catch (error) {
      setSubmitError(
        error instanceof ApiError && error.code === 'resource.conflict'
          ? 'That email is already registered.'
          : 'Could not create your account. Please try again.',
      );
      setBusy(false);
    }
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Appear>
        <View style={s.header}>
          <Kicker tone="accent">Gym Platform</Kicker>
          <Text style={s.h1}>Create your account</Text>
          <Text style={s.lede}>One identity. Your standing at the gym attaches to it.</Text>
          </View>
      </Appear>

      {submitError ? <ErrorBanner message={submitError} /> : null}

      <Controller
        control={control}
        name="displayName"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Display name"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            autoComplete="name"
            placeholder="Sam Ortiz"
            error={formState.errors.displayName?.message}
          />
        )}
      />

      <Controller
        control={control}
        name="email"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Email"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            autoCapitalize="none"
            autoComplete="email"
            keyboardType="email-address"
            placeholder="you@example.com"
            error={formState.errors.email?.message}
          />
        )}
      />

      <Controller
        control={control}
        name="password"
        render={({ field: { onChange, onBlur, value } }) => (
          <Field
            label="Password"
            value={value}
            onChangeText={onChange}
            onBlur={onBlur}
            secureTextEntry
            autoComplete="new-password"
            placeholder="12 characters minimum"
            error={formState.errors.password?.message}
          />
        )}
      />

      <Button label="Create account" onPress={onSubmit} busy={busy} />
      <Text style={s.footerText}>
        Already training here?{' '}
        <Text style={s.footerLink} onPress={() => router.back()}>
          Sign in
        </Text>
      </Text>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    header: { gap: 6 },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      marginTop: 4,
    },
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    footerText: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
    },
    footerLink: { color: t.color.accentDeep, fontFamily: fonts.bold },
  });
