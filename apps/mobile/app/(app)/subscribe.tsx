import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  listMembers,
  listPlans,
  listSubscriptions,
  subscribeMember,
  type GymMember,
  type Plan,
} from '@/api/gym';
import { summariseGrants } from '@/features/billing/entitlements';
import { useSession } from '@/session/store';
import {
  Button,
  Centered,
  EmptyState,
  ErrorBanner,
  Field,
  InitialsSquare,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

function localToday(): string {
  const d = new Date();
  const p = (n: number) => `${n}`.padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/**
 * Put a member on a plan.
 *
 * Subscribing bills immediately — the first invoice is issued in the same
 * transaction — so this screen says so before you commit rather than after the
 * member asks what the email was.
 *
 * The subscription copies the plan's price at signup. Changing the plan's price
 * later does not move anyone already on it.
 */
export default function Subscribe() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));

  const [member, setMember] = useState<GymMember | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [startedOn, setStartedOn] = useState(localToday());
  const [error, setError] = useState<string | null>(null);

  const members = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId),
  });
  const plans = useQuery({
    queryKey: ['plans', gymId],
    queryFn: () => listPlans(gymId!),
    enabled: Boolean(gymId),
  });
  const subscriptions = useQuery({
    queryKey: ['subscriptions', gymId],
    queryFn: () => listSubscriptions(gymId!),
    enabled: Boolean(gymId),
  });

  // Already paying — subscribing twice is refused by the server, so do not
  // offer it and then explain the refusal.
  const alreadyPaying = useMemo(
    () =>
      new Set(
        (subscriptions.data ?? []).filter((x) => x.is_active).map((x) => x.member_id),
      ),
    [subscriptions.data],
  );

  const candidates = (members.data ?? []).filter((m) => m.capacities.includes('member'));
  const offered = (plans.data ?? []).filter((p) => p.is_offered);

  const save = useMutation({
    mutationFn: () =>
      subscribeMember(gymId!, {
        memberId: member!.user_id,
        planId: plan!.id,
        startedOn,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['subscriptions', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['invoices', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['plans', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'They already have an active subscription.'
          : e instanceof ApiError && e.code === 'request.invalid'
            ? e.message
            : 'Could not set that up. Please try again.',
      );
    },
  });

  const dateOk = /^\d{4}-\d{2}-\d{2}$/.test(startedOn);
  const loading = members.isLoading || plans.isLoading;

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Start a membership' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Start a membership</Text>
        <Text style={s.lede}>
          The first invoice is raised straight away. The price is copied now — changing the plan
          later will not move anyone already on it.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {loading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : offered.length === 0 ? (
        <EmptyState
          glyph="◫"
          title="Nothing on sale"
          hint="Add a plan first — a membership needs something to be a membership of."
        />
      ) : candidates.length === 0 ? (
        <EmptyState
          glyph="○"
          title="No members yet"
          hint="Invite someone as a member first."
        />
      ) : (
        <>
          <View>
            <Section label="Who" count={candidates.length} />
            {candidates.map((m, i, arr) => {
              const on = member?.user_id === m.user_id;
              const paying = alreadyPaying.has(m.user_id);
              return (
                <Pressable
                  key={m.user_id}
                  onPress={() => !paying && setMember(m)}
                  disabled={paying}
                  accessibilityRole="radio"
                  accessibilityState={{ selected: on, disabled: paying }}
                  accessibilityLabel={paying ? `${m.display_name}, already paying` : m.display_name}
                  style={[s.row, i === arr.length - 1 && s.rowLast, on && s.rowOn]}
                >
                  <InitialsSquare name={m.display_name} />
                  <View style={s.rowBody}>
                    <Text style={s.rowName}>{m.display_name}</Text>
                    {paying ? <Text style={s.rowMeta}>Already on a plan</Text> : null}
                  </View>
                  {on ? <Text style={s.tick}>✓</Text> : null}
                </Pressable>
              );
            })}
          </View>

          <View>
            <Section label="On which plan" count={offered.length} />
            {offered.map((p, i, arr) => {
              const on = plan?.id === p.id;
              return (
                <Pressable
                  key={p.id}
                  onPress={() => setPlan(p)}
                  accessibilityRole="radio"
                  accessibilityState={{ selected: on }}
                  accessibilityLabel={`${p.name}, ${p.price_label} ${
                    p.interval === 'monthly' ? 'a month' : 'once'
                  }, includes ${summariseGrants(p.grants)}`}
                  style={[s.row, i === arr.length - 1 && s.rowLast, on && s.rowOn]}
                >
                  <View style={s.rowBody}>
                    <Text style={s.rowName}>{p.name}</Text>
                    <Text style={s.rowMeta}>
                      {`${p.price_label}${p.interval === 'monthly' ? '/mo' : ' once'} · ${summariseGrants(p.grants)}`}
                    </Text>
                  </View>
                  {on ? <Text style={s.tick}>✓</Text> : null}
                </Pressable>
              );
            })}
          </View>

          <Field
            label="Starts on"
            value={startedOn}
            onChangeText={setStartedOn}
            placeholder="YYYY-MM-DD"
            autoCapitalize="none"
            error={dateOk ? undefined : 'A date like 2026-08-01'}
          />

          {member && plan ? (
            <View style={s.summary}>
              <Text style={s.summaryLabel}>Will bill</Text>
              <Text style={s.summaryValue}>{plan.price_label}</Text>
              <Text style={s.summaryHint}>
                {`${member.display_name} · ${plan.name}${
                  plan.interval === 'monthly' ? ' · every month' : ' · one-off'
                }`}
              </Text>
            </View>
          ) : null}

          <Button
            label={save.isPending ? 'Setting up…' : 'Start membership'}
            disabled={save.isPending || !member || !plan || !dateOk}
            onPress={() => {
              setError(null);
              save.mutate();
            }}
          />
        </>
      )}
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
    row: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: 12,
      paddingHorizontal: 6,
      paddingVertical: 11,
    },
    rowLast: { borderBottomWidth: 0 },
    rowOn: { backgroundColor: t.color.accentHi },
    rowBody: { flex: 1, gap: 2 },
    rowName: { color: t.color.ink, fontFamily: fonts.bold, fontSize: t.font.md },
    rowMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    tick: { color: t.color.accentDeep, fontFamily: fonts.bold, fontSize: t.font.lg },
    summary: {
      borderRadius: t.radius.md,
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.ink,
      gap: 3,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    summaryLabel: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.xs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    summaryValue: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      fontVariant: ['tabular-nums'],
    },
    summaryHint: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.xs },
  });
