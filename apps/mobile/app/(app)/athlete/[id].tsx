import { useQuery } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import {
  getAthleteProfileOf,
  getMeasurementsOf,
  listAssignments,
  listCoachRelationships,
  listGoals,
  listCheckins,
  listSessions,
} from '@/api/gym';
import {
  attendanceCalendar,
  formatDuration,
  summarise,
  todayKey,
} from '@/features/coaching/attendance';
import { sessionNameFor } from '@/features/session/name';
import { can, mayPrescribeFor, useActiveMembership, useSession } from '@/session/store';
import {
  Badge,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  InitialsSquare,
  ListRow,
  LivePill,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** How much history the calendar shows. Six weeks reads as a training block. */
const WINDOW_DAYS = 42;

const shortDate = (key: string) => {
  // Indexed rather than destructured: under `noUncheckedIndexedAccess` a split
  // yields `string | undefined`, and Date does not take undefined.
  const parts = key.split('-');
  const date = new Date(Number(parts[0]), Number(parts[1]) - 1, Number(parts[2]));
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
};

/**
 * One client, everything a coach needs about them.
 *
 * This screen is the hole the app had: the People tab listed clients as flat,
 * un-tappable rows, so a trainer could see that Sara existed and nothing else —
 * not her sessions, not her history, not her measurements. Every read here
 * already existed as an endpoint; there was simply no screen calling them.
 *
 * The attendance strip deliberately distinguishes **trained** from **came in**.
 * A door scan and a logged workout are different facts, and merging them would
 * misreport adherence in both directions: someone who scans in and leaves would
 * look like they trained, and someone who trains at home would look absent.
 */
export default function AthleteDetail() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];
  const myId = useSession((st) => st.user?.id);
  const { id, name } = useLocalSearchParams<{ id: string; name?: string }>();

  const athleteName = name ?? 'Member';

  /*
    Whether I may prescribe for THIS athlete is a relationship question, not a
    capacity one (ADR-0034): their own coach, or themselves. A manager writes
    the catalogue and decides who coaches whom, and does not reach past the
    trainer — so gating this row on a capacity would show it to exactly the
    wrong person.
  */
  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId),
  });

  const sessions = useQuery({
    queryKey: ['sessions', gymId, id],
    queryFn: () => listSessions(gymId!, { athleteId: id, limit: 200 }),
    enabled: Boolean(gymId && id),
  });

  const checkIns = useQuery({
    queryKey: ['checkins', gymId],
    queryFn: () => listCheckins(gymId!),
    // Front-desk read: coaches have it, plain members do not. Asking anyway
    // would paint a 403 banner over a screen that is otherwise fine.
    enabled: Boolean(gymId) && can.coach(capacities),
  });

  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });

  const goals = useQuery({
    queryKey: ['goals', gymId],
    queryFn: () => listGoals(gymId!),
    enabled: Boolean(gymId),
  });

  const profile = useQuery({
    queryKey: ['athlete-profile', gymId, id],
    queryFn: () => getAthleteProfileOf(gymId!, id),
    enabled: Boolean(gymId && id),
    // A member who has never filled one in is the normal case, not an error.
    retry: false,
  });

  const measurements = useQuery({
    queryKey: ['measurements', gymId, id],
    queryFn: () => getMeasurementsOf(gymId!, id),
    enabled: Boolean(gymId && id),
    retry: false,
  });

  const calendar = useMemo(
    () =>
      attendanceCalendar(
        sessions.data ?? [],
        (checkIns.data ?? []).filter((c: { member_id: string }) => c.member_id === id),
        WINDOW_DAYS,
      ),
    [sessions.data, checkIns.data, id],
  );

  const summary = useMemo(() => summarise(calendar), [calendar]);

  const mayPrescribe = mayPrescribeFor(myId, String(id), relationships.data ?? []);

  const activeAssignment = (assignments.data ?? []).find(
    (a) => a.athlete_id === id && a.is_active,
  );
  const theirGoals = (goals.data ?? []).filter((g) => g.athlete_id === id);
  // Newest first is not guaranteed by the endpoint, so pick explicitly rather
  // than trusting an order and quietly showing a year-old weight.
  const latest = [...(measurements.data ?? [])].sort((a, b) =>
    b.measured_on.localeCompare(a.measured_on),
  )[0];

  // Sessions with something in them, newest first — the calendar already
  // carries the shape of the block, so this is the detail underneath it.
  const recent = useMemo(
    () => [...calendar].reverse().flatMap((d) => d.sessions),
    [calendar],
  );

  if (sessions.isLoading) {
    return (
      <Screen edges={['bottom']}>
        <Stack.Screen options={{ title: athleteName }} />
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      </Screen>
    );
  }

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: athleteName }} />

      {sessions.isError ? <ErrorBanner message="Could not load their training." /> : null}

      <View style={s.head}>
        <InitialsSquare name={athleteName} size={48} />
        <View style={s.headBody}>
          <Text style={s.name}>{athleteName}</Text>
          <Text style={s.meta}>
            {activeAssignment
              ? (activeAssignment.program_name ?? 'On a programme')
              : 'No programme assigned'}
          </Text>
        </View>
      </View>

      {/* The four numbers a coach checks before anything else. */}
      <View style={s.stats}>
        <Stat label="Sessions" value={String(summary.sessions)} hint="last 6 weeks" />
        <Stat
          label="Trained"
          value={formatDuration(summary.seconds)}
          hint="total"
        />
        <Stat
          label="Average"
          value={formatDuration(summary.averageSeconds)}
          hint="per session"
        />
        <Stat
          label="Last seen"
          value={
            summary.daysSinceLast == null
              ? '—'
              : summary.daysSinceLast === 0
                ? 'Today'
                : `${summary.daysSinceLast}d`
          }
          hint={summary.daysSinceLast == null ? 'never trained' : 'ago'}
          alert={summary.daysSinceLast != null && summary.daysSinceLast >= 7}
        />
      </View>

      <View>
        <Section label="Attendance" meta="6 weeks" />
        {/* One cell per day, gaps included — the gaps are what a coach is
            looking for. Filled = trained; outlined = came through the door
            without logging anything. */}
        <View style={s.grid}>
          {calendar.map((day) => (
            <View
              key={day.date}
              accessibilityLabel={
                day.trained
                  ? `${shortDate(day.date)}: trained ${formatDuration(day.seconds)}`
                  : day.attended
                    ? `${shortDate(day.date)}: came in, logged nothing`
                    : `${shortDate(day.date)}: nothing`
              }
              style={[
                s.cell,
                day.attended && s.cellAttended,
                day.trained && s.cellTrained,
                day.date === todayKey() && s.cellToday,
              ]}
            />
          ))}
        </View>
        <View style={s.legend}>
          <View style={s.legendItem}>
            <View style={[s.cell, s.cellTrained, s.legendSwatch]} />
            <Text style={s.legendText}>Trained</Text>
          </View>
          <View style={s.legendItem}>
            <View style={[s.cell, s.cellAttended, s.legendSwatch]} />
            <Text style={s.legendText}>Came in, logged nothing</Text>
          </View>
        </View>
      </View>

      {theirGoals.length > 0 ? (
        <View>
          <Section label="Goals" count={theirGoals.length} />
          {theirGoals.map((goal, i) => (
            <ListRow
              key={goal.id}
              title={
                goal.metric.kind === 'bodyweight'
                  ? `Bodyweight to ${goal.metric.target_kg} kg`
                  : `Lift ${goal.metric.target_kg} kg`
              }
              subtitle={`from ${goal.metric.baseline_kg} kg${
                goal.target_date ? ` · by ${goal.target_date}` : ''
              }`}
              last={i === theirGoals.length - 1}
              right={goal.is_active ? undefined : <Badge tone="muted">Closed</Badge>}
            />
          ))}
        </View>
      ) : null}

      {latest || profile.data ? (
        <View>
          <Section label="Body" />
          {profile.data?.height_cm ? (
            <ListRow title="Height" subtitle={`${profile.data.height_cm} cm`} />
          ) : null}
          {latest?.weight_kg != null ? (
            <ListRow
              title="Latest weight"
              subtitle={`${latest.weight_kg} kg · ${latest.measured_on}`}
              last
            />
          ) : null}
        </View>
      ) : null}

      <View>
        <Section label="Sessions" count={recent.length} />
        {recent.length === 0 ? (
          <EmptyState
            glyph="○"
            title="No training logged"
            hint={
              activeAssignment
                ? 'They are on a programme but have not logged a workout in this window.'
                : 'Assign them a programme and their sessions will appear here.'
            }
          />
        ) : (
          recent.map((session, i) => (
            <ListRow
              key={session.id}
              title={sessionNameFor(session)}
              subtitle={[
                new Date(session.started_at).toLocaleDateString(undefined, {
                  weekday: 'short',
                  day: 'numeric',
                  month: 'short',
                }),
                formatDuration(session.duration_seconds),
                `${session.set_count ?? 0} sets`,
              ].join(' · ')}
              last={i === recent.length - 1}
              right={
                session.status.state === 'abandoned' ? (
                  <Badge tone="muted">Cut short</Badge>
                ) : session.is_open ? (
                  <LivePill />
                ) : undefined
              }
              onPress={() =>
                router.push({
                  pathname: '/(app)/session/[id]',
                  params: { id: session.id },
                })
              }
            />
          ))
        )}
      </View>

      {/* The action a coach comes here to take. Offered to anyone who may
          coach this person — the server re-checks the relationship. */}
      {mayPrescribe || can.setCapacities(capacities) ? (
        <View>
          <Section label="Coaching" />
          <Card padded={false}>
            {mayPrescribe ? (
              <ListRow
                title={activeAssignment ? 'Change their programme' : 'Put them on a programme'}
                subtitle={
                  activeAssignment
                    ? `On ${activeAssignment.program_name ?? 'a programme'} — pick a different published version`
                    : 'Pick a published version and they see it on Today straight away'
                }
                last={!can.setCapacities(capacities)}
                onPress={() =>
                  router.push({
                    pathname: '/(app)/assign-program-for',
                    params: { athlete: String(id), name: String(name ?? '') },
                  })
                }
              />
            ) : null}
            {/* Standing is what invitations became (ADR-0031) — this is where
                a member becomes a trainer. */}
            {can.setCapacities(capacities) ? (
              <ListRow
                title="Standing"
                subtitle="What they hold at this gym"
                last
                onPress={() =>
                  router.push({
                    pathname: '/(app)/standing',
                    params: { id: String(id), name: String(name ?? '') },
                  })
                }
              />
            ) : null}
          </Card>
        </View>
      ) : null}
    </Screen>
  );
}

function Stat({
  label,
  value,
  hint,
  alert,
}: {
  label: string;
  value: string;
  hint: string;
  alert?: boolean;
}) {
  const s = useStyles(styleFactory);
  return (
    <View style={s.stat}>
      <Text style={s.statLabel}>{label}</Text>
      <Text style={[s.statValue, alert && s.statValueAlert]}>{value}</Text>
      <Text style={s.statHint}>{hint}</Text>
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    head: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    headBody: { flex: 1, gap: 3 },
    name: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 30,
    },
    meta: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm },

    stats: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      overflow: 'hidden',
      ...t.elevation(1),
    },
    stat: {
      borderRightColor: t.color.line,
      borderRightWidth: t.border.hair,
      flex: 1,
      gap: 2,
      paddingHorizontal: t.space.md,
      paddingVertical: t.space.md,
    },
    statLabel: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
    statValue: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    statValueAlert: { color: t.color.accentDeep },
    statHint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xxs },

    grid: { flexDirection: 'row', flexWrap: 'wrap', gap: 5, paddingVertical: t.space.sm },
    cell: {
      backgroundColor: t.color.track,
      borderRadius: t.radius.xs,
      height: 18,
      width: 18,
    },
    // Outlined, not filled: they were here, but there is no training to show.
    cellAttended: {
      backgroundColor: t.color.accentHi,
      borderColor: t.color.accent,
      borderWidth: t.border.hair,
    },
    cellTrained: { backgroundColor: t.color.accent, borderWidth: 0 },
    cellToday: { borderColor: t.color.ink, borderWidth: t.border.ink },
    legend: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.lg, paddingTop: t.space.xs },
    legendItem: { alignItems: 'center', flexDirection: 'row', gap: 6 },
    legendSwatch: { borderRadius: t.radius.xs, height: 12, width: 12 },
    legendText: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
  });
