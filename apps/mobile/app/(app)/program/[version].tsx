import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import * as Crypto from 'expo-crypto';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Alert, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  getVersionContent,
  assignProgram,
  listAssignments,
  listPlans,
  listSessions,
  newDraftFrom,
  startSession,
  transitionVersion,
  type Assignment,
  type Transition,
  type VersionContent,
  type WorkoutSession,
} from '@/api/gym';
import { missingReason } from '@/features/billing/entitlements';
import {
  prescriptionLabel,
  weekTitle,
  workoutTitle,
  type Prescription,
} from '@/features/programs/format';
import { can, useActiveMembership, useSession } from '@/session/store';
import {
  Button,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  Pill,
  Screen,
  Touchable,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * A programme version, read the way a member reads it: week by week, day by
 * day, each exercise as a sentence.
 *
 * The screen renders exactly what the server returns — the immutable snapshot
 * the member was assigned. No "latest version" logic here, on purpose: if their
 * coach publishes v2 tomorrow, this screen still shows the v1 they are on
 * (ADR-0006).
 */
export default function ProgramVersionScreen() {
  const t = useTokens();
  const styles = useStyles(styleFactory);
  const { version: versionId, name, programId } = useLocalSearchParams<{
    version: string;
    name?: string;
    /** Present when arrived from the programme list; needed to fork a draft. */
    programId?: string;
  }>();
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((s) => (s.membership?.gymId ?? null));
  const userId = useSession((s) => s.user?.id);
  // Two different refusals, told apart because they need different words: a
  // flaky request is worth retrying, a missing membership never is.
  const [startError, setStartError] = useState<'generic' | 'entitlement' | null>(null);
  const [moveError, setMoveError] = useState<string | null>(null);

  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];
  /*
    Two rights, not one, matching the server after ADR-0024. The line is
    whether a move binds the gym:

      authoring   add weeks/workouts/prescriptions, submit for review,
                  return to draft. Coach-level — a draft commits nobody.
      publishing  approve, publish, archive. Head coach and above, because
                  these are the moves athletes actually feel.

    The review gate on top of that is still NOT a capability: it is the
    domain's rule that nobody approves their own version, which cannot be
    known from capacities alone — so the button is offered and the refusal,
    if it comes, says exactly why.
  */
  const canAuthor = can.authorPrograms(capacities);
  const canApprove = can.publishPrograms(capacities);

  const move = useMutation({
    mutationFn: (to: Transition) => transitionVersion(gymId!, versionId, to),
    onSuccess: () => {
      setMoveError(null);
      void queryClient.invalidateQueries({ queryKey: ['program-version', gymId, versionId] });
      void queryClient.invalidateQueries({ queryKey: ['programs', gymId] });
    },
    onError: (e: Error) => {
      setMoveError(
        e instanceof ApiError && e.message.toLowerCase().includes('self')
          ? 'You wrote this version, so someone else has to approve it. Ask another coach to sign it off.'
          : e instanceof ApiError && e.code === 'auth.forbidden'
            ? 'Your role cannot move a programme through review.'
            : e instanceof ApiError && e.code === 'request.invalid'
              ? e.message
              : 'That change was refused. Reload and check where this version is up to.',
      );
    },
  });

  const draft = useMutation({
    mutationFn: () => newDraftFrom(gymId!, programId!),
    onSuccess: (created) => {
      setMoveError(null);
      void queryClient.invalidateQueries({ queryKey: ['programs', gymId] });
      router.replace({
        pathname: '/(app)/program/[version]',
        params: { version: created.id, name: name ?? '', programId: programId ?? '' },
      });
    },
    onError: () => setMoveError('Could not start a new draft. Please try again.'),
  });

  /*
    Taking a version out of the library.

    Archive, not delete, and the same call for a draft as for a published one —
    `ProgramVersion::archive` is reachable from every state except archived. A
    draft nobody wants is clutter; a published version the gym has stopped
    running should stop being assignable. Neither is a reason to lose the row:
    assignments pin a specific version (ADR-0006) and history has to stay
    readable, so anyone already on it keeps training.
  */
  const archive = useMutation({
    mutationFn: () => transitionVersion(gymId!, versionId, 'archive'),
    onSuccess: () => {
      setMoveError(null);
      void queryClient.invalidateQueries({ queryKey: ['programs', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['program-version', gymId, versionId] });
      // It is out of the library, so this screen is no longer a place to be.
      router.back();
    },
    onError: (e: Error) => {
      setMoveError(
        e instanceof ApiError && e.code === 'auth.forbidden'
          ? 'Only an owner or admin can remove a version from the library.'
          : e instanceof ApiError
            ? e.message
            : 'Could not remove that version. Please try again.',
      );
    },
  });

  /*
    A member putting THEMSELVES on this programme.

    Without it, somebody who joined the gym on their own had no route onto any
    programme: the whole library was readable and none of it was trainable, so
    the app tracked nothing for them. Self-service, and narrow — the server
    allows `athlete == actor` and nothing wider, so this grants no authority
    over anybody else.
  */
  const [startProgramError, setStartProgramError] = useState<string | null>(null);
  const takeOn = useMutation({
    mutationFn: () =>
      assignProgram(gymId!, {
        athleteId: userId!,
        programVersionId: versionId,
        // Today: a member choosing a programme means starting it now.
        startDate: new Date().toISOString().slice(0, 10),
      }),
    onSuccess: () => {
      setStartProgramError(null);
      void queryClient.invalidateQueries({ queryKey: ['assignments', gymId] });
    },
    onError: (e: Error) => {
      setStartProgramError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'You are already on this programme. Pull down to refresh.'
          : e instanceof ApiError
            ? e.message
            : 'Could not start this programme. Please try again.',
      );
    },
  });

  // Fetched only once a start has actually been refused. The price list is a
  // round trip nobody training needs — but a member who has just been stopped
  // deserves to be told which plan would let them in.
  const plans = useQuery({
    queryKey: ['plans', gymId],
    queryFn: () => listPlans(gymId!),
    enabled: Boolean(gymId) && startError === 'entitlement',
  });

  const query = useQuery({
    queryKey: ['program-version', gymId, versionId],
    queryFn: () => getVersionContent(gymId!, versionId),
    enabled: Boolean(gymId && versionId),
  });

  // If *I* am actively assigned this exact version, each workout is startable.
  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });
  const myAssignment = (assignments.data ?? []).find(
    (a: Assignment) =>
      a.athlete_id === userId && a.program_version_id === versionId && a.is_active,
  );

  // An already-open session for a workout turns Start into Continue — starting
  // a second session for the same day is almost always a mistake.
  const sessions = useQuery({
    queryKey: ['sessions', gymId],
    queryFn: () => listSessions(gymId!),
    enabled: Boolean(gymId && myAssignment),
  });
  const openSessionFor = (workoutId: string): WorkoutSession | undefined =>
    (sessions.data ?? []).find(
      (s: WorkoutSession) =>
        s.athlete_id === userId && s.workout_template_id === workoutId && s.is_open,
    );

  const start = useMutation({
    mutationFn: (workoutTemplateId: string) =>
      startSession(gymId!, {
        // Minted here, on the device (ADR-0008): retrying this exact request
        // after a timeout replays the same id and cannot double-start.
        id: Crypto.randomUUID(),
        assignmentId: myAssignment!.id,
        workoutTemplateId,
      }),
    onSuccess: (session) => {
      setStartError(null);
      void queryClient.invalidateQueries({ queryKey: ['sessions', gymId] });
      router.push({ pathname: '/(app)/session/[id]', params: { id: session.id } });
    },
    onError: (e: Error) =>
      setStartError(e instanceof ApiError && e.status === 403 ? 'entitlement' : 'generic'),
  });

  /*
    The server returns three flat lists (weeks, workouts, exercises) because a
    plan is read whole in one round trip. The tree a human reads is assembled
    here, once per fetch.
  */
  const weeks = useMemo(() => {
    const content: VersionContent | undefined = query.data;
    if (!content) return [];

    return content.weeks.map((week) => ({
      ...week,
      workouts: content.workouts
        .filter((w) => w.week_id === week.id)
        .map((workout) => ({
          ...workout,
          exercises: content.exercises.filter((e) => e.workout_id === workout.id),
        })),
    }));
  }, [query.data]);

  // What review actually counts, and what a member needs before "Start this"
  // means anything.
  const prescribedCount = (query.data?.exercises ?? []).length;

  if (query.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  if (query.isError || !query.data) {
    return (
      <Screen edges={['bottom']}>
        <ErrorBanner message="Could not load this programme." />
      </Screen>
    );
  }

  const version = query.data.version;

  return (
    /*
      The shared wrapper, like every other pushed screen (add-week, prescribe,
      athlete/[id], manage — fourteen of them). This used to be a bare
      ScrollView with a hand-rolled `paddingBottom: huge`, which ignored the
      bottom safe area entirely: on a phone with a home indicator the last
      workout sat under it, and the spacing did not match anything else.

      `edges={['bottom']}` because the native header already owns the top.
    */
    <Screen scroll edges={['bottom']}>
      <View style={styles.header}>
        {name ? <Text style={styles.title}>{name}</Text> : null}
        <View style={styles.meta}>
          <Pill tone="accent">{`Version ${version.version_number}`}</Pill>
          {version.is_assignable ? (
            <Pill>Published</Pill>
          ) : (
            // A member should normally never see this — only staff can reach
            // unpublished versions — but stating it beats a silent wrong guess.
            <Pill>{version.status.state.replaceAll('_', ' ')}</Pill>
          )}
        </View>
      </View>

      {/*
        A refusal with a way out of it (ADR-0031). This used to name the plans
        that would have let you in and stop there, which is the most annoying
        possible thing to read — the app knew exactly what was wrong and
        offered nothing. Members can put themselves on a plan now, so the
        banner is followed by the button that does it.
      */}
      {startError === 'entitlement' ? (
        <View style={styles.gate}>
          <ErrorBanner message={missingReason('gym_access', plans.data ?? [])} />
          <Button label="Join a plan" onPress={() => router.push('/(app)/join-plan')} />
        </View>
      ) : startError === 'generic' ? (
        <ErrorBanner message="Could not start the workout. It is safe to try again." />
      ) : null}

      {moveError ? <ErrorBanner message={moveError} /> : null}

      {/*
        ---- taking it on yourself -----------------------------------------
        Offered to anyone who is not already on this version, which in practice
        means a member with no coach: previously they could read this whole
        screen and start nothing from it. Only for a published version with
        something actually in it — a programme prescribing nothing would open
        on a blank logging screen, which is the bug that started all this.
      */}
      {!myAssignment && version.is_assignable && prescribedCount > 0 ? (
        <View style={styles.takeOn}>
          <Button
            label={takeOn.isPending ? 'Starting…' : 'Start this programme'}
            detail={`${prescribedCount} exercise${prescribedCount === 1 ? '' : 's'}`}
            disabled={takeOn.isPending}
            onPress={() => takeOn.mutate()}
          />
          <Text style={styles.takeOnSays}>
            Puts you on it from today, and each workout below becomes startable.
            You can come off it whenever you like.
          </Text>
          {startProgramError ? <ErrorBanner message={startProgramError} /> : null}
        </View>
      ) : null}

      {/*
        The lifecycle, as a place plus one next move.

        It used to be a tray of six chips — Add week, Submit, Approve, Back to
        draft, Publish, Assign, Edit as new draft — all the same size, in the
        order they happened to be written. An owner opening their own draft
        could not tell from it where the programme *was*, which of those they
        were supposed to press, or why pressing Approve sometimes failed.

        So: a strip that says which of the four stages this is at, one primary
        button for the move that actually comes next, and a sentence saying
        what it will do. Everything else demotes to a quiet row. The refusals
        that used to arrive as a surprise — no weeks yet, somebody else has to
        approve — are printed before the press, not after it.
      */}
      {canAuthor ? (
        <View style={styles.life}>
          <StageStrip state={version.status.state} />
          <LifecycleActions
            state={version.status.state}
            assignable={version.is_assignable}
            editable={version.is_editable}
            weekCount={weeks.length}
            prescribedCount={prescribedCount}
            canApprove={canApprove}
            busy={move.isPending || draft.isPending}
            copying={draft.isPending}
            hasProgramId={Boolean(programId)}
            onAddWeek={() =>
              router.push({
                pathname: '/(app)/add-week',
                params: { version: versionId, next: String(weeks.length + 1) },
              })
            }
            onMove={(to) => move.mutate(to)}
            onFork={() => draft.mutate()}
            canArchive={canApprove && version.status.state !== 'archived'}
            archiving={archive.isPending}
            onArchive={() =>
              Alert.alert(
                'Remove this version from the library?',
                version.is_assignable
                  ? 'Nobody new can be put on it. Anyone already training on it keeps going, and their history stays — this is not a delete.'
                  : 'It stops appearing in the library. The record stays, so nothing already referring to it breaks.',
                [
                  { text: 'Keep it', style: 'cancel' },
                  {
                    text: 'Remove',
                    style: 'destructive',
                    onPress: () => archive.mutate(),
                  },
                ],
              )
            }
          />
        </View>
      ) : null}

      {weeks.length === 0 ? (
        <EmptyState
          glyph="◌"
          title="No content yet"
          hint={
            canAuthor && version.is_editable
              ? 'Start with a week — it leads on to a workout, and then to the exercises in it.'
              : 'This version has no weeks. A manager adds them while it is a draft.'
          }
        />
      ) : (
        weeks.map((week) => (
          <View key={week.id} style={styles.week}>
            <Text style={styles.weekTitle}>{weekTitle(week.week_number, week.label)}</Text>

            {canAuthor && version.is_editable ? (
              <Touchable
                onPress={() =>
                  router.push({
                    pathname: '/(app)/add-workout',
                    params: {
                      week: week.id,
                      version: versionId,
                      weekLabel: weekTitle(week.week_number, week.label),
                      // Days already used, so the form can grey them out rather
                      // than letting the server reject a filled-in form.
                      taken: week.workouts.map((w) => w.day_number).join(','),
                    },
                  })
                }
                accessibilityRole="button"
                accessibilityLabel={`Add a workout to ${weekTitle(week.week_number, week.label)}`}
                style={styles.addRow}
              >
                <Feather name="plus" size={14} color={t.color.accent} />
                <Text style={styles.addRowText}>Add workout</Text>
              </Touchable>
            ) : null}

            {week.workouts.length === 0 ? (
              <Text style={styles.restNote}>No workouts this week.</Text>
            ) : (
              week.workouts.map((workout) => (
                <Card key={workout.id} style={styles.workout}>
                  <Text style={styles.workoutTitle}>
                    {workoutTitle(workout.day_number, workout.name)}
                  </Text>
                  {workout.notes ? (
                    <Text style={styles.workoutNotes}>{workout.notes}</Text>
                  ) : null}

                  {myAssignment ? (
                    (() => {
                      const open = openSessionFor(workout.id);
                      return (
                        <Touchable
                          onPress={() => {
                            if (open) {
                              router.push({
                                pathname: '/(app)/session/[id]',
                                params: { id: open.id },
                              });
                            } else if (!start.isPending) {
                              start.mutate(workout.id);
                            }
                          }}
                          accessibilityRole="button"
                          accessibilityLabel={
                            open
                              ? `Continue your open ${workout.name} session`
                              : `Start ${workout.name}`
                          }
                          style={styles.startButton}
                        >
                          <Feather
                            name={open ? 'play' : 'play-circle'}
                            size={16}
                            color={t.color.onAccent}
                          />
                          <Text style={styles.startText}>
                            {open
                              ? 'Continue workout'
                              : start.isPending
                                ? 'Starting…'
                                : 'Start workout'}
                          </Text>
                        </Touchable>
                      );
                    })()
                  ) : null}

                  {workout.exercises.map((exercise, index) => (
                    <View
                      key={exercise.id}
                      style={[styles.exercise, index === 0 && styles.exerciseFirst]}
                    >
                      <View style={styles.exercisePosition}>
                        <Text style={styles.exercisePositionText}>{exercise.position}</Text>
                      </View>
                      <View style={styles.exerciseBody}>
                        <Text style={styles.exerciseName} numberOfLines={2}>
                          {exercise.exercise_name}
                        </Text>
                        <Text style={styles.exercisePrescription}>
                          {prescriptionLabel(exercise.prescription as Prescription)}
                        </Text>
                        {exercise.notes ? (
                          <Text style={styles.exerciseNotes}>{exercise.notes}</Text>
                        ) : null}
                      </View>
                      <Feather
                        name={
                          exercise.prescription.kind === 'repetitions'
                            ? 'repeat'
                            : exercise.prescription.kind === 'duration'
                              ? 'clock'
                              : 'trending-up'
                        }
                        size={15}
                        color={t.color.faint}
                      />
                    </View>
                  ))}

                  {canAuthor && version.is_editable ? (
                    <Touchable
                      onPress={() =>
                        router.push({
                          pathname: '/(app)/prescribe',
                          params: {
                            workout: workout.id,
                            version: versionId,
                            workoutName: workout.name,
                          },
                        })
                      }
                      accessibilityRole="button"
                      accessibilityLabel={`Add an exercise to ${workout.name}`}
                      style={styles.addRow}
                    >
                      <Feather name="plus" size={14} color={t.color.accent} />
                      <Text style={styles.addRowText}>Add exercise</Text>
                    </Touchable>
                  ) : null}
                </Card>
              ))
            )}
          </View>
        ))
      )}
    </Screen>
  );
}

/** The four places a version can be, in order, with this one marked. */
const STAGES: { key: string; label: string }[] = [
  { key: 'draft', label: 'Draft' },
  { key: 'in_review', label: 'In review' },
  { key: 'approved', label: 'Approved' },
  { key: 'published', label: 'Published' },
];

/**
 * Where this version is.
 *
 * A version moves in one direction through four states and the screen never
 * said so. Four labels with the current one filled answers "what happens next"
 * before anybody reads a button — and it makes `archived` legible as what it
 * is: off the track entirely, rather than a fifth stage.
 */
function StageStrip({ state }: { state: string }) {
  const s = useStyles(styleFactory);
  const index = STAGES.findIndex((x) => x.key === state);

  if (index === -1) {
    return (
      <View style={s.stageStrip}>
        <View style={[s.stage, s.stageOn]}>
          <Text style={[s.stageText, s.stageTextOn]}>{state.replaceAll('_', ' ')}</Text>
        </View>
      </View>
    );
  }

  return (
    <View style={s.stageStrip} accessibilityLabel={`Stage: ${STAGES[index]!.label}`}>
      {STAGES.map((stage, i) => (
        <View
          key={stage.key}
          style={[s.stage, i === index && s.stageOn, i < index && s.stageDone]}
        >
          <Text
            style={[s.stageText, i === index && s.stageTextOn, i < index && s.stageTextDone]}
            numberOfLines={1}
          >
            {stage.label}
          </Text>
        </View>
      ))}
    </View>
  );
}

/**
 * One primary move, a sentence about it, and the rest demoted.
 *
 * `blocked` is the important half: where the next move cannot be made yet, the
 * button is disabled and the reason is printed under it. "Someone else has to
 * approve this" is not an error — it is how review works — and finding it out
 * by pressing a button and reading a red banner is the wrong way round.
 */
function LifecycleActions({
  state,
  assignable,
  editable,
  weekCount,
  prescribedCount,
  canApprove,
  busy,
  copying,
  hasProgramId,
  onAddWeek,
  onMove,
  onFork,
  onArchive,
  canArchive,
  archiving,
}: {
  state: string;
  assignable: boolean;
  editable: boolean;
  weekCount: number;
  /** Exercises prescribed across the whole version — what review counts. */
  prescribedCount: number;
  canApprove: boolean;
  busy: boolean;
  copying: boolean;
  hasProgramId: boolean;
  onAddWeek: () => void;
  onMove: (to: Transition) => void;
  onFork: () => void;
  onArchive: () => void;
  /** Owners and admins, and only while it is not already archived. */
  canArchive: boolean;
  archiving: boolean;
}) {
  const s = useStyles(styleFactory);

  type Plan = {
    label: string;
    says: string;
    onPress: () => void;
    blocked?: string;
    secondary?: { label: string; onPress: () => void }[];
  };

  const plan = ((): Plan | null => {
    if (state === 'draft' && editable) {
      if (weekCount === 0) {
        return {
          label: 'Add the first week',
          // Name all three levels. The shape of a programme is
          // weeks -> workouts -> exercises, and somebody looking at an empty
          // one has no way to know that exercises are two levels below the
          // button they are being offered. Adding a week now leads straight
          // into a workout and then the exercise picker, so this says so.
          says: 'A programme is weeks, each holding workouts, each holding exercises. This walks you through all three.',
          onPress: onAddWeek,
        };
      }
      // Weeks are containers; review counts PRESCRIBED EXERCISES. An empty week
      // (or a workout with nothing in it) used to sail through review and
      // publish, and a published version is frozen — so the athlete got a
      // workout that opened on a blank screen and nobody could repair it. Say
      // so before the press rather than after it, which is what `blocked` is for.
      if (prescribedCount === 0) {
        return {
          label: 'Send for review',
          says: 'Locks the content while somebody signs it off.',
          onPress: () => onMove('submit_for_review'),
          blocked:
            weekCount > 0
              ? 'Nothing is prescribed yet. Add a workout and at least one exercise — a week with nothing in it cannot be trained, so it cannot be reviewed.'
              : undefined,
        };
      }
      return {
        label: 'Send for review',
        says: 'Locks the content while somebody signs it off. You can pull it back to draft afterwards.',
        onPress: () => onMove('submit_for_review'),
        secondary: [{ label: 'Add another week', onPress: onAddWeek }],
      };
    }

    if (state === 'in_review') {
      if (!canApprove) {
        return {
          label: 'Waiting for a head coach',
          says: 'A trainer writes, a head coach signs off. Nothing more to do here until they do.',
          onPress: () => {},
          blocked: 'Only a head coach or above can approve a version.',
        };
      }
      return {
        label: 'Approve it',
        says: 'Says the content is right. It still is not live until it is published.',
        onPress: () => onMove('approve'),
        secondary: [
          { label: 'Send back to draft', onPress: () => onMove('return_to_draft') },
        ],
      };
    }

    if (state === 'approved') {
      if (!canApprove) {
        return {
          label: 'Waiting to be published',
          says: 'Approved, but not live yet. A head coach or above publishes it.',
          onPress: () => {},
          blocked: 'Only a head coach or above can publish.',
        };
      }
      return {
        label: 'Publish it',
        says: 'Makes it assignable — and freezes the content for good. Changes after this start a new version.',
        onPress: () => onMove('publish'),
      };
    }

    if (assignable) {
      // "Put an athlete on it" used to live here, and it is gone (ADR-0034).
      // Whoever reaches this block is a manager — they wrote the catalogue —
      // and a manager no longer prescribes from it. Their trainers do, from the
      // client's own screen where they can see what that person did last week,
      // which is the only place the choice can be made well.
      //
      // So the published state's next move is the only one left: supersede it.
      return {
        label: copying ? 'Copying…' : 'Edit as a new draft',
        says: 'Published content is frozen. This copies it into a new draft; everyone already on this version stays on it.',
        onPress: onFork,
        blocked: hasProgramId ? undefined : 'Reopen this from the programme list to edit it.',
      };
    }

    return null;
  })();

  if (!plan) return null;

  return (
    <>
      <Button
        label={plan.label}
        disabled={busy || Boolean(plan.blocked)}
        onPress={plan.onPress}
      />
      <Text style={plan.blocked ? s.saysBlocked : s.says}>{plan.blocked ?? plan.says}</Text>
      {plan.secondary && plan.secondary.length > 0 ? (
        <View style={s.secondaryRow}>
          {plan.secondary.map((action) => (
            <Touchable
              key={action.label}
              onPress={action.onPress}
              disabled={busy}
              accessibilityRole="button"
              accessibilityLabel={action.label}
              style={s.lifeAction}
            >
              <Text style={s.lifeActionText}>{action.label}</Text>
            </Touchable>
          ))}
        </View>
      ) : null}
      {assignable ? (
        <Text style={s.immutableNote}>
          A published version is never edited. Editing copies it into a new one, and everyone
          already on this version stays on it.
        </Text>
      ) : null}

      {/*
        Taking it out of the library.
        
        Quiet and last, because it is the one move here that removes something.
        Offered for a draft AND for a published version: a draft nobody wants is
        clutter, and a published version the gym has stopped running should stop
        being assignable — which is exactly what archiving does.

        It is ARCHIVE, not delete, and the button says so. Assignments reference
        a specific version (ADR-0006) and a member's history has to stay
        readable, so the row survives and only its status changes. Anyone already
        on it keeps training; nobody new can be put on it.
      */}
      {canArchive ? (
        <Touchable
          onPress={onArchive}
          disabled={busy}
          accessibilityRole="button"
          accessibilityLabel="Remove this version from the library"
          style={s.archiveRow}
        >
          <Text style={s.archiveText}>
            {archiving ? 'Removing…' : 'Remove from the library'}
          </Text>
        </Touchable>
      ) : null}
    </>
  );
}


const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    archiveRow: { alignItems: 'center', paddingTop: t.space.md },
    archiveText: {
      color: t.color.danger,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
    },
    takeOn: { gap: t.space.sm },
    takeOnSays: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 18,
    },
    // Wrapping, gutters and safe-area padding come from `Screen` now.

    header: { gap: t.space.md },
    gate: { gap: t.space.md },
    title: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 30,
    },
    meta: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.sm },

    // The lifecycle sits in one recessed tray, so "where is this and what
    // happens next" is a single object on the screen rather than six chips.
    life: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.lg,
      gap: t.space.md,
      padding: t.space.lg,
    },
    stageStrip: { flexDirection: 'row', gap: 4 },
    stage: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderRadius: t.radius.sm,
      flex: 1,
      paddingHorizontal: 4,
      paddingVertical: 7,
    },
    stageDone: { backgroundColor: t.color.accentHi },
    stageOn: { backgroundColor: t.color.accent },
    stageText: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
    stageTextDone: { color: t.color.accentBadgeInk },
    stageTextOn: { color: t.color.onAccent },
    says: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 18,
      marginTop: -t.space.xs,
    },
    saysBlocked: {
      color: t.color.warn,
      fontFamily: fonts.medium,
      fontSize: t.font.sm,
      lineHeight: 18,
      marginTop: -t.space.xs,
    },
    secondaryRow: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.sm },
    lifeAction: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderRadius: t.radius.pill,
      flexDirection: 'row',
      gap: 6,
      paddingHorizontal: 13,
      paddingVertical: 9,
      ...t.elevation(1),
    },
    lifeActionText: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },
    immutableNote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },

    addRow: {
      alignItems: 'center',
      alignSelf: 'flex-start',
      flexDirection: 'row',
      gap: 6,
      paddingVertical: t.space.sm,
    },
    addRowText: { color: t.color.accentDeep, fontFamily: fonts.bold, fontSize: t.font.sm },

    week: { gap: t.space.md },
    weekTitle: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    restNote: { color: t.color.faint, fontFamily: fonts.regular, fontSize: t.font.sm },

    workout: { gap: t.space.sm },
    workoutTitle: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    workoutNotes: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },

    startButton: {
      alignItems: 'center',
      backgroundColor: t.color.accent,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      gap: t.space.sm,
      justifyContent: 'center',
      marginTop: t.space.xs,
      minHeight: 46,
      paddingHorizontal: t.space.md,
    },
    startText: { color: t.color.onAccent, fontFamily: fonts.bold, fontSize: t.font.md },

    exercise: {
      alignItems: 'center',
      borderTopColor: t.color.line,
      borderTopWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      marginTop: t.space.xs,
      paddingTop: t.space.md,
    },
    // The first row sits directly under the title; a rule there reads as clutter.
    exerciseFirst: { borderTopWidth: 0, marginTop: 0 },
    exercisePosition: {
      alignItems: 'center',
      // `sunken`, not surface2: the card is already surface2, so the old badge
      // was an invisible square. A well in the card is the honest shape for a
      // position marker.
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.sm,
      height: 28,
      justifyContent: 'center',
      width: 28,
    },
    exercisePositionText: {
      color: t.color.mut2,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
    exerciseBody: { flex: 1, gap: 2 },
    exerciseName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    exercisePrescription: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },
    exerciseNotes: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 18,
    },
  });
