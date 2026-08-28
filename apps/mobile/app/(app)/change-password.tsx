import { zodResolver } from '@hookform/resolvers/zod';
import { Stack } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { changePassword } from '@/api/gym';
import { Button, Callout, Card, ErrorBanner, Field, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Change your own password (ADR-0032).
 *
 * Here because staff accounts start on a password somebody else generated and
 * read out (see `new-staff.tsx`). Without this the only way off it is the
 * reset link, which goes through an email this deployment does not send — so
 * a trainer would be stuck on a password their manager remembers.
 *
 * On success the server revokes every session, so the app signs out. That is
 * not a side effect to hide: it is the point, and the screen says so before
 * the button rather than surprising somebody back at the sign-in screen.
 */
const schema = z
  .object({
    current: z.string().min(1, 'Enter your current password'),
    next: z.string().min(12, 'Use at least 12 characters'),
    confirm: z.string().min(1, 'Type it again'),
  })
  .refine((v) => v.next === v.confirm, {
    path: ['confirm'],
    message: 'Those do not match',
  });

type FormValues = z.infer<typeof schema>;

export default function ChangePassword() {
  const s = useStyles(styleFactory);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const { control, handleSubmit, formState } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { current: '', next: '', confirm: '' },
  });

  const onSubmit = handleSubmit(async (values) => {
    setError(null);
    setBusy(true);
    try {
      // Signs out on success — the root layout's guard takes it from here, so
      // there is deliberately no navigation call after this.
      await changePassword(values.current, values.next);
    } catch (e) {
      setError(
        e instanceof ApiError && e.code === 'auth.unauthenticated'
          ? 'That is not your current password.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : 'Could not change it. Please try again.',
      );
      setBusy(false);
    }
  });

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Change password' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Change password</Text>
        <Text style={s.lede}>
          If somebody set this account up for you, this is how you stop using the password they
          chose.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <Card>
        <Controller
          control={control}
          name="current"
          render={({ field: { onChange, onBlur, value } }) => (
            <Field
              label="Current password"
              value={value}
              onChangeText={onChange}
              onBlur={onBlur}
              secureTextEntry
              autoComplete="current-password"
              error={formState.errors.current?.message}
            />
          )}
        />
        <Controller
          control={control}
          name="next"
          render={({ field: { onChange, onBlur, value } }) => (
            <Field
              label="New password"
              value={value}
              onChangeText={onChange}
              onBlur={onBlur}
              secureTextEntry
              autoComplete="new-password"
              placeholder="12 characters minimum"
              error={formState.errors.next?.message}
            />
          )}
        />
        <Controller
          control={control}
          name="confirm"
          render={({ field: { onChange, onBlur, value } }) => (
            <Field
              label="New password again"
              value={value}
              onChangeText={onChange}
              onBlur={onBlur}
              secureTextEntry
              autoComplete="new-password"
              error={formState.errors.confirm?.message}
              onSubmitEditing={onSubmit}
              returnKeyType="go"
            />
          )}
        />
      </Card>

      <Callout>
        Changing it signs you out everywhere, including here — sign back in with the new one.
        That is deliberate: anyone else holding a session on this account loses it.
      </Callout>

      <Button label={busy ? 'Changing…' : 'Change password'} onPress={onSubmit} busy={busy} />
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    intro: { gap: 6 },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
    },
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
  });
