import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { listPlans, listSubscriptions, subscribeMember, type Plan } from '@/api/gym';
import { summariseGrants } from '@/features/billing/entitlements';
import { useSession } from '@/session/store';
import {
  Badge,
  Button,
  Callout,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** Today in the member's own calendar, as the API's YYYY-MM-DD. */
function localToday(): string {
  const d = new Date();
  return `${d.getFullYear()}-${`${d.getMonth() + 1}`.padStart(2, '0')}-${`${d.getDate()}`.padStart(2, '0')}`;
}

/**
 * Join a plan (ADR-0031).
 *
 * The screen that turns "you cannot start a workout" from a dead end into a
 * sentence with a button under it.
 *
 * Before this, a gym that sold anything blocked every member who was not on
 * something — and nothing in the app could put them on one, because
 * subscribing was managers-only. The refusal listed the plans that *would*
 * have let them in, which read as taunting. Now it links here.
 *
 * What a member may do is narrow on purpose: subscribe **themselves**, to a
 * plan the gym **currently offers**, when they are **not already on one**. The
 * price and what it unlocks are the gym's, copied at signup exactly as when a
 * manager does it. The only choice being made here is which of the gym's own
 * offers to take.
 */
export default function JoinPlan() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const userId = useSession((st) => st.user?.id ?? null);

  const [chosen, setChosen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  // The server narrows this to your own already; the filter is belt and braces
  // for the manager case, where the same call returns the whole gym.
  const mine = (subscriptions.data ?? []).find(
    (x) => x.member_id === userId && x.is_active,
  );

  const offered = useMemo(
    () => (plans.data ?? []).filter((p) => p.is_offered),
    [plans.data],
  );

  const join = useMutation({
    mutationFn: (planId: string) =>
      subscribeMember(gymId!, {
        memberId: userId!,
        planId,
        startedOn: localToday(),
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
          ? 'You are already on a plan. Ask at the desk to change it.'
          : e instanceof ApiError && e.status === 404
            ? 'That plan is no longer on sale.'
            : 'Could not set that up. Please try again.',
      );
    },
  });

  if (plans.isLoading || subscriptions.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  if (mine) {
    return (
      <Screen scroll>
        <Stack.Screen options={{ title: 'Membership' }} />
        <EmptyState
          glyph="✓"
          title="You are already on a plan"
          hint={`${mine.plan_name} · ${mine.price_label}. Changing plan is done at the desk, so nobody is billed twice by accident.`}
          action={
            <Button
              label="See your membership"
              variant="secondary"
              onPress={() => router.replace('/(app)/membership')}
            />
          }
        />
      </Screen>
    );
  }

  if (offered.length === 0) {
    return (
      <Screen scroll>
        <Stack.Screen options={{ title: 'Join a plan' }} />
        <EmptyState
          glyph="◫"
          title="Nothing on sale"
          hint="This gym is not selling memberships through the app. Nothing is being withheld from you — go and train."
        />
      </Screen>
    );
  }

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Join a plan' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Join a plan</Text>
        <Text style={s.lede}>
          Pick what you want and you can start training straight away. The first invoice is
          raised now and you can pay it by card from Membership.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      <View>
        <Section label="What this gym sells" count={offered.length} />
        <View style={s.plans}>
          {offered.map((plan) => (
            <PlanCard
              key={plan.id}
              plan={plan}
              selected={chosen === plan.id}
              onPress={() => {
                setError(null);
                setChosen(chosen === plan.id ? null : plan.id);
              }}
            />
          ))}
        </View>
      </View>

      {chosen ? (
        <Button
          label={
            join.isPending
              ? 'Setting up…'
              : `Join ${offered.find((p) => p.id === chosen)?.name ?? 'this plan'}`
          }
          disabled={join.isPending}
          onPress={() => join.mutate(chosen)}
        />
      ) : null}

      <Callout>
        Joining raises your first invoice. It does not take a payment — you pay it when you
        are ready, by card or at the desk.
      </Callout>
    </Screen>
  );
}

function PlanCard({
  plan,
  selected,
  onPress,
}: {
  plan: Plan;
  selected: boolean;
  onPress: () => void;
}) {
  const s = useStyles(styleFactory);
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="radio"
      accessibilityState={{ selected }}
      accessibilityLabel={`${plan.name}, ${plan.price_label} ${
        plan.interval === 'monthly' ? 'a month' : 'once'
      }. Unlocks ${summariseGrants(plan.grants)}.`}
      style={[s.plan, selected && s.planOn]}
    >
      <View style={s.planHead}>
        <Text style={s.planName}>{plan.name}</Text>
        <Text style={s.planPrice}>
          {plan.price_label}
          <Text style={s.planInterval}>{plan.interval === 'monthly' ? ' /mo' : ' once'}</Text>
        </Text>
      </View>
      {plan.description ? <Text style={s.planBody}>{plan.description}</Text> : null}
      {/* What it unlocks, not what the owner wrote about it: the grants are
          what the gate actually reads when you press Start. */}
      <Badge tone={selected ? 'accent' : 'muted'}>{summariseGrants(plan.grants)}</Badge>
    </Pressable>
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

    plans: { gap: t.space.md, paddingTop: t.space.sm },
    plan: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      gap: t.space.sm,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    planOn: { backgroundColor: t.color.accentHi, borderColor: t.color.accent },
    planHead: { alignItems: 'baseline', flexDirection: 'row', gap: t.space.md },
    planName: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    planPrice: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    planInterval: { color: t.color.mut, fontFamily: fonts.medium, fontSize: t.font.sm },
    planBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },
  });
