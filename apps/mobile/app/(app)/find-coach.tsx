import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  chooseCoach,
  listCoachRelationships,
  listTrainers,
  type TrainerDirectoryEntry,
} from '@/api/gym';
import { useSession } from '@/session/store';
import {
  Badge,
  Button,
  Callout,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  Field,
  InitialsSquare,
  Screen,
  Section,
  Touchable,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Pick a coach. One step (ADR-0031).
 *
 * This used to send a request the coach then had to accept, and the screen
 * said so at some length. The handshake is gone: the thing a member grants
 * here is access to their *own* training history, which is theirs to grant,
 * and waiting on somebody else to open an app meant the pairing — and the
 * programme, and the workout — did not happen.
 *
 * So the screen is a list and a button. The note is still asked for, because
 * it is the only context the coach gets and "back to squatting after an ankle
 * injury" changes the first session they write.
 */
export default function FindCoach() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const userId = useSession((st) => st.user?.id);

  const [chosen, setChosen] = useState<TrainerDirectoryEntry | null>(null);
  const [message, setMessage] = useState('');
  const [error, setError] = useState<string | null>(null);

  const trainers = useQuery({
    queryKey: ['trainers', gymId],
    queryFn: () => listTrainers(gymId!),
    enabled: Boolean(gymId),
  });

  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId),
  });

  /**
   * Coaches you already work with.
   *
   * Shown on the row rather than filtered out. Removing somebody from the list
   * because you already picked them looks like they vanished; saying "your
   * coach" is the honest version, and it stops the second tap that would only
   * earn a 409.
   */
  const already = useMemo(() => {
    const set = new Set<string>();
    for (const r of relationships.data ?? []) {
      if (r.athlete_id === userId && r.is_active) set.add(r.coach_id);
    }
    return set;
  }, [relationships.data, userId]);

  // You cannot coach yourself, and the server refuses it — so do not offer it.
  const candidates = useMemo(
    () => (trainers.data ?? []).filter((x) => x.user_id !== userId),
    [trainers.data, userId],
  );

  const choose = useMutation({
    mutationFn: () => chooseCoach(gymId!, chosen!.user_id, message),
    onSuccess: () => {
      // Both lists changed: the pairing exists now, and the directory's
      // client counts moved with it.
      void queryClient.invalidateQueries({ queryKey: ['coach-relationships', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['trainers', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? e.message
          : e instanceof ApiError && e.status === 404
            ? 'That person is no longer coaching here.'
            : 'Could not set that up. Please try again.',
      );
    },
  });

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Choose a coach' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Choose a coach</Text>
        <Text style={s.lede}>
          Pick someone and they start coaching you straight away. They will be able to see your
          training history and write your programme.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {trainers.isLoading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : candidates.length === 0 ? (
        <EmptyState
          glyph="○"
          title="No coaches here yet"
          hint="Nobody at this gym is set up to coach. Whoever runs it can change that from the roster."
        />
      ) : (
        <View>
          <Section label="Coaches here" count={candidates.length} />
          <Card padded={false}>
            {candidates.map((coach, i, arr) => {
              const mine = already.has(coach.user_id);
              const on = chosen?.user_id === coach.user_id;
              return (
                <Touchable
                  key={coach.user_id}
                  onPress={() => !mine && setChosen(on ? null : coach)}
                  disabled={mine}
                  accessibilityRole="radio"
                  accessibilityState={{ selected: on, disabled: mine }}
                  accessibilityLabel={
                    mine
                      ? `${coach.display_name}, already your coach`
                      : `${coach.display_name}. ${coach.headline ?? 'Coach'}.`
                  }
                  style={[s.row, i === arr.length - 1 && s.rowLast, on && s.rowOn]}
                >
                  <InitialsSquare name={coach.display_name} />
                  <View style={s.rowBody}>
                    <View style={s.rowHead}>
                      <Text style={s.rowName}>{coach.display_name}</Text>
                      {mine ? <Badge tone="success">Your coach</Badge> : null}
                    </View>
                    {coach.headline ? <Text style={s.rowLine}>{coach.headline}</Text> : null}
                    {coach.specialties.length > 0 ? (
                      <Text style={s.rowMeta} numberOfLines={1}>
                        {coach.specialties.join(' · ')}
                      </Text>
                    ) : null}
                    <Text style={s.rowMeta}>
                      {/* A capacity signal, not a popularity score: somebody
                          deciding who to pick deserves to know who has room. */}
                      {coach.active_clients === 0
                        ? 'No clients yet'
                        : coach.active_clients === 1
                          ? '1 client'
                          : `${coach.active_clients} clients`}
                    </Text>
                  </View>
                  {on ? <Text style={s.tick}>✓</Text> : null}
                </Touchable>
              );
            })}
          </Card>
        </View>
      )}

      {chosen ? (
        <View style={s.compose}>
          <Field
            label="Anything they should know?"
            value={message}
            onChangeText={setMessage}
            placeholder="Optional — e.g. back to squatting after an ankle injury"
            hint="They see this on their client list. It is the only context they get."
            multiline
          />
          <Button
            label={choose.isPending ? 'Setting up…' : `Start with ${chosen.display_name}`}
            disabled={choose.isPending}
            onPress={() => {
              setError(null);
              choose.mutate();
            }}
          />
          <Callout>
            You can change coach at any time, and so can they — ask at the desk to end it.
          </Callout>
        </View>
      ) : null}
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
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    row: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: t.space.md,
    },
    rowLast: { borderBottomWidth: 0 },
    rowOn: { backgroundColor: t.color.accentHi },
    rowBody: { flex: 1, gap: 3 },
    rowHead: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    rowName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    rowLine: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm },
    rowMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    tick: { color: t.color.accentDeep, fontFamily: fonts.bold, fontSize: t.font.lg },
    compose: { gap: t.space.lg },
  });
