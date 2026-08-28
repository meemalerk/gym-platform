import { useMutation, useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { joinOpenGym, listOpenGyms, signOut } from '@/api/gym';
import { markPlanChoicePending } from '@/session/onboarding';
import { useSession } from '@/session/store';
import { GYM_ID } from '@/config';
import { Appear, Button, Callout, Centered, ErrorBanner, Kicker, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * The first thing an account with no gym sees.
 *
 * There is one door now (ADR-0031). ADR-0026 shipped two — an open door and an
 * invite code — and the code half is gone: it needed an email the deployment
 * does not send, so the flow could not be finished inside the app, and staff
 * are made from members afterwards rather than arriving pre-titled.
 *
 * That makes this screen simple and occasionally unhelpful, and the unhelpful
 * case is stated rather than hidden: if the gym has not opened its doors,
 * there is nothing here to press, and somebody at the desk has to open them.
 */
export default function Start() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);

  const open = useQuery({ queryKey: ['open-gyms'], queryFn: listOpenGyms });

  const join = useMutation({
    mutationFn: (gymId: string) => joinOpenGym(gymId),
    onSuccess: async () => {
      /*
        Joining is the moment registration acquires a plan step, so it is the
        moment the marker is set — and the ONLY moment. Somebody signing in to
        an account that joined months ago never comes through here, which is why
        they are no longer asked to choose a plan every time.

        Marked before navigating: the guard reads it, and setting it afterwards
        would leave a frame in which the onboarding group has nothing keeping it
        mounted.
      */
      const userId = useSession.getState().user?.id;
      if (userId) {
        await markPlanChoicePending(userId);
        useSession.getState().setPlanPendingFor(userId);
      }
      router.replace('/(onboarding)/choose-plan');
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.status === 404
          ? 'That gym has stopped accepting new members. Ask at the desk.'
          : e instanceof ApiError && e.code === 'resource.conflict'
            ? 'You already belong to this gym.'
            : 'Could not join right now. Please try again.',
      );
    },
  });

  const gyms = open.data ?? [];

  /*
    One gym means no question.

    This is a single-gym deployment (ADR-0023), so "which gym?" has exactly one
    answer and asking it is a screen between somebody and the product. When the
    list has one entry we join it and go, and the only thing a new member is
    actually asked is which membership they want.

    The picker below survives for the case this cannot assume away: a dev
    database with several gyms open. In production it never renders.
  */
  /*
    Which gym to join, in order of preference:

      1. the one this build is CONFIGURED for (`EXPO_PUBLIC_GYM_ID`) — a
         single-gym deployment knows its own gym, so there is nothing to ask;
      2. the only one with an open door, if there is exactly one;
      3. nothing, and the picker below renders.

    (2) exists for the browser demo, which sets no gym id. (3) should never be
    reached in production and is why the picker still exists at all: in a dev
    database every throwaway gym the verify suites created is still advertising
    an open door, and silently joining an arbitrary one of those would be worse
    than asking.
  */
  const only = GYM_ID
    ? (gyms.find((g) => g.id === GYM_ID) ?? { id: GYM_ID, name: 'your gym' })
    : gyms.length === 1
      ? gyms[0]
      : undefined;
  const onlyId = only?.id;
  const autoJoined = useRef(false);
  /*
    Fire once, keyed on the gym's ID rather than the object.

    `join` is deliberately NOT a dependency: the object `useMutation` returns
    changes identity as its state moves, so listing it re-runs this on every
    mutation update. `mutate` is stable, and the ref is what guarantees one
    attempt — together they mean a re-render cannot turn into a second join.
  */
  const joinMutate = join.mutate;
  useEffect(() => {
    if (onlyId && !autoJoined.current) {
      autoJoined.current = true;
      joinMutate(onlyId);
    }
  }, [onlyId, joinMutate]);

  return (
    <Screen scroll edges={['top', 'bottom']}>
      <View style={s.intro}>
        <Appear>
          <Kicker tone="accent">Welcome</Kicker>
        </Appear>
        <Appear index={1}>
          <Text style={s.h1}>You&apos;re in.{'\n'}Now find your gym.</Text>
        </Appear>
        <Appear index={2}>
          <Text style={s.lede}>
            Your account exists. What it can do depends on the gym you attach it to.
          </Text>
        </Appear>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {open.isLoading || only ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : gyms.length === 0 ? (
        <View style={s.option}>
          <Text style={s.optionTitle}>No gym is taking members right now</Text>
          <Text style={s.optionBody}>
            A gym has to open its doors before anyone can join. Ask at the desk — whoever runs
            it can turn that on, and you will be able to join from this screen straight away.
          </Text>
        </View>
      ) : (
        <View style={s.options}>
          {gyms.map((gym) => (
            <View key={gym.id} style={s.option}>
              <Text style={s.optionTitle}>{gym.name}</Text>
              <Text style={s.optionBody}>
                Join now and start training. Your coach, your programme and what you pay for
                all come after — none of it is decided here.
              </Text>
              <Button
                label={join.isPending ? 'Joining…' : `Join ${gym.name}`}
                disabled={join.isPending}
                onPress={() => {
                  setError(null);
                  join.mutate(gym.id);
                }}
              />
            </View>
          ))}
        </View>
      )}

      {/*
        Said once, here, because it is the question the old invite-code screen
        was answering and somebody will look for it: you do not arrive as
        staff, you are made staff.
      */}
      <Callout>
        Everyone joins as a member. If you coach here, whoever runs the gym will set that on
        your account once you are in.
      </Callout>

      <View style={s.footer}>
        <Button label="Sign out" variant="ghost" onPress={() => void signOut()} />
      </View>
    </Screen>
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
    options: { gap: t.space.lg },
    option: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      gap: t.space.md,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    optionTitle: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    optionBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    footer: { marginTop: 'auto', paddingTop: t.space.xl },
  });
