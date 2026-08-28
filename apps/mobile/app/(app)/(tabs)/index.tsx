import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import {
  closeGoal,
  getExerciseHistory,
  getRecommendations,
  getVersionContent,
  listAssignments,
  answerCoachingRequest,
  listClasses,
  listCoachingRequests,
  listCoachRelationships,
  listExercises,
  listGoals,
  listMeasurements,
  listSessions,
  type Assignment,
  type Goal,
  type WorkoutSession,
} from '@/api/gym';
import {
  distinctClasses,
  mine as myBookings,
  occupancy,
  on as sittingsOn,
  taughtBy,
  timeLabel,
  todayLocal,
  windowEnd,
  type Sitting,
} from '@/features/classes/timetable';
import { clientsNeedingAttention, trainingNow } from '@/features/coaching/attention';
import { nextWorkout } from '@/features/programs/next-workout';
import { sessionAge } from '@/features/session/age';
import { sessionNameFor } from '@/features/session/name';
import { goalProgress, sessionPoints } from '@/features/progress/metrics';
import { can, useSession } from '@/session/store';
import {
  Appear,
  Button,
  Card,
  IconButton,
  ListRow,
  LivePill,
  Ring,
  Section,
  StatRow,
  Touchable,
} from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** "Sun 20 Jul" — the kicker's right-hand meta. */
const dateLabel = (now: Date) =>
  now.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });

/** "started 12 July", "starts tomorrow" — the tense matters more than the date. */
function startLabel(startDate: string, now: Date): string {
  const start = new Date(`${startDate}T00:00:00`);
  if (Number.isNaN(start.getTime())) return '';
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((start.getTime() - today.getTime()) / 86_400_000);
  if (days > 1) return `starts in ${days} days`;
  if (days === 1) return 'starts tomorrow';
  if (days === 0) return 'starts today';
  if (days === -1) return 'started yesterday';
  return `started ${start.toLocaleDateString(undefined, { day: 'numeric', month: 'long' })}`;
}

export default function Today() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const tabBarHeight = useTabBarHeight();
  const queryClient = useQueryClient();
  const now = new Date();

  const user = useSession((st) => st.user);
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const capacities = useSession((st) => st.membership?.capacities ?? []);
  const coaches = can.coach(capacities);

  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });
  /*
    The class timetable, one week out. Everybody gets it: a member books from
    it, a trainer finds their own classes in it, an owner reads how full the
    place is. One query, three slices below — the slicing is pure and tested
    (scripts/verify-timetable.mjs).
  */
  const classWindowFrom = todayLocal(now);
  const classWindowTo = windowEnd(classWindowFrom, 6);
  const classes = useQuery({
    queryKey: ['classes', gymId, classWindowFrom, classWindowTo],
    queryFn: () => listClasses(gymId!, classWindowFrom, classWindowTo),
    enabled: Boolean(gymId),
  });

  /*
    Proposals waiting on this trainer (ADR-0034).

    The gym names a pairing and the trainer accepts — and until they do, nothing
    exists. So this has to be somewhere they will actually see it, which is
    here: a proposal nobody looks at is the reason the old handshake was removed
    in the first place, and re-adding one without an inbox would repeat that.
  */
  const requests = useQuery({
    queryKey: ['coaching-requests', gymId],
    queryFn: () => listCoachingRequests(gymId!),
    enabled: Boolean(gymId) && coaches,
  });

  const sessions = useQuery({
    queryKey: ['sessions', gymId],
    queryFn: () => listSessions(gymId!),
    enabled: Boolean(gymId),
  });

  const mine = (assignments.data ?? []).filter(
    (a: Assignment) => a.athlete_id === user?.id && a.is_active,
  );
  const mySessions = (sessions.data ?? []).filter(
    (x: WorkoutSession) => x.athlete_id === user?.id,
  );
  const openSession = mySessions.find((x) => x.is_open);

  // Only asked for when it can change what the screen says: an open session
  // already answers "what now", so the programme's content is not needed.
  const focusAssignment = mine[0];
  const content = useQuery({
    queryKey: ['version-content', gymId, focusAssignment?.program_version_id],
    queryFn: () => getVersionContent(gymId!, focusAssignment!.program_version_id),
    enabled: Boolean(gymId && focusAssignment && !openSession),
  });

  const upNext =
    !openSession && content.data
      ? nextWorkout(
          content.data.weeks,
          content.data.workouts,
          // Unplanned sessions (ADR-0035) have no workout to have been done,
          // so they say nothing about where the member is in a programme.
          // Dropping them here is what keeps "up next" pointing at the right
          // day for somebody who trains their own way between coached ones.
          mySessions
            .filter((x) => !x.is_open)
            .map((x) => x.workout_template_id)
            .filter((id): id is string => id != null),
        )
      : null;

  /*
    Assigned, but there is no workout to point at yet.

    `nextWorkout` returns null when the programme has weeks and no workouts in
    them — a real state, because a coach can publish a skeleton and fill it in.
    Without this the member's Today went completely blank the moment they were
    assigned something, which reads as "the app did not notice" rather than
    "your coach has not written this week yet". The card is the same shape as
    the real one so the screen does not reflow when the workouts land.
  */
  const assignedButEmpty = Boolean(
    !openSession && focusAssignment && content.data && !upNext,
  );

  const goals = useQuery({
    queryKey: ['goals', gymId],
    queryFn: () => listGoals(gymId!),
    enabled: Boolean(gymId),
  });
  const myGoals = (goals.data ?? [])
    .filter((g: Goal) => g.athlete_id === user?.id && g.is_active)
    .slice(0, 2);

  const recommendations = useQuery({
    queryKey: ['recommendations', gymId],
    queryFn: () => getRecommendations(gymId!),
    enabled: Boolean(gymId) && myGoals.length > 0,
  });
  const suggestedPrograms = (recommendations.data?.programs ?? []).slice(0, 2);
  const suggestedTrainers = (recommendations.data?.trainers ?? []).slice(0, 1);

  // ---- coaching side -----------------------------------------------------

  const relationships = useQuery({
    queryKey: ['coach-relationships', gymId],
    queryFn: () => listCoachRelationships(gymId!),
    enabled: Boolean(gymId) && coaches,
  });
  const myClients = (relationships.data ?? [])
    .filter((r) => r.coach_id === user?.id && r.is_active)
    .map((r) => ({ athleteId: r.athlete_id, athleteName: r.athlete_name ?? 'Athlete' }));

  // ---- proposals waiting on me -------------------------------------------
  //
  // Mine to answer: addressed to me, still pending. A manager's own proposal
  // never appears here for them — they cannot answer it, which is the point.
  const myProposals = (requests.data ?? []).filter(
    (r) => r.is_pending && r.is_proposal && r.coach_id === user?.id,
  );

  const answer = useMutation({
    mutationFn: ({ id, decision }: { id: string; decision: 'accept' | 'decline' }) =>
      answerCoachingRequest(gymId!, id, decision),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['coaching-requests', gymId] });
      // Accepting creates the pairing, so the client list changes too.
      void queryClient.invalidateQueries({ queryKey: ['coach-relationships', gymId] });
    },
  });

  // ---- classes ------------------------------------------------------------
  //
  // Three slices of one list, because the three roles want different questions
  // answered off the same timetable: "what can I book", "what am I teaching",
  // "how full is the place".
  const sittings = (classes.data ?? []) as Sitting[];
  const classesToday = sittingsOn(sittings, classWindowFrom);
  const myClassBookings = myBookings(sittings);
  const myTeaching = user?.id ? taughtBy(sittings, user.id) : [];
  const myTeachingToday = myTeaching.filter((x) => x.on_date === classWindowFrom);
  const classOccupancy = occupancy(sittings);
  const classCount = distinctClasses(sittings);

  const attention = coaches
    ? clientsNeedingAttention(myClients, assignments.data ?? [], sessions.data ?? [], now)
    : [];
  const live = coaches ? trainingNow(myClients, sessions.data ?? []) : [];
  const trainedThisWeek = coaches
    ? new Set(
        (sessions.data ?? [])
          .filter(
            (x) =>
              !x.is_open &&
              myClients.some((c) => c.athleteId === x.athlete_id) &&
              now.getTime() - new Date(x.started_at).getTime() < 7 * 86_400_000,
          )
          .map((x) => x.athlete_id),
      ).size
    : 0;

  return (
    <ScrollView
      style={s.screen}
      contentContainerStyle={[
        s.content,
        { paddingBottom: tabBarHeight + t.space.xl, paddingTop: insets.top + t.space.md },
      ]}
      showsVerticalScrollIndicator={false}
    >
      <GymHeader
        title={coaches ? 'Your floor' : 'Today'}
        meta={dateLabel(now)}
        /*
          The entry pass lives up here rather than in the content.
          Everybody has one and nobody reads it — they hold the phone up at a
          door — so it wants to be a fixed, findable target, not a row that
          moves down the screen as the day fills up.
        */
        action={
          <IconButton
            icon="maximize"
            label="Show your entry pass"
            onPress={() => router.push('/(app)/entry-pass')}
          />
        }
      />

      {/*
        The one focal element. A workout in progress outranks everything; with
        nothing open, the next workout in the pinned programme takes its place,
        so the screen answers "what now" instead of listing what exists.
      */}
      {openSession ? (
        (() => {
          // A workout left open for days is a leftover, not a workout in
          // progress — say so rather than running the clock past absurdity.
          const age = sessionAge(openSession.started_at, now);
          return (
            <Appear>
              <Touchable
                onPress={() =>
                  router.push({ pathname: '/(app)/session/[id]', params: { id: openSession.id } })
                }
                accessibilityLabel={`${age.stale ? 'Resume the workout you left open' : 'Continue'} ${sessionNameFor(openSession, 'your workout')}, ${openSession.set_count ?? 0} sets logged`}
                style={s.focal}
              >
                <View style={s.focalTopRow}>
                  {age.stale ? (
                    <Text style={s.focalKicker} numberOfLines={1}>
                      Left open · {age.label}
                    </Text>
                  ) : (
                    <LivePill>{age.label} in</LivePill>
                  )}
                  <Text style={s.focalKickerRight} numberOfLines={1}>
                    {`${openSession.set_count ?? 0} ${openSession.set_count === 1 ? 'set' : 'sets'}`}
                  </Text>
                </View>
                <Text style={s.focalTitle} numberOfLines={2}>
                  {sessionNameFor(openSession)}
                </Text>
                <View style={s.focalFootRow}>
                  <Text style={s.focalFoot} numberOfLines={1}>
                    {age.stale ? 'Finish it or discard it to start fresh' : 'Pick up where you left off'}
                  </Text>
                  <View style={s.focalCta}>
                    <Text style={s.focalCtaText}>{age.stale ? 'Resume' : 'Continue'}</Text>
                    <Feather name="arrow-right" size={15} color={t.color.accent} />
                  </View>
                </View>
              </Touchable>
            </Appear>
          );
        })()
      ) : assignedButEmpty ? (
        <Appear>
          <Touchable
            onPress={() =>
              router.push({
                pathname: '/(app)/program/[version]',
                params: {
                  version: focusAssignment!.program_version_id,
                  name: focusAssignment!.program_name ?? 'Programme',
                },
              })
            }
            accessibilityLabel={`You are on ${focusAssignment!.program_name ?? 'a programme'}. It has no workouts yet.`}
            style={s.focal}
          >
            <View style={s.focalTopRow}>
              <Text style={s.focalKicker} numberOfLines={1}>
                You are on
              </Text>
            </View>
            <Text style={s.focalTitle} numberOfLines={2}>
              {focusAssignment!.program_name ?? 'Programme'}
            </Text>
            <View style={s.focalFootRow}>
              <Text style={s.focalFoot} numberOfLines={2}>
                No workouts written into it yet — your coach is still filling it in.
              </Text>
              <View style={s.focalCta}>
                <Text style={s.focalCtaText}>Open</Text>
                <Feather name="arrow-right" size={15} color={t.color.accent} />
              </View>
            </View>
          </Touchable>
        </Appear>
      ) : upNext ? (
        <Appear>
          <Touchable
            onPress={() =>
              router.push({
                pathname: '/(app)/program/[version]',
                params: {
                  version: focusAssignment!.program_version_id,
                  name: focusAssignment!.program_name ?? 'Programme',
                },
              })
            }
            accessibilityLabel={`Next workout: ${upNext.name}, week ${upNext.weekNumber} day ${upNext.dayNumber}. Open your programme to start it.`}
            style={s.focal}
          >
            <View style={s.focalTopRow}>
              <Text style={s.focalKicker} numberOfLines={1}>
                {upNext.cycleRestarted ? 'Starting again' : 'Up next'}
              </Text>
              <Text style={s.focalKickerRight} numberOfLines={1}>
                {`Week ${upNext.weekNumber} · Day ${upNext.dayNumber}`}
              </Text>
            </View>
            <Text style={s.focalTitle} numberOfLines={2}>
              {upNext.name}
            </Text>
            <View style={s.focalProgress}>
              <View style={s.focalTrack}>
                <View
                  style={[
                    s.focalFill,
                    { width: `${Math.round((upNext.completed / Math.max(upNext.total, 1)) * 100)}%` },
                  ]}
                />
              </View>
              <Text style={s.focalCount}>
                {upNext.completed}/{upNext.total}
              </Text>
            </View>
            <View style={s.focalFootRow}>
              <Text style={s.focalFoot} numberOfLines={1}>
                {focusAssignment?.program_name ?? 'Programme'}
              </Text>
              <View style={s.focalCta}>
                <Text style={s.focalCtaText}>Open</Text>
                <Feather name="arrow-right" size={15} color={t.color.accent} />
              </View>
            </View>
          </Touchable>
        </Appear>
      ) : null}

      {/* ---- coaching ---- */}

      {coaches ? (
        <>
          <Appear index={1}>
            <StatRow
              stats={[
                { label: 'Clients', value: String(myClients.length) },
                { label: 'Trained · 7d', value: String(trainedThisWeek) },
                {
                  label: 'Needs you',
                  value: String(attention.length),
                  alert: attention.length > 0,
                },
              ]}
            />
          </Appear>

          {live.length > 0 ? (
            <View style={s.group}>
              <Section label="On the floor now" count={live.length} />
              <Card padded={false}>
                {live.map((c, i) => {
                  const session = (sessions.data ?? []).find(
                    (x) => x.athlete_id === c.athleteId && x.is_open,
                  );
                  const age = session ? sessionAge(session.started_at, now) : null;
                  return (
                    <ListRow
                      key={c.athleteId}
                      title={c.athleteName}
                      subtitle={
                        session && age
                          ? age.stale
                            ? `${sessionNameFor(session)} · left open ${age.label.replace('opened ', '')}`
                            : `${sessionNameFor(session)} · ${age.label} in · ${session.set_count ?? 0} sets`
                          : undefined
                      }
                      last={i === live.length - 1}
                      right={age && !age.stale ? <LivePill /> : undefined}
                    />
                  );
                })}
              </Card>
            </View>
          ) : null}

          {attention.length > 0 ? (
            <View style={s.group}>
              <Section label="Needs you" count={attention.length} />
              <Card padded={false}>
                {attention.slice(0, 4).map((item, i) => (
                  <ListRow
                    key={item.athleteId}
                    title={item.athleteName}
                    subtitle={item.because}
                    subtitleTone="reason"
                    last={i === Math.min(attention.length, 4) - 1}
                    onPress={() => router.push('/(app)/(tabs)/people')}
                  />
                ))}
              </Card>
            </View>
          ) : null}
        </>
      ) : null}

      {/* ---- the member's own training ---- */}

      {/*
        Goals are the one thing a member sets for themselves, so the empty state
        is an invitation rather than a blank: with no goal there are also no
        recommendations, and "nothing suggested" reads as a broken app unless
        you are told why.
      */}
      <View style={s.group}>
        <Section
          label="Your goals"
          meta={myGoals.length > 0 ? undefined : 'none set'}
          count={myGoals.length > 0 ? myGoals.length : undefined}
        />
        {myGoals.length > 0 ? (
          <View style={s.goalGrid}>
            {myGoals.map((goal, index) => (
              <Appear key={goal.id} index={index}>
                <GoalTile goal={goal} gymId={gymId!} />
              </Appear>
            ))}
          </View>
        ) : null}
        <Card padded={false} style={s.groupCard}>
          <ListRow
            title={myGoals.length > 0 ? 'Set another goal' : 'Set your first goal'}
            subtitle={
              myGoals.length > 0
                ? 'A bodyweight target, or a lift you are chasing'
                : 'Pick a target and this screen starts tracking it — and the suggestions below get something to work from'
            }
            last
            onPress={() => router.push('/(app)/new-goal')}
          />
        </Card>
      </View>

      {/*
        ---- a pairing waiting on me ---------------------------------------
        The gym has named this trainer for somebody, and until the trainer
        answers there is no pairing and no access. Placed high and shown with
        both answers, because a proposal nobody notices is exactly why the old
        handshake was removed — adding one back without somewhere to see it
        would repeat that mistake.
      */}
      {myProposals.length > 0 ? (
        <View style={s.group}>
          <Section
            label={myProposals.length === 1 ? 'A new client for you' : 'New clients for you'}
            count={myProposals.length}
          />
          <Card padded={false}>
            {myProposals.map((req, i) => (
              <View
                key={req.id}
                style={[s.proposal, i < myProposals.length - 1 && s.proposalLine]}
              >
                <Text style={s.proposalName}>{req.athlete_name || 'A member'}</Text>
                <Text style={s.proposalSays}>
                  {req.message?.trim()
                    ? req.message
                    : 'The gym has asked you to coach them. Accepting lets you see their sessions, goals and body trend, and put them on a programme.'}
                </Text>
                <View style={s.proposalActions}>
                  <Button
                    label="Accept"
                    size="small"
                    busy={answer.isPending}
                    onPress={() => answer.mutate({ id: req.id, decision: 'accept' })}
                  />
                  <Button
                    label="Not me"
                    size="small"
                    variant="ghost"
                    disabled={answer.isPending}
                    onPress={() => answer.mutate({ id: req.id, decision: 'decline' })}
                  />
                </View>
              </View>
            ))}
          </Card>
        </View>
      ) : null}

      {/*
        ---- classes -------------------------------------------------------
        One timetable, three readings. A member sees what they can book and
        what they already hold; the instructor sees what they are teaching
        today; the owner sees how full the place is. All three land on the same
        screen for the detail, so there is one timetable in the product rather
        than three that can disagree.
      */}
      {sittings.length > 0 ? (
        <View style={s.group}>
          <Section
            label="Classes"
            meta={classCount === 1 ? '1 class' : `${classCount} classes`}
            action={
              <IconButton
                icon="calendar"
                label="See the whole timetable"
                onPress={() => router.push('/(app)/classes')}
              />
            }
          />

          {/* The instructor's own first — it is the thing they came to check. */}
          {myTeachingToday.length > 0 ? (
            <Card padded={false} style={s.groupCard}>
              {myTeachingToday.map((sitting, i) => (
                <ListRow
                  key={`teach-${sitting.class_id}-${sitting.on_date}`}
                  title={`${timeLabel(sitting.starts_at)} · ${sitting.name}`}
                  subtitle={`You are teaching · ${sitting.booked} of ${sitting.capacity} booked`}
                  subtitleTone="reason"
                  last={i === myTeachingToday.length - 1}
                  onPress={() =>
                    router.push({
                      pathname: '/(app)/class-roster',
                      params: {
                        classId: sitting.class_id,
                        onDate: sitting.on_date,
                        name: sitting.name,
                      },
                    })
                  }
                />
              ))}
            </Card>
          ) : null}

          {/* Places this member already holds — the reason to open Today. */}
          {myClassBookings.length > 0 ? (
            <Card padded={false} style={s.groupCard}>
              {myClassBookings.slice(0, 3).map((sitting, i, arr) => (
                <ListRow
                  key={`booked-${sitting.class_id}-${sitting.on_date}`}
                  title={sitting.name}
                  subtitle={`Booked · ${sitting.weekday_name} ${timeLabel(sitting.starts_at)} · ${sitting.instructor_name}`}
                  last={i === arr.length - 1}
                  onPress={() => router.push('/(app)/classes')}
                />
              ))}
            </Card>
          ) : null}

          {/* What is on today, for anyone not already booked into it. */}
          {classesToday.filter((x) => !x.booked_by_me).length > 0 ? (
            <Card padded={false} style={s.groupCard}>
              {classesToday
                .filter((x) => !x.booked_by_me)
                .map((sitting, i, arr) => (
                  <ListRow
                    key={`today-${sitting.class_id}`}
                    title={`${timeLabel(sitting.starts_at)} · ${sitting.name}`}
                    subtitle={
                      sitting.is_full
                        ? `Full · ${sitting.instructor_name}`
                        : `${sitting.places_left} places left · ${sitting.instructor_name}`
                    }
                    last={i === arr.length - 1}
                    onPress={() => router.push('/(app)/classes')}
                  />
                ))}
            </Card>
          ) : null}

          {/* The owner's one-glance number. Places taken over places offered,
              not a mean of rates — see the note on `occupancy`. */}
          {can.manageGym(capacities) ? (
            <Card>
              <StatRow
                stats={[
                  { label: 'On the timetable', value: `${classCount}` },
                  {
                    label: 'Occupancy',
                    value: `${Math.round(classOccupancy * 100)}`,
                    unit: '%',
                    context: 'places taken this week',
                  },
                ]}
              />
            </Card>
          ) : null}
        </View>
      ) : can.manageGym(capacities) ? (
        <View style={s.group}>
          <Section label="Classes" />
          <Card padded={false} style={s.groupCard}>
            <ListRow
              title="Put a class on the timetable"
              subtitle="Zumba, yoga, a cardio session — members can book a place from their phone"
              last
              onPress={() => router.push('/(app)/new-class')}
            />
          </Card>
        </View>
      ) : null}

      {mine.length > 0 ? (
        <View style={s.group}>
          <Section label={mine.length === 1 ? 'Your programme' : 'Your programmes'} />
          <Card padded={false}>
            {mine.map((assignment, index) => (
              <ListRow
                key={assignment.id}
                title={assignment.program_name ?? 'Programme'}
                subtitle={`v${assignment.version_number ?? 1} · ${startLabel(assignment.start_date, now)}`}
                last={index === mine.length - 1}
                onPress={() =>
                  router.push({
                    pathname: '/(app)/program/[version]',
                    params: {
                      // The PINNED version — not "latest" (ADR-0006).
                      version: assignment.program_version_id,
                      name: assignment.program_name ?? 'Programme',
                    },
                  })
                }
              />
            ))}
          </Card>
        </View>
      ) : null}

      {suggestedPrograms.length > 0 || suggestedTrainers.length > 0 ? (
        <View style={s.group}>
          <Section label="Suggested for you" meta="from your goals" />
          <Card padded={false}>
            {suggestedPrograms.map((suggestion) => (
              <ListRow
                key={suggestion.program_id}
                title={suggestion.name}
                subtitle={suggestion.because}
                subtitleTone="reason"
                onPress={() =>
                  router.push({
                    pathname: '/(app)/program/[version]',
                    params: { version: suggestion.version_id, name: suggestion.name },
                  })
                }
              />
            ))}
            {suggestedTrainers.map((trainer, i) => (
              <ListRow
                key={trainer.user_id}
                title={
                  trainer.headline
                    ? `${trainer.display_name} — ${trainer.headline}`
                    : trainer.display_name
                }
                subtitle={trainer.because}
                subtitleTone="reason"
                last={i === suggestedTrainers.length - 1}
              />
            ))}
          </Card>
          <Text style={s.footnote}>Ask at the desk to be paired with a coach.</Text>
        </View>
      ) : null}

      {/*
        Quick actions exist for coaches and managers, and one of them is
        load-bearing: a manager trades the Library TAB for Billing (five is the
        ceiling — see navigation/tabs.ts), so this is their route to the
        catalogue. verify-nav pins the trade; removing this would strand the
        only people who can author exercises.

        A grid rather than a list: these are five unrelated destinations of
        equal weight, and a list implies an order they do not have.
      */}
      {coaches ? (
        <View style={s.group}>
          <Section label="Jump to" />
          <View style={s.tiles}>
            {/*
              Programmes first: writing them is the job this product exists
              for, and burying it under the exercise catalogue got the ordering
              backwards — a movement is an ingredient, a programme is the meal.
            */}
            <ActionTile
              title="Programmes"
              hint={
                can.publishPrograms(capacities)
                  ? 'Write, publish, assign'
                  : can.authorPrograms(capacities)
                    ? 'Write one for review'
                    : 'What this gym publishes'
              }
              icon="layers"
              onPress={() => router.push('/(app)/programs')}
            />
            <ActionTile
              title="Scan entry"
              hint="Check a pass at the door"
              icon="check-square"
              onPress={() => router.push('/(app)/scan-entry')}
            />
            {can.proposeExercises(capacities) ? (
              <ActionTile
                title="New movement"
                hint={
                  can.curateCatalogue(capacities)
                    ? 'Add it to the catalogue'
                    : 'Propose it — usable today'
                }
                icon="plus-square"
                onPress={() => router.push('/(app)/new-exercise')}
              />
            ) : null}
            {can.manageGym(capacities) ? (
              <ActionTile
                title="Manage gym"
                hint="Joining, catalogue, invitations"
                icon="sliders"
                onPress={() => router.push('/(app)/manage')}
              />
            ) : null}
            <ActionTile
              title="Pair a coach"
              hint={
                can.manageCatalogue(capacities)
                  ? 'Open an athlete to a coach'
                  : 'Ask your head coach'
              }
              icon="link"
              onPress={() =>
                can.manageCatalogue(capacities)
                  ? router.push('/(app)/assign-coach')
                  : router.push('/(app)/(tabs)/people')
              }
            />
          </View>
        </View>
      ) : null}

      {/*
        Train on your own (ADR-0035).

        Offered to any member without a session already running — not only to
        those with no programme. Somebody on a coached plan still does the odd
        extra session, and hiding this from them would say the app only counts
        training a coach set, which is not what the history is for.

        Placed after "up next" deliberately: for a coached member the
        prescribed workout is the answer to "what now", and this is the
        alternative. For an Open Gym member there is no "up next", so this is
        the first thing on the screen that does anything.
      */}
      {!openSession && !coaches ? (
        <Appear index={mine.length === 0 ? 0 : 3}>
          <View style={s.group}>
            <Section label={mine.length === 0 ? 'Train today' : 'Something else'} />
            <Card>
              <Text style={s.startTitle}>
                {mine.length === 0 ? 'Build your own workout' : 'Training off-plan today?'}
              </Text>
              <Text style={s.startBody}>
                {mine.length === 0
                  ? 'Pick your exercises, log your sets, and it all counts towards your history and your goals — the same as any coached session.'
                  : 'Start a workout of your own. It is logged alongside your programme rather than against it, so it will not move you through your weeks.'}
              </Text>
              <Button
                label="Start your own workout"
                variant={mine.length === 0 ? 'primary' : 'secondary'}
                onPress={() => router.push('/(app)/new-session')}
              />
            </Card>
          </View>
        </Appear>
      ) : null}

      {!openSession && !upNext && mine.length === 0 && !coaches ? (
        <View style={s.group}>
          <Section label="Getting started" />
          <Card>
            <Text style={s.startTitle}>No coach yet</Text>
            <Text style={s.startBody}>
              A coach can put you on a programme built around your goals — ask at the desk, or
              find one from People. The library also has every movement this gym uses, with your
              own history against each.
            </Text>
          </Card>
        </View>
      ) : null}
    </ScrollView>
  );
}

/**
 * One destination in the coach's grid. Square-ish, icon top-left, so five of
 * them scan as a keypad rather than as five more sentences to read.
 */
function ActionTile({
  title,
  hint,
  icon,
  onPress,
}: {
  title: string;
  hint: string;
  icon: React.ComponentProps<typeof Feather>['name'];
  onPress: () => void;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  return (
    <Touchable
      onPress={onPress}
      accessibilityLabel={`${title}. ${hint}`}
      style={s.tile}
    >
      <Feather name={icon} size={18} color={t.color.accentDeep} />
      <View style={s.tileText}>
        <Text style={s.tileTitle} numberOfLines={1}>
          {title}
        </Text>
        <Text style={s.tileHint} numberOfLines={2}>
          {hint}
        </Text>
      </View>
    </Touchable>
  );
}

/**
 * One goal as a card with a ring. The goal stores baseline + target; the
 * CURRENT value comes fresh from the series it measures, so this can never go
 * stale (ADR-0018).
 */
function GoalTile({ goal, gymId }: { goal: Goal; gymId: string }) {
  const s = useStyles(styleFactory);
  const queryClient = useQueryClient();
  const metric = goal.metric;
  const isLift = metric.kind === 'exercise_est_1rm';

  const measurements = useQuery({
    queryKey: ['measurements'],
    queryFn: listMeasurements,
    enabled: metric.kind === 'bodyweight',
  });
  const history = useQuery({
    queryKey: ['exercise-history', gymId, isLift ? metric.exercise_id : null],
    queryFn: () => getExerciseHistory(gymId, isLift ? metric.exercise_id : ''),
    enabled: isLift,
  });
  const exercises = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId),
    enabled: isLift,
  });

  let current: number | null = null;
  let title = '';

  if (metric.kind === 'bodyweight') {
    current = measurements.data?.find((m) => m.weight_kg != null)?.weight_kg ?? null;
    title =
      metric.target_kg < metric.baseline_kg
        ? `Cut to ${metric.target_kg} kg`
        : `Build to ${metric.target_kg} kg`;
  } else {
    const points = sessionPoints(history.data ?? []);
    current = points.length > 0 ? (points[points.length - 1]?.score ?? null) : null;
    const name = exercises.data?.find((e) => e.id === metric.exercise_id)?.name ?? 'Lift';
    title = `${name} 1RM → ${metric.target_kg} kg`;
  }

  const progress =
    current != null ? goalProgress(metric.baseline_kg, metric.target_kg, current) : null;

  // Closing is a long-press, not a button: the tile is small and a delete-shaped
  // affordance next to a progress ring gets hit by accident. Neither outcome
  // erases anything — a closed goal keeps its history either way.
  const close = useMutation({
    mutationFn: (outcome: 'achieved' | 'abandoned') => closeGoal(gymId, goal.id, outcome),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['goals', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['recommendations', gymId] });
    },
  });

  const reached = progress != null && progress >= 1;

  return (
    <Pressable
      onLongPress={() =>
        Alert.alert(
          title,
          reached
            ? 'You have hit this. Close it off?'
            : 'Close this goal? It stops being tracked; nothing you have logged is lost.',
          [
            { text: 'Keep it', style: 'cancel' },
            { text: 'Achieved', onPress: () => close.mutate('achieved') },
            {
              text: 'Gave up on it',
              style: 'destructive',
              onPress: () => close.mutate('abandoned'),
            },
          ],
        )
      }
      accessibilityRole="button"
      accessibilityLabel={`${title}. ${
        progress != null ? `${Math.round(progress * 100)} per cent there.` : 'No data yet.'
      } Long press to close this goal.`}
      style={s.goalTile}
    >
      <Ring
        fraction={progress ?? 0}
        size={58}
        label={progress != null ? `${Math.round(progress * 100)}` : '—'}
        tone={reached ? 'success' : 'accent'}
      />
      <Text style={s.goalTitle} numberOfLines={2}>
        {title}
      </Text>
      <Text style={s.goalMeta} numberOfLines={1}>
        {close.isPending
          ? 'Closing…'
          : current != null
            ? `${current} kg now · from ${metric.baseline_kg}`
            : 'No data yet'}
      </Text>
    </Pressable>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    content: { gap: t.space.xl, paddingHorizontal: t.space.gutter },
    proposal: { gap: t.space.sm, paddingHorizontal: t.space.lg, paddingVertical: t.space.md },
    proposalLine: {
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
    },
    proposalName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    proposalSays: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 18,
    },
    proposalActions: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    group: { gap: 0 },
    groupCard: { marginTop: t.space.md },

    // The focal card. Accent-filled: on a screen of quiet containers, the one
    // thing you came to do should not need a caption to be found.
    focal: {
      backgroundColor: t.color.accent,
      borderRadius: t.radius.xl,
      gap: t.space.md,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.lg,
      ...t.elevation(3),
    },
    focalTopRow: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.sm,
      justifyContent: 'space-between',
      minHeight: 24,
    },
    focalKicker: {
      color: t.color.onAccent,
      // Shrinks so the right-hand label never wraps: two kickers competing for
      // one row is how "Week 1 · Day 1" ends up on two lines.
      flexShrink: 1,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      opacity: 0.85,
      textTransform: 'uppercase',
    },
    focalKickerRight: {
      color: t.color.onAccent,
      flexShrink: 0,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      opacity: 0.85,
      textTransform: 'uppercase',
    },
    focalTitle: {
      color: t.color.onAccent,
      fontFamily: fonts.displayHeavy,
      fontSize: 27,
      letterSpacing: t.tracking.display,
      lineHeight: 30,
    },
    focalProgress: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    focalTrack: {
      backgroundColor: t.color.accentTrack,
      borderRadius: 999,
      flex: 1,
      height: 6,
      overflow: 'hidden',
    },
    focalFill: { backgroundColor: t.color.onAccent, borderRadius: 999, height: 6 },
    focalCount: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      opacity: 0.85,
    },
    focalFootRow: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.md,
      justifyContent: 'space-between',
    },
    focalFoot: {
      color: t.color.onAccent,
      flexShrink: 1,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      opacity: 0.85,
    },
    // A raised chip on the accent field. The label reads in the accent itself,
    // which is the one place on this card where the brand colour becomes ink.
    focalCta: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderRadius: t.radius.pill,
      flexDirection: 'row',
      gap: 5,
      paddingHorizontal: 13,
      paddingVertical: 7,
    },
    focalCtaText: {
      color: t.color.accent,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      letterSpacing: t.tracking.tight,
    },

    goalGrid: { flexDirection: 'row', gap: t.space.md, paddingTop: t.space.sm },
    goalTile: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      flex: 1,
      gap: t.space.sm,
      minHeight: 150,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    goalTitle: {
      color: t.color.ink,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
      lineHeight: 17,
      marginTop: t.space.xs,
    },
    goalMeta: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      marginTop: 'auto',
    },

    tiles: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.md, paddingTop: t.space.sm },
    tile: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      flexBasis: '47%',
      flexGrow: 1,
      gap: t.space.md,
      minHeight: 104,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    tileText: { gap: 2 },
    tileTitle: {
      color: t.color.ink,
      fontFamily: fonts.semibold,
      fontSize: t.font.md,
    },
    tileHint: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 15,
    },

    startTitle: { color: t.color.ink, fontFamily: fonts.display, fontSize: t.font.lg },
    startBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },

    footnote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
      paddingTop: t.space.md,
    },
  });
