import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useMemo } from 'react';
import { ActivityIndicator, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import {
  listAssignments,
  listCoachRelationships,
  listMembers,
  listSessions,
  type CoachRelationship,
} from '@/api/gym';
import { daysSince } from '@/features/coaching/attention';
import { can, capacityLabel, useActiveMembership, useSession } from '@/session/store';
import {
  Badge,
  Button,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  InitialsSquare,
  ListRow,
  LivePill,
  Section,
} from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

export default function People() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const tabBarHeight = useTabBarHeight();
  const now = new Date();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const userId = useSession((st) => st.user?.id);
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];

  // Standing is the replacement for invitations (ADR-0031): everybody joins
  // as a member and somebody who runs the gym promotes them.
  const canSetStanding = can.setCapacities(capacities);
  // Pairing is head-coach-and-above, matching the server. A trainer assigning
  // themselves a client would be a self-service grant of access to that
  // member's data, so the control is not offered rather than offered and refused.
  const canPair = can.manageCatalogue(capacities);
  const isManager = canPair || canSetStanding;

  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId),
  });
  const roster = useQuery({
    queryKey: ['members', gymId],
    queryFn: () => listMembers(gymId!),
    enabled: Boolean(gymId) && isManager,
  });
  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });
  const sessions = useQuery({
    queryKey: ['sessions', gymId],
    queryFn: () => listSessions(gymId!),
    enabled: Boolean(gymId),
  });

  /*
    The server already scoped this list to what the caller may see, so the split
    here is purely about presentation: which side of the relationship am I on?
    Someone who is both a trainer and a member sees both sections, which is the
    normal case under ADR-0014 rather than an oddity.
  */
  const { clients, coaches } = useMemo(() => {
    const rows = relationships.data ?? [];
    return {
      clients: rows.filter((r: CoachRelationship) => r.coach_id === userId),
      coaches: rows.filter((r: CoachRelationship) => r.athlete_id === userId),
    };
  }, [relationships.data, userId]);

  /** "Hypertrophy Block · trained 2 days ago" / "No programme assigned". */
  const clientLine = (
    athleteId: string,
  ): { text: string; alert: boolean; live?: boolean } => {
    const assignment = (assignments.data ?? []).find(
      (a) => a.athlete_id === athleteId && a.is_active,
    );
    if (!assignment) return { text: 'No programme assigned', alert: true };

    const theirs = (sessions.data ?? []).filter((x) => x.athlete_id === athleteId);
    if (theirs.some((x) => x.is_open)) {
      return { text: assignment.program_name ?? 'Programme', alert: false, live: true };
    }

    const gaps = theirs
      .map((x) => daysSince(x.started_at, now))
      .filter((d): d is number => d != null);
    if (gaps.length === 0) {
      return { text: `${assignment.program_name ?? 'Programme'} · never trained`, alert: true };
    }

    const idle = Math.min(...gaps);
    const when = idle === 0 ? 'trained today' : idle === 1 ? 'trained yesterday' : `${idle} days ago`;
    return { text: `${assignment.program_name ?? 'Programme'} · ${when}`, alert: idle >= 7 };
  };

  const loading = relationships.isLoading || (isManager && roster.isLoading);
  const nothingAtAll =
    clients.length === 0 &&
    coaches.length === 0 &&
    (roster.data ?? []).length === 0 &&
    !loading;

  return (
    <View style={s.screen}>
      <View style={[s.header, { paddingTop: insets.top + t.space.md }]}>
        <GymHeader
          title="People"
          meta={
            isManager
              ? `${(roster.data ?? []).length} in this gym`
              : `${clients.filter((c) => c.is_active).length} clients`
          }
        />
      </View>

      {relationships.isError ? (
        <View style={s.bannerWrap}>
          <ErrorBanner message="Could not load coaching relationships." />
        </View>
      ) : null}

      {loading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : (
        <ScrollView
          contentContainerStyle={[s.content, { paddingBottom: tabBarHeight + t.space.xl }]}
          showsVerticalScrollIndicator={false}
          refreshControl={
            <RefreshControl
              refreshing={relationships.isRefetching}
              onRefresh={() => {
                void relationships.refetch();
                if (isManager) void roster.refetch();
              }}
              tintColor={t.color.mut}
            />
          }
        >
          {/*
            The way in for the person this feature exists for. Shown only when
            it would do something: someone already coached, or already waiting
            on an answer, does not need a second door.
          */}
          {coaches.filter((c) => c.is_active).length === 0 ? (
            <View style={s.actions}>
              <Button
                label="Choose a coach"
                variant={clients.length > 0 ? 'secondary' : 'primary'}
                onPress={() => router.push('/(app)/find-coach')}
              />
            </View>
          ) : null}

          {isManager ? (
            <View style={s.actions}>
              {canSetStanding ? (
                <Button
                  label="Add a staff account"
                  onPress={() => router.push('/(app)/new-staff')}
                />
              ) : null}
              {canPair ? (
                <Button
                  label="Pair a coach with an athlete"
                  variant="secondary"
                  onPress={() => router.push('/(app)/assign-coach')}
                />
              ) : null}
            </View>
          ) : null}

          {clients.length > 0 ? (
            <View>
              <Section label="Your clients" count={clients.filter((c) => c.is_active).length} />
              <Card padded={false}>
              {clients.map((r, i) => {
                const line = r.is_active
                  ? clientLine(r.athlete_id)
                  : { text: 'No longer coaching', alert: false };
                return (
                  <ListRow
                    key={r.id}
                    title={r.athlete_name ?? 'Member'}
                    subtitle={line.text}
                    subtitleTone={line.alert ? 'reason' : 'muted'}
                    last={i === clients.length - 1}
                    left={<InitialsSquare name={r.athlete_name ?? 'Member'} />}
                    right={
                      !r.is_active ? (
                        <Badge tone="muted">Ended</Badge>
                      ) : line.live ? (
                        <LivePill />
                      ) : undefined
                    }
                    /*
                      The link this screen was missing. A client row used to be
                      inert, so a trainer could see that someone existed and
                      nothing else — no sessions, no history, no measurements.
                      Ended relationships stay un-tappable: the coach no longer
                      has standing to read that data, and offering a row that
                      404s would be worse than not offering it.
                    */
                    onPress={
                      r.is_active
                        ? () =>
                            router.push({
                              pathname: '/(app)/athlete/[id]',
                              params: { id: r.athlete_id, name: r.athlete_name ?? 'Member' },
                            })
                        : undefined
                    }
                  />
                );
              })}
              </Card>
            </View>
          ) : null}

          {coaches.length > 0 ? (
            <View>
              <Section label="Your coaches" count={coaches.filter((c) => c.is_active).length} />
              <Card padded={false}>
              {coaches.map((r, i) => (
                <ListRow
                  key={r.id}
                  title={r.coach_name ?? 'Coach'}
                  subtitle={r.is_active ? 'Coaches you' : 'Previously coached you'}
                  last={i === coaches.length - 1}
                  left={<InitialsSquare name={r.coach_name ?? 'Coach'} />}
                  right={!r.is_active ? <Badge tone="muted">Ended</Badge> : undefined}
                />
              ))}
              </Card>
            </View>
          ) : null}

          {/*
            The roster is the whole of "who is in this gym" now that
            invitations are gone (ADR-0031) — there are no person-shaped holes
            waiting to be filled, only people. Tapping somebody opens them; a
            manager can change what they hold from there.
          */}
          {isManager && (roster.data ?? []).length > 0 ? (
            <View>
              <Section label="Roster" meta="A – Z" />
              <Card padded={false}>
              {[...(roster.data ?? [])]
                .sort((a, b) => a.display_name.localeCompare(b.display_name))
                .map((m) => (
                  <ListRow
                    key={m.user_id}
                    title={m.user_id === userId ? `${m.display_name} · you` : m.display_name}
                    left={<InitialsSquare name={m.display_name} size={32} />}
                    // A manager already has oversight of everyone here
                    // (`may_view_athlete`), so the roster drills down too.
                    onPress={() =>
                      router.push({
                        pathname: '/(app)/athlete/[id]',
                        params: { id: m.user_id, name: m.display_name },
                      })
                    }
                    right={
                      <View style={s.badges}>
                        {m.capacities.map((c) => (
                          <Badge
                            key={c}
                            tone={c === 'owner' ? 'ink' : c === 'member' ? 'outline' : 'accent'}
                          >
                            {capacityLabel(c)}
                          </Badge>
                        ))}
                      </View>
                    }
                  />
                ))}
              </Card>
            </View>
          ) : null}

          {!isManager && clients.length > 0 ? (
            <Text style={s.footnote}>
              Pairing is done by a manager — ask your head coach to add clients.
            </Text>
          ) : null}

          {nothingAtAll ? (
            <EmptyState
              glyph="◍"
              title={canSetStanding ? 'Nobody here yet' : 'No clients yet'}
              hint={
                canSetStanding
                  ? 'Open the door in Manage and people can join. Anyone who joins arrives as a member; promote them from this list.'
                  : 'A head coach assigns the members you work with. They will appear here.'
              }
            />
          ) : null}
        </ScrollView>
      )}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    header: { paddingHorizontal: t.space.gutter },
    bannerWrap: { paddingHorizontal: t.space.gutter, paddingTop: t.space.md },
    content: { gap: t.space.xl, paddingHorizontal: t.space.gutter, paddingTop: t.space.md },
    actions: { gap: t.space.sm },
    stack: { gap: t.space.md },
    grow: { flex: 1 },
    badges: { flexDirection: 'row', flexWrap: 'wrap', gap: 5, justifyContent: 'flex-end', maxWidth: 150 },
    requestHead: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    requestBody: { flex: 1, gap: 2 },
    requestName: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    requestMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    requestNote: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
      paddingHorizontal: 14,
      paddingVertical: 11,
    },
    requestActions: { flexDirection: 'row', gap: t.space.sm },
    undo: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
    },
    footnote: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 18,
    },
  });
