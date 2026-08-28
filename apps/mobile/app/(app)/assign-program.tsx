import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  assignProgram,
  listAssignments,
  listCoachRelationships,
  listMembers,
  type GymMember,
} from '@/api/gym';
import { can, useActiveMembership, useSession } from '@/session/store';
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

/** Today, in the local calendar, as the API's YYYY-MM-DD. */
function localToday(): string {
  const d = new Date();
  const p = (n: number) => `${n}`.padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/**
 * Put an athlete on this published version.
 *
 * The version is pinned by id, never "the latest": if this programme is edited
 * tomorrow, the athlete stays on the one they were actually given (ADR-0006).
 *
 * Who is offered follows the same rule as the server: a manager may assign to
 * anyone in the gym; a trainer only to their own clients. The list is filtered
 * here so a trainer is not shown names they will then be refused for.
 */
export default function AssignProgram() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const userId = useSession((st) => st.user?.id);
  const { version, name } = useLocalSearchParams<{ version: string; name?: string }>();

  const active = useActiveMembership();
  const isManager = can.manageCatalogue(active?.capacities ?? []);

  type Candidate = Pick<GymMember, 'user_id' | 'display_name'>;
  const [chosen, setChosen] = useState<Candidate | null>(null);
  const [startDate, setStartDate] = useState(localToday());
  const [error, setError] = useState<string | null>(null);

  /*
    Each role asks the ONE question it is allowed to ask.

    This screen used to fetch the roster unconditionally and filter it down for
    trainers. The roster is head-coach-and-above (`GymService::roster` — a
    membership list is personal information about everyone on it), so for the
    people this screen exists to serve it returned 403, `candidates` came back
    empty, and a trainer was told "you are not coaching anyone" while the
    server would happily have accepted the assignment. Asking for something you
    will be refused and then rendering the refusal as an empty state is the
    worst of both.
  */
  const members = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId) && isManager,
  });

  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId) && !isManager,
  });

  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });

  /** Already on this exact version — assigning twice is a no-op worth avoiding. */
  const alreadyOn = useMemo(() => {
    const set = new Set<string>();
    for (const a of assignments.data ?? []) {
      if (a.program_version_id === version && a.is_active) set.add(a.athlete_id);
    }
    return set;
  }, [assignments.data, version]);

  /**
   * Who this caller may put on a programme, in the shape the list renders.
   *
   * A manager picks from the roster; a trainer picks from their own active
   * clients, which the relationship list already names — so no wider read is
   * needed, and the client never holds a list it was not entitled to.
   */
  const candidates = useMemo((): Candidate[] => {
    if (isManager) {
      return (members.data ?? []).filter((m) => m.capacities.includes('member'));
    }

    return (relationships.data ?? [])
      .filter((r) => r.coach_id === userId && r.is_active)
      .map((r) => ({
        user_id: r.athlete_id,
        // The roster carries no emails and this carries no capacities; both
        // are deliberate. A name is all this screen needs.
        display_name: r.athlete_name ?? 'Member',
      }));
  }, [members.data, relationships.data, isManager, userId]);

  const loadingCandidates = isManager ? members.isLoading : relationships.isLoading;

  const save = useMutation({
    mutationFn: () =>
      assignProgram(gymId!, {
        athleteId: chosen!.user_id,
        programVersionId: version,
        startDate,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['assignments', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'They are already on this programme.'
          : e instanceof ApiError && e.code === 'auth.forbidden'
            ? 'You can only assign programmes to your own clients.'
            : e instanceof ApiError && e.code === 'request.invalid'
              ? e.message
              : 'Could not assign that. Please try again.',
      );
    },
  });

  const dateValid = /^\d{4}-\d{2}-\d{2}$/.test(startDate);

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Assign' }} />

      <View style={s.intro}>
        <Text style={s.h1}>Assign</Text>
        <Text style={s.lede}>
          {name ? `${name}. ` : ''}They stay on this exact version even if you publish a new one
          later.
        </Text>
      </View>

      {error ? <ErrorBanner message={error} /> : null}

      {loadingCandidates ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : candidates.length === 0 ? (
        <EmptyState
          glyph="○"
          title={isManager ? 'No members yet' : 'No clients yet'}
          hint={
            isManager
              ? 'Invite someone as a member first — a programme needs an athlete.'
              : 'You are not coaching anyone in this gym yet. A head coach pairs you first.'
          }
        />
      ) : (
        <View>
          <Section label="Who" count={candidates.length} />
          {candidates.map((m, i, arr) => {
            const on = chosen?.user_id === m.user_id;
            const already = alreadyOn.has(m.user_id);
            return (
              <Pressable
                key={m.user_id}
                onPress={() => !already && setChosen(m)}
                disabled={already}
                accessibilityRole="radio"
                accessibilityState={{ selected: on, disabled: already }}
                accessibilityLabel={
                  already ? `${m.display_name}, already on this version` : m.display_name
                }
                style={[s.row, i === arr.length - 1 && s.rowLast, on && s.rowOn]}
              >
                <InitialsSquare name={m.display_name} />
                <View style={s.rowBody}>
                  <Text style={s.rowName}>{m.display_name}</Text>
                  {already ? <Text style={s.rowMeta}>Already on this version</Text> : null}
                </View>
                {on ? <Text style={s.tick}>✓</Text> : null}
              </Pressable>
            );
          })}
        </View>
      )}

      <Field
        label="Starts on"
        value={startDate}
        onChangeText={setStartDate}
        placeholder="YYYY-MM-DD"
        autoCapitalize="none"
        error={dateValid ? undefined : 'A date like 2026-08-01'}
      />

      <Button
        label={save.isPending ? 'Assigning…' : 'Assign programme'}
        disabled={save.isPending || !chosen || !dateValid}
        onPress={() => {
          setError(null);
          save.mutate();
        }}
      />
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
  });
