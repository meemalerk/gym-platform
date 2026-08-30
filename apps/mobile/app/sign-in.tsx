import { zodResolver } from '@hookform/resolvers/zod';
import { useRouter } from 'expo-router';
import { Controller, useForm } from 'react-hook-form';
import { useState } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { z } from 'zod';

import { ApiError } from '@/api/client';
import { signIn } from '@/api/gym';
import { API_URL, isLoopbackApi } from '@/config';
import { DEMO_ACCOUNTS, DEMO_PASSWORD, SHOW_DEMO_ACCOUNTS } from '@/dev/demo-accounts';
import { Appear, Button, ErrorBanner, Field, Touchable } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

const schema = z.object({
  email: z.string().trim().min(1, 'Email is required').email('Enter a valid email'),
  password: z.string().min(1, 'Password is required'),
});

type FormValues = z.infer<typeof schema>;

export default function SignIn() {
  const s = useStyles(styleFactory);
  const insets = useSafeAreaInsets();
  const router = useRouter();
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const { control, handleSubmit, formState, setValue } = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { email: '', password: '' },
  });

  /** One path in, whether the credentials were typed or tapped. */
  const submit = async (values: FormValues) => {
    setSubmitError(null);
    setBusy(true);
    try {
      await signIn(values);
      // Back to the entry point rather than a named screen: whether this account
      // lands in the app or in onboarding depends on whether it has a gym, and
      // index.tsx is the one place that decides.
      router.replace('/');
    } catch (error) {
      setSubmitError(
        error instanceof ApiError && error.code === 'auth.unauthenticated'
          ? 'Incorrect email or password.'
          : // A distinct code, so this does not read as "wrong password" —
            // telling someone to keep trying when trying is the problem is the
            // worst possible advice here.
            error instanceof ApiError && error.code === 'auth.too_many_attempts'
            ? 'Too many attempts. Wait a few minutes and try again, or reset your password.'
            : error instanceof ApiError && error.code === 'network.unreachable'
              ? // Nothing answered, so nothing judged these credentials. Say so,
                // and on a device name the likeliest cause: a build pointed at
                // `localhost` is pointed at the phone itself, where no API runs.
                isLoopbackApi
                ? `Cannot reach the API at ${API_URL}. On a phone that address is the phone — set EXPO_PUBLIC_API_URL to this machine's LAN address.`
                : `Cannot reach the API at ${API_URL}. Check it is running, then try again.`
              : 'Could not sign in. Please try again.',
      );
      setBusy(false);
    }
  };

  const onSubmit = handleSubmit(submit);

  /** Fill the form as well as signing in, so it is obvious who you became. */
  const signInAs = (email: string) => {
    setValue('email', email);
    setValue('password', DEMO_PASSWORD);
    void submit({ email, password: DEMO_PASSWORD });
  };

  return (
    <ScrollView
      style={s.flex}
      contentContainerStyle={{ paddingBottom: insets.bottom + 24 }}
      keyboardShouldPersistTaps="handled"
      showsVerticalScrollIndicator={false}
    >
      {/*
        The poster: a full-bleed accent panel with its bottom corners rounded,
        and the form card riding up over the join. That overlap is the one
        piece of stagecraft on the screen — it makes the first thing you see
        read as one object rather than as a coloured header stuck above a
        form, and it costs a negative margin.
      */}
      <View style={[s.poster, { paddingTop: insets.top + 36 }]}>
        <Appear>
          <Text style={s.posterKicker}>Gym Platform</Text>
        </Appear>
        <Appear index={1}>
          <Text style={s.posterTitle}>Train.{'\n'}Coach.{'\n'}Run the floor.</Text>
        </Appear>
      </View>

      <View style={s.body}>
        <Appear index={2}>
          <View style={s.card}>
            <Text style={s.h1}>Sign in</Text>

            {submitError ? <ErrorBanner message={submitError} /> : null}

            <View style={s.form}>
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
                  autoComplete="current-password"
                  placeholder="••••••••••••"
                  error={formState.errors.password?.message}
                  onSubmitEditing={onSubmit}
                  returnKeyType="go"
                />
              )}
            />

              <Button label="Sign in" onPress={onSubmit} busy={busy} />
              {/* A forgotten password used to be permanent — there was no reset
                  of any kind (ADR-0029). This is the way out. */}
              <Text style={s.forgot} onPress={() => router.push('/forgot-password')}>
                Forgotten your password?
              </Text>
            </View>
          </View>
        </Appear>

        <Appear index={4}>
          <Text style={s.footerText}>
            New here?{' '}
            <Text style={s.footerLink} onPress={() => router.push('/sign-up')}>
              Create an account
            </Text>{' '}
            — then join the gym with a code, or on your own.
          </Text>
        </Appear>

        {/*
          Development, plus the browser demo that opts in at build time. Both
          conditions are compile-time, so a shipped app dead-strips this whole
          branch — and the form above stays the way in for a real account.
          See SHOW_DEMO_ACCOUNTS for why the second door exists.
        */}
        {SHOW_DEMO_ACCOUNTS ? (
          <Appear index={5}>
            <View style={s.demo}>
              <View style={s.demoHead}>
                <Text style={s.demoTitle}>Demo accounts</Text>
                {/* Accurate in both contexts: a developer needs the warning,
                    a demo viewer needs to know these are not real people. */}
                <Text style={s.demoTag}>{__DEV__ ? 'Dev only' : 'Demo'}</Text>
              </View>
              {DEMO_ACCOUNTS.map((account, i) => (
                <Touchable
                  key={account.email}
                  onPress={() => signInAs(account.email)}
                  disabled={busy}
                  accessibilityLabel={`Sign in as ${account.label}. ${account.hint}.`}
                  style={[s.demoRow, i === DEMO_ACCOUNTS.length - 1 && s.demoRowLast]}
                >
                  <View style={s.demoBody}>
                    <Text style={s.demoLabel}>{account.label}</Text>
                    <Text style={s.demoHint} numberOfLines={1}>
                      {account.hint}
                    </Text>
                  </View>
                  <Text style={s.demoGo}>Sign in</Text>
                </Touchable>
              ))}
            </View>
          </Appear>
        ) : null}
      </View>
    </ScrollView>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    flex: { backgroundColor: t.color.surface, flex: 1 },
    poster: {
      backgroundColor: t.color.accent,
      borderBottomLeftRadius: t.radius.xl,
      borderBottomRightRadius: t.radius.xl,
      paddingBottom: t.space.huge,
      paddingHorizontal: t.space.gutter,
    },
    posterKicker: {
      // onAccent, not a translucent white: the poster sits on the accent,
      // and in the dark scheme onAccent is near-BLACK. Hierarchy against
      // the title comes from size and case, which is where it belongs —
      // light's accent is dark enough that a softened tone cannot clear AA.
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    posterTitle: {
      color: t.color.onAccent,
      fontFamily: fonts.displayHeavy,
      fontSize: 40,
      letterSpacing: t.tracking.display,
      lineHeight: 41,
      marginTop: t.space.md,
    },
    body: { gap: t.space.lg, paddingHorizontal: t.space.gutter },
    card: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.xl,
      borderWidth: t.border.hair,
      gap: t.space.lg,
      // Rides up over the poster's rounded edge. See the note at the markup.
      marginTop: -t.space.xxl,
      padding: t.space.xl,
      ...t.elevation(3),
    },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
    },
    form: { gap: t.space.lg },
    footerText: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
      textAlign: 'center',
    },
    footerLink: { color: t.color.accentDeep, fontFamily: fonts.bold },
    forgot: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      // Full-width tap target under the button, rather than a cramped link.
      paddingVertical: t.space.sm,
      textAlign: 'center',
    },

    demo: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.lg,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.sm,
    },
    demoHead: {
      alignItems: 'center',
      flexDirection: 'row',
      justifyContent: 'space-between',
      paddingBottom: t.space.xs,
      paddingTop: t.space.sm,
    },
    demoTitle: {
      color: t.color.mut2,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    demoTag: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
    demoRow: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      // Thumb-sized: this is the control used most often in development.
      minHeight: 56,
      paddingVertical: t.space.sm,
    },
    demoRowLast: { borderBottomWidth: 0 },
    demoBody: { flex: 1, gap: 2 },
    demoLabel: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    demoHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    demoGo: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },
  });
