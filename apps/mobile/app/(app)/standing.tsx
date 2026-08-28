import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { listMembers, setCapacities } from '@/api/gym';
import { can, capacityLabel, useActiveMembership, useSession } from '@/session/store';
import {
  Button,
  Callout,
  Card,
  Centered,
  ErrorBanner,
  InitialsSquare,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * What somebody holds at this gym (ADR-0031).
 *
 * This screen is what invitations became. Before, a manager typed an address,
 * the server minted a token, and somebody had to receive an email the
 * deployment never sent. Now everyone walks in as a member and this is where
 * they become a trainer or an owner.
 *
 * The list is the capability **ladder**, in order, with what each rung
 * actually unlocks written next to it — because a role name is not
 * self-explanatory to somebody opening this for the first time, and the
 * difference between trainer and owner is the difference between coaching the
 * clients you are given and deciding what the gym sells.
 *
 * Three rungs since ADR-0036, down from five: `admin` and `head_coach` were
 * only ever "slightly less than owner", and the picker offering them was the
 * main place they existed at all.
 */

/** The ladder, senior first, with what each rung actually lets somebody do. */
const RUNGS: { key: string; unlocks: string }[] = [
  { key: 'owner', unlocks: 'Run the gym: billing, settings, the catalogue, who is staff' },
  { key: 'trainer', unlocks: 'Coach clients and assign them published programmes' },
  { key: 'member', unlocks: 'Train here, log workouts, be coached' },
];

export default function Standing() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const { id, name } = useLocalSearchParams<{ id: string; name?: string }>();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const myId = useSession((st) => st.user?.id);
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];

  const mayEdit = can.setCapacities(capacities);
  const mayTouchOwner = can.setOwner(capacities);

  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<string[] | null>(null);

  const roster = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId),
  });

  const person = (roster.data ?? []).find((m) => m.user_id === id);
  const current = useMemo(() => person?.capacities ?? [], [person]);
  const chosen = draft ?? current;

  const save = useMutation({
    mutationFn: () => setCapacities(gymId!, id, chosen),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['members', gymId] });
      // A trainer who just stopped being one should disappear from pickers.
      void queryClient.invalidateQueries({ queryKey: ['trainers', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'auth.forbidden'
          ? mayTouchOwner
            ? 'That change was refused.'
            : 'Only an owner can grant or remove owner.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : 'Could not save that. Please try again.',
      );
    },
  });

  if (roster.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  if (!person) {
    return (
      <Screen>
        <Stack.Screen options={{ title: 'Standing' }} />
        <ErrorBanner message="That person is not in this gym." />
      </Screen>
    );
  }

  const displayName = person.display_name || name || 'This person';
  const changed = JSON.stringify([...chosen].sort()) !== JSON.stringify([...current].sort());
  const isSelf = person.user_id === myId;
  // Mirrors the server's rule so the button is not offered into a refusal:
  // the last owner cannot step down, and the client cannot tell how many
  // owners there are — but it can tell that *this* is you giving yours up.
  const losingOwnOwner = isSelf && current.includes('owner') && !chosen.includes('owner');

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Standing' }} />

      {error ? <ErrorBanner message={error} /> : null}

      <Card>
        <View style={s.head}>
          <InitialsSquare name={displayName} size={46} />
          <View style={s.headBody}>
            <Text style={s.name} numberOfLines={2}>
              {displayName}
            </Text>
            <Text style={s.meta}>
              {current.length > 0
                ? `Currently ${current.map(capacityLabel).join(', ').toLowerCase()}`
                : 'Not a member here'}
            </Text>
          </View>
        </View>
      </Card>

      <View>
        <Section label="What they hold" meta={mayEdit ? undefined : 'read only'} />
        <Card padded={false}>
          {RUNGS.map((rung, i) => {
            const on = chosen.includes(rung.key);
            // Owner is the one rung an admin can see and not move.
            const locked = !mayEdit || (rung.key === 'owner' && !mayTouchOwner);
            return (
              <Pressable
                key={rung.key}
                disabled={locked}
                onPress={() => {
                  setError(null);
                  setDraft(
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

      {!mayTouchOwner && mayEdit ? (
        <Callout>
          Only an owner can grant or remove owner. Everything else is yours to set.
        </Callout>
      ) : null}

      {losingOwnOwner ? (
        <Callout tone="warn">
          You are giving up your own owner standing. If you are the only owner this will be
          refused — appoint another owner first.
        </Callout>
      ) : null}

      {mayEdit ? (
        <Button
          label={save.isPending ? 'Saving…' : 'Save standing'}
          disabled={!changed || chosen.length === 0 || save.isPending}
          onPress={() => {
            setError(null);
            save.mutate();
          }}
        />
      ) : null}

      {chosen.length === 0 ? (
        <Text style={s.footnote}>
          Somebody has to hold something. To take a person out of the gym entirely, ask at the
          desk — it is not something this screen does, because it would take their history
          with it.
        </Text>
      ) : null}
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    head: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    headBody: { flex: 1, gap: 3 },
    name: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
    },
    meta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.sm },

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
