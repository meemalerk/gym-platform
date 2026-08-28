import { useMutation, useQueryClient } from '@tanstack/react-query';
import * as Clipboard from 'expo-clipboard';
import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { createStaff, type CreatedStaff } from '@/api/gym';
import { can, capacityLabel, useActiveMembership } from '@/session/store';
import {
  Button,
  Callout,
  Card,
  ErrorBanner,
  Field,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Create a staff account (ADR-0032).
 *
 * The fast way to set a gym up. Everything else here makes staff out of people
 * who are already members; this makes the person and their standing in one go,
 * for the owner who has just hired a trainer and does not want to talk them
 * through signing up first.
 *
 * The screen has two halves and only ever shows one. Before: a form. After: a
 * password that exists exactly once, and a deliberate "I have written this
 * down" before the screen will let go of it — because the server does not keep
 * a copy and cannot show it again.
 */

/** The ladder, senior first, with what each rung actually lets somebody do. */
const RUNGS: { key: string; unlocks: string }[] = [
  { key: 'owner', unlocks: 'Run the gym: billing, settings, the catalogue, who is staff' },
  { key: 'trainer', unlocks: 'Coach clients and assign them published programmes' },
  { key: 'member', unlocks: 'Train here, log workouts, be coached' },
];

export default function NewStaff() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];
  const mayTouchOwner = can.setOwner(capacities);

  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  // Trainer and member together is the common case by a distance: a coach at a
  // gym almost always trains there too, and starting from the answer people
  // want saves a tap without hiding the choice.
  const [chosen, setChosen] = useState<string[]>(['trainer', 'member']);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatedStaff | null>(null);
  const [copied, setCopied] = useState(false);

  const create = useMutation({
    mutationFn: () => createStaff(active!.gymId, { email, displayName: name, capacities: chosen }),
    onSuccess: (result) => {
      setError(null);
      setCreated(result);
      void queryClient.invalidateQueries({ queryKey: ['members', active?.gymId] });
      void queryClient.invalidateQueries({ queryKey: ['trainers', active?.gymId] });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'That address already has an account. Ask them to join this gym, then set their standing from the roster.'
          : e instanceof ApiError && e.code === 'auth.forbidden'
            ? mayTouchOwner
              ? 'That was refused.'
              : 'Only an owner can create an owner.'
            : e instanceof ApiError && e.code === 'request.invalid'
              ? e.message
              : 'Could not create that account. Please try again.',
      );
    },
  });

  // ---- after: the password, once ----------------------------------------

  if (created) {
    return (
      <Screen scroll>
        <Stack.Screen options={{ title: 'Account created', headerBackVisible: false }} />

        <View style={s.intro}>
          <Text style={s.h1}>{created.display_name} is set up</Text>
          <Text style={s.lede}>
            They sign in with their email and this password, then change it from their own
            account.
          </Text>
        </View>

        <Card>
          <Section label="Their sign-in" />
          <Text style={s.fieldLabel}>Email</Text>
          <Text style={s.value} selectable>
            {created.email}
          </Text>

          <Text style={s.fieldLabel}>Password</Text>
          <Pressable
            onPress={() => {
              void Clipboard.setStringAsync(created.temporary_password);
              setCopied(true);
            }}
            accessibilityRole="button"
            accessibilityLabel="Copy the password"
            style={s.secret}
          >
            <Text style={s.secretText} selectable>
              {created.temporary_password}
            </Text>
            <Text style={s.copy}>{copied ? 'Copied' : 'Copy'}</Text>
          </Pressable>
        </Card>

        <Callout tone="warn">
          This is the only time this password is shown. It is not stored anywhere it can be
          read back — if it is lost, delete nothing: just have them use it once and change it,
          or create the account again under a different address.
        </Callout>

        <Button label="Done" onPress={() => router.back()} />
      </Screen>
    );
  }

  // ---- before: the form --------------------------------------------------

  const ready =
    name.trim().length > 0 && email.trim().includes('@') && chosen.length > 0 && !create.isPending;

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'New staff account' }} />

      <View style={s.intro}>
        <Text style={s.h1}>New staff account</Text>
        <Text style={s.lede}>
          Creates the account and their standing together, and gives you a password to hand
          over. Use this for somebody who does not have an account yet.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <Card>
        <Field
          label="Name"
          value={name}
          onChangeText={setName}
          placeholder="Tariq Trainer"
          autoComplete="name"
        />
        <Field
          label="Email"
          value={email}
          onChangeText={setEmail}
          placeholder="tariq@example.com"
          autoCapitalize="none"
          autoCorrect={false}
          keyboardType="email-address"
          hint="They sign in with this. It cannot be changed here afterwards."
        />
      </Card>

      <View>
        <Section label="What they hold" />
        <Card padded={false}>
          {RUNGS.map((rung, i) => {
            const on = chosen.includes(rung.key);
            const locked = rung.key === 'owner' && !mayTouchOwner;
            return (
              <Pressable
                key={rung.key}
                disabled={locked}
                onPress={() => {
                  setError(null);
                  setChosen(
                    on ? chosen.filter((c) => c !== rung.key) : [...chosen, rung.key],
                  );
                }}
                accessibilityRole="checkbox"
                accessibilityState={{ checked: on, disabled: locked }}
                accessibilityLabel={`${capacityLabel(rung.key)}. ${rung.unlocks}.`}
                style={[s.rung, i === RUNGS.length - 1 && s.rungLast, locked && s.rungLocked]}
              >
                <View style={[s.tick, on && s.tickOn]}>
                  {on ? <Text style={s.tickMark}>✓</Text> : null}
                </View>
                <View style={s.rungBody}>
                  <Text style={s.rungLabel}>{capacityLabel(rung.key)}</Text>
                  <Text style={s.rungHint}>{rung.unlocks}</Text>
                </View>
              </Pressable>
            );
          })}
        </Card>
      </View>

      {!mayTouchOwner ? (
        <Callout>Only an owner can create another owner. Everything else is yours to set.</Callout>
      ) : null}

      <Button
        label={create.isPending ? 'Creating…' : 'Create the account'}
        disabled={!ready}
        onPress={() => {
          setError(null);
          create.mutate();
        }}
      />

      <Text style={s.footnote} onPress={() => router.back()}>
        Already have an account? They join through the door and you set their standing from the
        roster — that way they choose their own password.
      </Text>
      <View style={{ height: t.space.xl }} />
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
      lineHeight: 30,
    },
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },

    fieldLabel: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    value: { color: t.color.ink, fontFamily: fonts.medium, fontSize: t.font.md },

    // The password gets the display face and generous tracking: it is read
    // aloud or copied by hand, and an l/1 confusion costs a support call.
    secret: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      gap: t.space.md,
      paddingHorizontal: 14,
      paddingVertical: 14,
    },
    secretText: {
      color: t.color.ink,
      flex: 1,
      fontFamily: 'monospace',
      fontSize: t.font.lg,
      letterSpacing: 1,
    },
    copy: { color: t.color.accentDeep, fontFamily: fonts.bold, fontSize: t.font.sm },

    rung: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: t.space.md,
    },
    rungLast: { borderBottomWidth: 0 },
    rungLocked: { opacity: 0.45 },
    rungBody: { flex: 1, gap: 2 },
    rungLabel: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    rungHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },

    tick: {
      alignItems: 'center',
      borderColor: t.color.rule,
      borderRadius: t.radius.xs,
      borderWidth: t.border.ink,
      height: 24,
      justifyContent: 'center',
      width: 24,
    },
    tickOn: { backgroundColor: t.color.accent, borderColor: t.color.accent },
    tickMark: { color: t.color.onAccent, fontFamily: fonts.bold, fontSize: t.font.sm },

    footnote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
  });
