import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { listPlans, signOut, subscribeMember, type Plan } from '@/api/gym';
import { clearPlanChoicePending } from '@/session/onboarding';
import { useSession } from '@/session/store';
import {
  Appear,
  Button,
  Centered,
  ErrorBanner,
  Kicker,
  Screen,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Pick a membership. The only thing a new member sees, and they cannot leave
 * without choosing.
 *
 * **Why this replaced the old two-step onboarding.** It used to be "which gym?"
 * then "how do you want to train?". The first question is noise in a single-gym
 * deployment (ADR-0023) — there is exactly one answer — and the second one
 * wrote nothing at all: it was a signpost to the coach directory.
 *
 * The plans already carry that second question, and carry it honestly, because
 * a plan states what it GRANTS. A plan with `coached_programming` is "with a
 * trainer"; one with only `gym_access` is "on your own". So this screen asks the
 * real question — what are you buying — and the training style falls out of the
 * answer instead of being asked separately and then ignored.
 *
 * **It gates registration, and only registration.** The root layout keeps the
 * onboarding group mounted while this account carries the "still owes a plan
 * choice" marker set when it joined, so force-quitting here and reopening lands
 * back here rather than sneaking into a Today screen where everything tapped is
 * refused.
 *
 * The guard used to read the ENTITLEMENT instead — "no `gym_access` → choose a
 * plan" — which is also true of a member of three years whose subscription
 * lapsed, or who was signed up at the desk and never bought anything through
 * the app. They were shown this screen on every sign-in with no way past it.
 * Registration is a journey that ends; a billing state is not, and gating on
 * one meant existing members could not get in. See `@/session/onboarding`.
 */
export default function ChoosePlan() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const queryClient = useQueryClient();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const userId = useSession((st) => st.user?.id);

  const [picked, setPicked] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const plans = useQuery({
    queryKey: ['plans', gymId],
    queryFn: () => listPlans(gymId!),
    enabled: Boolean(gymId),
  });

  const join = useMutation({
    mutationFn: (planId: string) =>
      subscribeMember(gymId!, {
        memberId: userId!,
        planId,
        startedOn: localToday(),
      }),
    onSuccess: async () => {
      /*
        Clearing the marker flips the root layout's guard, which is what moves
        them into the app — no imperative navigation here, for the same reason
        the sign-in flow has none.

        The guard reads the MARKER, not the entitlement. It used to read the
        entitlement, and that made this screen a wall in front of every existing
        member whose subscription had lapsed or who had never bought one through
        the app. Registration is a journey with an end; a billing state is not.
      */
      await clearPlanChoicePending();
      useSession.getState().setPlanPendingFor(null);

      void queryClient.invalidateQueries({ queryKey: ['entitlements', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['subscriptions', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['invoices', gymId] });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'You are already on a plan here. Pull down to refresh.'
          : e instanceof ApiError
            ? e.message
            : 'Could not start that membership. Please try again.',
      );
    },
  });

  if (plans.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  const offered = [...(plans.data ?? [])].sort((a, b) => a.price_minor - b.price_minor);

  return (
    <Screen scroll edges={['top', 'bottom']}>
      <View style={s.intro}>
        <Appear>
          <Kicker tone="accent">One last thing</Kicker>
        </Appear>
        <Appear index={1}>
          <Text style={s.h1}>Choose your{'\n'}membership.</Text>
        </Appear>
        <Appear index={2}>
          <Text style={s.lede}>
            This decides what you can do here — and whether you train on your own
            or with a coach. You can change it whenever you like.
          </Text>
        </Appear>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {plans.isError ? (
        <ErrorBanner message="Could not load the memberships. Pull down to try again." />
      ) : offered.length === 0 ? (
        /*
          A gym that sells nothing withholds nothing (`Source::NotBilled`), so
          this account already has access and the guard will not have sent them
          here. Stated anyway rather than rendering an empty screen.
        */
        <View style={s.card}>
          <Text style={s.cardTitle}>Nothing on sale yet</Text>
          <Text style={s.cardBody}>
            This gym has not set up memberships, so nothing is being withheld —
            you can train. If this screen persists, ask at the desk.
          </Text>
        </View>
      ) : (
        <View style={s.options}>
          {offered.map((plan, i) => (
            <Appear key={plan.id} index={3 + i}>
              <PlanCard
                plan={plan}
                selected={picked === plan.id}
                onPress={() => {
                  setError(null);
                  setPicked(plan.id);
                }}
              />
            </Appear>
          ))}
        </View>
      )}

      {offered.length > 0 ? (
        <Button
          label={join.isPending ? 'Starting…' : 'Start this membership'}
          detail={picked ? offered.find((p) => p.id === picked)?.price_label : undefined}
          disabled={!picked || join.isPending}
          onPress={() => picked && join.mutate(picked)}
        />
      ) : null}

      {/*
        The only way out other than choosing. Left in on purpose: trapping
        somebody in an app with no exit is worse than letting them leave, and
        the account survives — they land back here next time they sign in.
      */}
      <View style={s.footer}>
        <Button label="Sign out" variant="ghost" onPress={() => void signOut()} />
      </View>
    </Screen>
  );
}

/** Today in the local calendar, as the API's YYYY-MM-DD. */
function localToday(): string {
  const d = new Date();
  const p = (n: number) => `${n}`.padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/**
 * One plan, and what it means in plain words.
 *
 * The grants are translated rather than listed: "coached_programming" is a
 * database value, and a member deciding how to spend £120 needs "a coach writes
 * your programme", not the identifier.
 */
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
  const coached = plan.grants.includes('coached_programming');

  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="radio"
      accessibilityState={{ selected }}
      accessibilityLabel={`${plan.name}, ${plan.price_label} ${plan.interval === 'once' ? 'one-off' : 'a month'}`}
      style={[s.card, selected && s.cardOn]}
    >
      <View style={s.cardHead}>
        <Text style={s.cardTitle}>{plan.name}</Text>
        <Text style={s.price}>{plan.price_label}</Text>
      </View>
      <Text style={s.interval}>
        {plan.interval === 'once' ? 'One-off' : 'Every month'}
      </Text>
      <Text style={s.cardBody}>
        {coached
          ? 'With a coach. They write your programme, watch your logs and adjust as you go — and can see your training history.'
          : 'On your own. Pick any published programme yourself, log your workouts, book classes.'}
      </Text>
      {plan.description ? <Text style={s.cardNote}>{plan.description}</Text> : null}
    </Pressable>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    intro: { gap: t.space.sm, paddingBottom: t.space.sm, paddingTop: t.space.md },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 32,
      marginTop: 4,
    },
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    options: { gap: t.space.md },
    card: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      gap: 6,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    // Selection is a fill plus a rule, not a tick: the whole card is the target.
    cardOn: { backgroundColor: t.color.accentBadge, borderColor: t.color.accent },
    cardHead: { alignItems: 'baseline', flexDirection: 'row', gap: t.space.sm },
    cardTitle: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    price: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.lg,
      fontVariant: ['tabular-nums'],
    },
    interval: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    cardBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    cardNote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
    footer: { marginTop: 'auto', paddingTop: t.space.lg },
  });
