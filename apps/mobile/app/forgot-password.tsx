import { zodResolver } from '@hookform/resolvers/zod';
import { Stack, useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { requestPasswordReset, resetPassword } from '@/api/gym';
import { ApiError } from '@/api/client';
import { Appear, Button, ErrorBanner, Field, Kicker, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

const askSchema = z.object({
  email: z.string().trim().min(1, 'Email is required').email('Enter a valid email'),
});

const resetSchema = z.object({
  token: z.string().trim().min(1, 'Paste the code from your email'),
  password: z.string().min(12, 'Use at least 12 characters'),
});

/**
 * Forgotten password — ask for a link, then use it.
 *
 * Before this, a forgotten password locked an account out permanently: there
 * was no reset of any kind (ADR-0029).
 *
 * **The confirmation is deliberately vague**, and must stay that way. "If that
 * address has an account, we've sent a link" is shown whether or not the
 * address is registered, because the server answers identically either way —
 * anything more specific turns this screen into a way to discover who trains
 * at the gym. Someone will eventually want to "improve" it to say "no account
 * found"; that is the change this comment exists to stop.
 *
 * Both steps live on one screen because they are one task, and because the
 * code arrives by email on the same device — bouncing to a second screen would
 * lose the half-typed address on the way back.
 */
export default function ForgotPassword() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const [sent, setSent] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const ask = useForm({ resolver: zodResolver(askSchema), defaultValues: { email: '' } });
  const finish = useForm({
    resolver: zodResolver(resetSchema),
    defaultValues: { token: '', password: '' },
  });

  const onAsk = ask.handleSubmit(async (values) => {
    setError(null);
    setBusy(true);
    try {
      await requestPasswordReset(values.email);
      setSent(true);
    } catch {
      setError('Could not send that right now. Please try again.');
    } finally {
      setBusy(false);
    }
  });

  const onReset = finish.handleSubmit(async (values) => {
    setError(null);
    setBusy(true);
    try {
      await resetPassword(values.token, values.password);
      setDone(true);
    } catch (e) {
      setError(
        e instanceof ApiError && e.code === 'auth.invalid_token'
          ? 'That code has expired or has already been used. Ask for a new one.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? 'Use at least 12 characters.'
            : 'Could not reset your password. Please try again.',
      );
    } finally {
      setBusy(false);
    }
  });

  if (done) {
    return (
      <Screen scroll edges={['bottom']}>
        <Stack.Screen options={{ title: '', headerTransparent: true }} />
        <View style={s.header}>
          <Kicker tone="accent">Done</Kicker>
          <Text style={s.h1}>Password changed</Text>
          <Text style={s.lede}>
            Every device has been signed out, including this one. Sign in again with your new
            password.
          </Text>
          </View>
        <Button label="Back to sign in" onPress={() => router.replace('/sign-in')} />
      </Screen>
    );
  }

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: '', headerTransparent: true }} />

      <Appear>
        <View style={s.header}>
          <Kicker tone="accent">Account</Kicker>
          <Text style={s.h1}>Forgotten{'\n'}password</Text>
          <Text style={s.lede}>
            {sent
              ? 'Enter the code from the email, and choose something new.'
              : 'Enter your email and we will send you a link to set a new one.'}
          </Text>
          </View>
      </Appear>

      {error ? <ErrorBanner message={error} /> : null}

      {sent ? (
        <>
          {/*
            Says nothing about whether the address is registered. See the
            module doc — this wording is a security property, not a placeholder.
          */}
          <Text style={s.notice}>
            If that address has an account, a link is on its way. It works once, and expires
            in an hour.
          </Text>

          <Controller
            control={finish.control}
            name="token"
            render={({ field: { onChange, onBlur, value } }) => (
              <Field
                label="Code from the email"
                value={value}
                onChangeText={onChange}
                onBlur={onBlur}
                autoCapitalize="none"
                autoCorrect={false}
                autoFocus
                error={finish.formState.errors.token?.message}
              />
            )}
          />

          <Controller
            control={finish.control}
            name="password"
            render={({ field: { onChange, onBlur, value } }) => (
              <Field
                label="New password"
                value={value}
                onChangeText={onChange}
                onBlur={onBlur}
                secureTextEntry
                autoComplete="new-password"
                placeholder="12 characters minimum"
                error={finish.formState.errors.password?.message}
                onSubmitEditing={onReset}
                returnKeyType="go"
              />
            )}
          />

          <Button label="Set new password" onPress={onReset} busy={busy} />
          <Text style={s.footerText}>
            Didn&apos;t get it?{' '}
            <Text style={s.footerLink} onPress={() => setSent(false)}>
              Try a different address
            </Text>
          </Text>
        </>
      ) : (
        <>
          <Controller
            control={ask.control}
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
                autoFocus
                error={ask.formState.errors.email?.message}
                onSubmitEditing={onAsk}
                returnKeyType="go"
              />
            )}
          />
          <Button label="Send me a link" onPress={onAsk} busy={busy} />
          <Text style={s.footerText}>
            Remembered it?{' '}
            <Text style={s.footerLink} onPress={() => router.back()}>
              Sign in
            </Text>
          </Text>
        </>
      )}
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    header: { gap: 6, paddingTop: t.space.xl },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 32,
      marginTop: 4,
    },
    lede: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5, lineHeight: 20 },
    notice: {
      backgroundColor: t.color.accentHi,
      borderRadius: t.radius.md,
      color: t.color.accentBadgeInk,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    footerText: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5 },
    footerLink: { color: t.color.accentDeep, fontFamily: fonts.bold },
  });
