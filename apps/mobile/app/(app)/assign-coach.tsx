import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  proposeCoach,
  listCoachRelationships,
  listMembers,
  type GymMember,
} from '@/api/gym';
import { can, capacityLabel, useSession } from '@/session/store';
import {
  Badge,
  Button,
  Callout,
  Centered,
  ErrorBanner,
  InitialsSquare,
  Screen,
  Section,
  Strong,
  Touchable,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Pair a coach with an athlete.
 *
 * Two pickers rather than a free-text search, because the roster of a gym is
 * small enough to scroll and a picker cannot produce an id that does not exist.
 * Search belongs here once a gym has hundreds of members (UX-3).
 *
 * The consequence of the pairing is stated before it is made: this grants one
 * person sight of another's training data, which is not something to discover
 * afterwards.
 */
export default function AssignCoach() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));

  const [coachId, setCoachId] = useState<string | null>(null);
  const [athleteId, setAthleteId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const members = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId),
  });
  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId),
  });

  // Only people who may actually coach can be the coach. Offering everyone and
  // letting the server refuse would be a worse way to learn the same rule.
  const coaches = useMemo(
    () => (members.data ?? []).filter((m: GymMember) => can.coach(m.capacities)),
    [members.data],
  );

  // Anyone in the gym can be coached, including a trainer — being coached by the
  // head coach is normal, and filtering to `member` only would forbid it.
  const athletes = members.data ?? [];

  const nameOf = (id: string | null) =>
    (members.data ?? []).find((m) => m.user_id === id)?.display_name ?? null;

  /*
    This PROPOSES the pairing; it does not make it (ADR-0034).

    A coaching relationship hands the trainer that member's whole training
    history, and the trainer was never asked. So the gym names the pairing and
    the trainer accepts — and the person who proposed it cannot accept it, which
    is what makes the consent real rather than two taps by the same person.
  */
  const mutation = useMutation({
    mutationFn: () => proposeCoach(gymId!, { coachId: coachId!, athleteId: athleteId! }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['coaching-requests', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['coach-relationships', gymId] });
      setNotice(
        `Sent to ${nameOf(coachId) ?? 'the trainer'}. They will see it on Today and the pairing starts when they accept.`,
      );
      setCoachId(null);
      setAthleteId(null);
    },
    onError: (e: Error) => {
      // Branch on the stable code, never on human-readable text.
      if (e instanceof ApiError && e.code === 'resource.conflict') {
        setError('These two already work together, or a proposal is already waiting.');
      } else if (e instanceof ApiError && e.code === 'auth.forbidden') {
        setError('You do not have permission to propose coaches in this gym.');
      } else if (e instanceof ApiError && e.code === 'resource.not_found') {
        setError('One of those people is not in this gym, or does not coach here.');
      } else {
        setError('Could not send the proposal. Please try again.');
      }
    },
  });

  const ready = Boolean(coachId && athleteId) && coachId !== athleteId;
  const existing = (relationships.data ?? []).filter((r) => r.is_active);

  if (members.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  if (members.isError) {
    return (
      <Screen>
        <ErrorBanner message="Could not load the gym's members." />
      </Screen>
    );
  }

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Propose a coach' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Propose a coach</Text>
        <Text style={s.lede}>
          Pick the trainer, then the member. The trainer decides — nothing
          happens until they accept it on their own Today screen.
        </Text>
      </View>

      {/*
        Numbered, because this is a two-part choice and the Send button stays
        disabled until both halves are made. Without the steps the screen reads
        as two unrelated lists and a dead button.
      */}
      <View style={s.steps}>
        <StepDot done={Boolean(coachId)} n={1} label="Trainer" />
        <View style={s.stepLine} />
        <StepDot done={Boolean(athleteId)} n={2} label="Member" />
      </View>

      {notice ? <Callout tone="success">{notice}</Callout> : null}
      {error ? <ErrorBanner message={error} /> : null}

      <PersonPicker
        label="Coach"
        people={coaches}
        selectedId={coachId}
        onSelect={(id) => {
          setCoachId(id);
          setError(null);
        }}
        emptyHint="Nobody in this gym holds a coaching capacity yet."
      />

      <PersonPicker
        label="Athlete"
        people={athletes}
        selectedId={athleteId}
        // Someone cannot coach themselves — excluded here rather than caught on
        // submit, so the invalid choice is never offered.
        excludeId={coachId}
        onSelect={(id) => {
          setAthleteId(id);
          setError(null);
        }}
        emptyHint="No members to coach yet."
      />

      {ready ? (
        <Callout>
          If <Strong>{nameOf(coachId)}</Strong> accepts, they will see{' '}
          {nameOf(athleteId)}&apos;s sessions, goals and body trend, and will be
          the one who puts them on programmes. Nothing happens until they do.
        </Callout>
      ) : null}

      <Button
        label={mutation.isPending ? 'Sending…' : 'Send to the trainer'}
        disabled={!ready || mutation.isPending}
        onPress={() => {
          setError(null);
          setNotice(null);
          mutation.mutate();
        }}
      />

      {existing.length > 0 ? (
        <View>
          <Section label="Existing pairings" count={existing.length} />
          {existing.map((r, i) => (
            <View key={r.id} style={[s.pairing, i === existing.length - 1 && s.pairingLast]}>
              <Text style={s.pairingText} numberOfLines={1}>
                <Text style={s.pairingName}>{r.coach_name ?? 'Coach'}</Text>
                {'  →  '}
                <Text style={s.pairingName}>{r.athlete_name ?? 'Athlete'}</Text>
              </Text>
            </View>
          ))}
        </View>
      ) : null}
    </Screen>
  );
}

/**
 * One step of the two-part choice.
 *
 * A filled dot with a tick once that half is chosen, hollow with its number
 * until then. No emoji: the design language uses fill and colour to carry
 * state, and a check glyph inside a filled dot is the same idea the rest of
 * the app uses for "done".
 */
function StepDot({ done, n, label }: { done: boolean; n: number; label: string }) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  return (
    <View style={s.step}>
      <View style={[s.stepDot, done && s.stepDotDone]}>
        {done ? (
          <Feather name="check" size={13} color={t.color.onAccent} />
        ) : (
          <Text style={s.stepNum}>{n}</Text>
        )}
      </View>
      <Text style={[s.stepLabel, done && s.stepLabelDone]}>{label}</Text>
    </View>
  );
}

function PersonPicker({
  label,
  people,
  selectedId,
  excludeId,
  onSelect,
  emptyHint,
}: {
  label: string;
  people: GymMember[];
  selectedId: string | null;
  excludeId?: string | null;
  onSelect: (id: string) => void;
  emptyHint: string;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const options = people.filter((p) => p.user_id !== excludeId);

  return (
    <View>
      <Section label={label} />
      {options.length === 0 ? (
        <View style={s.empty}>
          <Text style={s.emptyText}>{emptyHint}</Text>
        </View>
      ) : (
        <ScrollView style={s.list} nestedScrollEnabled showsVerticalScrollIndicator={false}>
          {options.map((person, i) => {
            const selected = person.user_id === selectedId;
            return (
              <Touchable
                key={person.user_id}
                onPress={() => onSelect(person.user_id)}
                accessibilityRole="radio"
                accessibilityState={{ selected }}
                accessibilityLabel={`${person.display_name}. ${person.capacities
                  .map(capacityLabel)
                  .join(', ')}.`}
                style={[
                  s.option,
                  i === options.length - 1 && s.optionLast,
                  selected && s.optionSelected,
                ]}
              >
                <InitialsSquare name={person.display_name} size={32} />
                <View style={s.optionBody}>
                  <Text style={s.optionName} numberOfLines={1}>
                    {person.display_name}
                  </Text>
                  <View style={s.badges}>
                    {person.capacities.map((c: string) => (
                      <Badge key={c} tone={selected ? 'accent' : 'outline'}>
                        {capacityLabel(c)}
                      </Badge>
                    ))}
                  </View>
                </View>
                {selected ? <Feather name="check" size={18} color={t.color.accent} /> : null}
              </Touchable>
            );
          })}
        </ScrollView>
      )}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    steps: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    step: { alignItems: 'center', flexDirection: 'row', gap: 7 },
    stepDot: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.pill,
      height: 24,
      justifyContent: 'center',
      width: 24,
    },
    stepDotDone: { backgroundColor: t.color.accent },
    stepNum: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
    stepLabel: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
    },
    stepLabelDone: { color: t.color.ink },
    // A hairline between the two steps, so they read as a sequence.
    stepLine: { backgroundColor: t.color.line, flex: 1, height: StyleSheet.hairlineWidth },
    empty: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      padding: t.space.lg,
    },
    emptyText: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },
    intro: { gap: 6, paddingTop: t.space.sm },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 30,
    },
    lede: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5, lineHeight: 20 },

    // Capped so both pickers and the button stay reachable without hunting.
    list: { maxHeight: 232 },
    option: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: 12,
      paddingHorizontal: 4,
      paddingVertical: 10,
    },
    optionLast: { borderBottomWidth: 0 },
    optionSelected: { backgroundColor: t.color.accentHi },
    optionBody: { flex: 1, gap: 4 },
    optionName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    badges: { flexDirection: 'row', flexWrap: 'wrap', gap: 5 },

    pairing: {
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      paddingVertical: 12,
    },
    pairingLast: { borderBottomWidth: 0 },
    pairingText: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5 },
    pairingName: { color: t.color.ink, fontFamily: fonts.semibold },
  });
