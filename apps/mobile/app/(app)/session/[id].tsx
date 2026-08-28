import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import * as Crypto from 'expo-crypto';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { ApiError } from '@/api/client';
import {
  finishSession,
  getSessionDetail,
  getVersionContent,
  listAssignments,
  listExercises,
  logSet,
  type Assignment,
  type Exercise,
  type ModalityKind,
  type PerformedSetPayload,
  type SessionDetail,
  type VersionContent,
} from '@/api/gym';
import { plateLabel, platesPerSide } from '@/features/gym/plates';
import { sessionAge } from '@/features/session/age';
import { sessionNameFor } from '@/features/session/name';
import { prescriptionLabel, secondsLabel, type Prescription } from '@/features/programs/format';
import { clockLabel, elapsedSeconds } from '@/features/timer/clock';
import { useSession } from '@/session/store';
import {
  BarProgress,
  Button,
  Centered,
  ErrorBanner,
  Field,
  LivePill,
  Section,
  Touchable,
  containerStyle,
} from '@/ui/components';
import { RestTimer } from '@/ui/rest-timer';
import { useNow } from '@/ui/use-now';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * The logging screen — built for the thirty seconds between sets, one thumb,
 * eyes flicking down.
 *
 * The reference design puts every exercise, every stepper and every list on the
 * screen at once. Standing under a bar you need four things: what am I lifting,
 * what did I just do, log it, how long until the next one. So exactly ONE
 * exercise is expanded (the one you are on, or the one you tap), the rest
 * collapse to a line with their progress, and a running rest timer becomes the
 * loudest thing on the screen because that is the only moment it matters.
 *
 * Set ids are minted on the device (ADR-0008) — a retry after a network wobble
 * replays the same id and the server treats it as a no-op, so mashing "Log set"
 * on a bad connection cannot double-log.
 */
/** How an exercise is measured, in words, for a row with no prescription. */
const MEASURED_IN: Record<ModalityKind, string> = {
  repetitions: 'Reps and load',
  duration: 'Timed',
  distance: 'Distance',
};

/**
 * One exercise's place in a session, whichever kind of session it is.
 *
 * `prescription` is what makes the two kinds differ and `modality` is what
 * makes them the same: every exercise is measured in something, only some of
 * them were asked for in advance.
 */
type Slot = {
  /** Stable list key: the prescription line's id, or the exercise's. */
  key: string;
  exerciseId: string;
  templateExerciseId: string | null;
  name: string;
  modality: ModalityKind;
  prescription: Prescription | null;
};

/**
 * The slots of an unplanned session: what the member picked on the way in,
 * what they have added since, and everything they have actually logged.
 *
 * Logged sets come last and are what makes the list durable — a phone that
 * dies mid-session loses `picked` and `added`, and reopening the session
 * rebuilds it from the history, which is the copy that matters. Order is
 * first-appearance, and the de-duplication is by exercise so an exercise
 * picked twice is one row.
 */
function unplannedSlots(
  picked: string | undefined,
  added: string[],
  sets: SessionDetail['sets'],
  catalogue: Exercise[] | undefined,
): Slot[] {
  const byId = new Map((catalogue ?? []).map((e) => [e.id, e]));

  const ids: string[] = [];
  for (const id of [
    ...(picked ? picked.split(',') : []),
    ...added,
    ...sets.map((set) => set.exercise_id),
  ]) {
    const trimmed = id.trim();
    if (trimmed && !ids.includes(trimmed)) ids.push(trimmed);
  }

  return ids.flatMap((id) => {
    const exercise = byId.get(id);
    // An exercise the catalogue does not have — retired since, or a stale id
    // in the route. Dropping it is right: there is no name to show and no
    // modality to decide which steppers to draw. Anything logged against it
    // is still in history; it just has no row to log MORE against.
    if (!exercise) return [];
    return [
      {
        key: exercise.id,
        exerciseId: exercise.id,
        templateExerciseId: null,
        name: exercise.name,
        modality: exercise.modality.kind,
        prescription: null,
      },
    ];
  });
}

export default function SessionScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const { id: sessionId, picked } = useLocalSearchParams<{
    id: string;
    /**
     * Exercise ids chosen on the way in, comma-separated.
     *
     * A STARTING LIST, not a stored plan — deliberately. An unplanned session
     * has no prescription to save these against (ADR-0035), and inventing a
     * table to hold "exercises I intend to do" would be a second, weaker copy
     * of what `performed_sets` already records the moment a set is logged. So
     * they live in the route for this sitting: log a set and the exercise is
     * durable, walk away first and it was never a promise.
     */
    picked?: string;
  }>();
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const userId = useSession((st) => st.user?.id);

  const detail = useQuery({
    queryKey: ['session', gymId, sessionId],
    queryFn: () => getSessionDetail(gymId!, sessionId),
    enabled: Boolean(gymId && sessionId),
  });

  /*
    Rest lives at this level, not inside an exercise card: rest follows the set
    you just logged, and it must survive switching exercises. Anchored to a
    wall-clock endsAt — pocketing the phone mid-rest costs nothing.
  */
  const [restDuration, setRestDuration] = useState(90);
  const [restEndsAt, setRestEndsAt] = useState<number | null>(null);
  const startRest = (seconds: number) => {
    setRestDuration(seconds);
    setRestEndsAt(Date.now() + seconds * 1000);
  };

  /** Which exercise is expanded. Null means "whichever is unfinished". */
  const [focusId, setFocusId] = useState<string | null>(null);

  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });

  const versionId = useMemo(() => {
    const d: SessionDetail | undefined = detail.data;
    const a: Assignment[] | undefined = assignments.data;
    if (!d || !a) return null;
    return a.find((x) => x.id === d.session.assignment_id)?.program_version_id ?? null;
  }, [detail.data, assignments.data]);

  const content = useQuery({
    queryKey: ['program-version', gymId, versionId],
    queryFn: () => getVersionContent(gymId!, versionId!),
    enabled: Boolean(gymId && versionId),
  });

  // Only an unplanned session needs the catalogue: a prescribed one carries
  // its exercise names and modalities inside the version content already.
  const unplanned = detail.data ? detail.data.session.workout_template_id == null : false;
  const catalogue = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId!),
    enabled: Boolean(gymId) && unplanned,
  });

  /** Exercises added by tapping "Add an exercise" during this sitting. */
  const [added, setAdded] = useState<string[]>([]);
  const [picking, setPicking] = useState(false);

  const finish = useMutation({
    mutationFn: (outcome: 'completed' | 'abandoned') =>
      finishSession(gymId!, sessionId, outcome),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['session', gymId, sessionId] });
      void queryClient.invalidateQueries({ queryKey: ['sessions', gymId] });
      router.back();
    },
  });

  if (detail.isLoading || (versionId && content.isLoading)) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  if (detail.isError || !detail.data) {
    return (
      <View style={s.errorWrap}>
        <ErrorBanner message="Could not load this workout session." />
      </View>
    );
  }

  const session = detail.data.session;
  const sets = detail.data.sets;
  const mine = session.athlete_id === userId;
  const open = session.is_open && mine;

  const setsFor = (exerciseId: string) => sets.filter((x) => x.exercise_id === exerciseId);
  const prescribedSets = (p: Prescription) => (p.kind === 'distance' ? 1 : p.sets);

  /*
    One list, two sources. A prescribed session's slots come from the published
    version; an unplanned one's are assembled here from what the member picked
    and what they have already logged. Everything below this line reads `slots`
    and does not care which kind of session it is looking at — which is the
    only reason there is one screen rather than two.
  */
  const slots: Slot[] = unplanned
    ? unplannedSlots(picked, added, sets, catalogue.data)
    : ((content.data as VersionContent | undefined)?.exercises ?? [])
        .filter((e) => e.workout_id === session.workout_template_id)
        .map((e) => ({
          key: e.id,
          exerciseId: e.exercise_id,
          templateExerciseId: e.id,
          name: e.exercise_name,
          // A prescription is written in the modality the exercise measures,
          // so its discriminant IS the modality. No second lookup needed.
          modality: (e.prescription as Prescription).kind,
          prescription: e.prescription as Prescription,
        }));

  // The exercise in focus: an explicit tap wins, otherwise the first one with
  // sets still owing, otherwise the last. In an unplanned session nothing is
  // owed, so the first with no sets stands in — that is the one you are on.
  const firstUnfinished = slots.find((slot) =>
    slot.prescription
      ? setsFor(slot.exerciseId).length < prescribedSets(slot.prescription)
      : setsFor(slot.exerciseId).length === 0,
  );
  const focused =
    slots.find((slot) => slot.key === focusId) ?? firstUnfinished ?? slots[slots.length - 1];

  const totalPrescribed = slots.reduce(
    (n, slot) => n + (slot.prescription ? prescribedSets(slot.prescription) : 0),
    0,
  );
  const doneRatio = totalPrescribed > 0 ? Math.min(1, sets.length / totalPrescribed) : 0;

  const alreadyIn = new Set(slots.map((slot) => slot.exerciseId));

  return (
    <KeyboardAvoidingView
      style={s.flex}
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
    >
      <Stack.Screen
        options={{
          title: sessionNameFor(session),
        }}
      />
      <ScrollView
        style={s.screen}
        contentContainerStyle={s.content}
        showsVerticalScrollIndicator={false}
        keyboardShouldPersistTaps="handled"
      >
        <View style={s.headRow}>
          {open ? (
            <LivePill>In progress</LivePill>
          ) : (
            <Text style={s.headKicker} numberOfLines={1}>
              {session.status.state.replaceAll('_', ' ')}
            </Text>
          )}
          <View style={s.headSpacer} />
          {session.is_open ? <SessionClock startedAt={session.started_at} /> : null}
        </View>
        {totalPrescribed > 0 ? (
          <View style={s.headBar}>
            <BarProgress fraction={doneRatio} height={4} />
            <Text style={s.headMeta}>
              {`${sets.length} of ${totalPrescribed} sets`}
              {!mine ? ' · read-only' : ''}
            </Text>
          </View>
        ) : null}

        {/*
          An empty screen has three quite different causes and the old copy
          ("No sets logged yet.") described none of them — it read as "you have
          not started", so nobody could tell a programme with nothing in it from
          a lookup that failed. Say which it is, because the person who has to
          act is different every time.
        */}
        {slots.length === 0 && !content.isLoading && !catalogue.isLoading ? (
          <View style={s.emptyBlock}>
            <Text style={s.note}>
              {unplanned
                ? 'Nothing in this workout yet. Add an exercise and start logging.'
                : sets.length > 0
                  ? 'Showing your logged sets — the prescription for this workout could not be loaded.'
                  : versionId == null
                    ? 'This workout could not be matched to a programme, so there is nothing to log.'
                    : 'This workout has no exercises in it yet, so there is nothing to log.'}
            </Text>
            {!unplanned && sets.length === 0 ? (
              <Text style={s.noteHint}>
                Nothing you can fix from here — ask your coach to add exercises to
                this workout. They will need to publish a new version of the
                programme, as published ones cannot be edited.
              </Text>
            ) : null}
          </View>
        ) : null}

        {focused ? (
          <ExerciseLogger
            key={focused.key}
            gymId={gymId!}
            sessionId={sessionId}
            exerciseId={focused.exerciseId}
            templateExerciseId={focused.templateExerciseId}
            name={focused.name}
            index={slots.indexOf(focused) + 1}
            modality={focused.modality}
            prescription={focused.prescription}
            sets={setsFor(focused.exerciseId)}
            editable={open}
            onLogged={() => startRest(restDuration)}
          />
        ) : null}

        {/* Adding to the workout you are doing, mid-session — the whole point
            of an unplanned one. Offered while it is open and yours. */}
        {unplanned && open ? (
          picking ? (
            <ExercisePicker
              catalogue={catalogue.data ?? []}
              loading={catalogue.isLoading}
              exclude={alreadyIn}
              onPick={(exercise) => {
                setAdded((prev) => (prev.includes(exercise.id) ? prev : [...prev, exercise.id]));
                // Bring it straight into focus: you tapped it because it is
                // what you are about to do, not to file it away for later.
                setFocusId(exercise.id);
                setPicking(false);
              }}
              onCancel={() => setPicking(false)}
            />
          ) : (
            <Button
              label="Add an exercise"
              variant="secondary"
              onPress={() => setPicking(true)}
            />
          )
        ) : null}

        {/* Everything else: one line each, tap to bring into focus. */}
        {slots.length > 1 ? (
          <View>
            <Section label={unplanned ? 'Also in this workout' : 'The rest of this workout'} />
            {slots
              .filter((slot) => slot.key !== focused?.key)
              .map((slot, i, arr) => {
                const done = setsFor(slot.exerciseId).length;
                const owed = slot.prescription ? prescribedSets(slot.prescription) : null;
                return (
                  <Touchable
                    key={slot.key}
                    onPress={() => setFocusId(slot.key)}
                    accessibilityLabel={
                      owed == null
                        ? `${slot.name}, ${done} sets logged. Switch to it.`
                        : `${slot.name}, ${done} of ${owed} sets logged. Switch to it.`
                    }
                    style={[s.queueRow, i === arr.length - 1 && s.queueRowLast]}
                  >
                    <Text style={s.queueIndex}>
                      {String(slots.indexOf(slot) + 1).padStart(2, '0')}
                    </Text>
                    <View style={s.queueBody}>
                      <Text style={s.queueName} numberOfLines={1}>
                        {slot.name}
                      </Text>
                      <Text style={s.queuePrescription} numberOfLines={1}>
                        {slot.prescription
                          ? prescriptionLabel(slot.prescription)
                          : MEASURED_IN[slot.modality]}
                      </Text>
                    </View>
                    <Text
                      style={[
                        s.queueCount,
                        owed != null && done >= owed && s.queueCountDone,
                      ]}
                    >
                      {owed == null ? `${done}` : `${done}/${owed}`}
                    </Text>
                  </Touchable>
                );
              })}
          </View>
        ) : null}

        {open ? (
          <View style={s.finishBlock}>
            {/* Secondary, deliberately: while you are training, the primary
                action is logging the next set — not ending the session. */}
            <Button
              label={finish.isPending ? 'Finishing…' : 'Finish workout'}
              variant="secondary"
              disabled={finish.isPending}
              onPress={() => finish.mutate('completed')}
            />
            <Button
              label="Discard session"
              variant="ghost"
              onPress={() =>
                Alert.alert(
                  'Discard this session?',
                  'It will be kept in your history as cut short — the sets you logged stay.',
                  [
                    { text: 'Keep going', style: 'cancel' },
                    {
                      text: 'Discard',
                      style: 'destructive',
                      onPress: () => finish.mutate('abandoned'),
                    },
                  ],
                )
              }
            />
          </View>
        ) : null}
      </ScrollView>

      {open && restEndsAt != null ? (
        <View style={s.restDock}>
          <RestTimer
            endsAt={restEndsAt}
            durationSeconds={restDuration}
            onRestart={startRest}
            onDismiss={() => setRestEndsAt(null)}
          />
        </View>
      ) : null}
    </KeyboardAvoidingView>
  );
}

/**
 * "12:41" — ticking coarsely; nobody needs their session to the second.
 *
 * A session left open for days stops claiming to be a stopwatch and says when
 * it was opened instead: "9519:27 elapsed" is not information.
 */
function SessionClock({ startedAt }: { startedAt: string }) {
  const s = useStyles(styleFactory);
  const now = useNow(15_000, true);
  const started = new Date(startedAt).getTime();
  if (Number.isNaN(started)) return null;

  const age = sessionAge(startedAt, new Date(now));
  if (age.stale) return <Text style={s.clockStale}>{age.label}</Text>;

  return <Text style={s.clock}>{clockLabel(elapsedSeconds(started, now))}</Text>;
}

/** A big, thumb-sized − value + control. The number is the point, so it is huge. */
function Stepper({
  label,
  value,
  onChange,
  step,
  min = 0,
  max,
  decimals = 0,
  size = 'lg',
  placeholder = '—',
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  step: number;
  min?: number;
  max: number;
  decimals?: number;
  size?: 'lg' | 'sm';
  placeholder?: string;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);

  const bump = (direction: 1 | -1) => {
    const current = Number.parseFloat(value);
    const base = Number.isFinite(current) ? current : min;
    const next = Math.min(max, Math.max(min, base + direction * step));
    onChange(`${Number(next.toFixed(decimals))}`);
  };

  return (
    <View style={s.stepperGroup}>
      <Text style={s.fieldLabel}>{label}</Text>
      <View style={[s.stepperRow, size === 'lg' ? s.stepperRowLg : s.stepperRowSm]}>
        <Pressable
          onPress={() => bump(-1)}
          accessibilityRole="button"
          accessibilityLabel={`Decrease ${label}`}
          style={[s.stepperButton, size === 'lg' ? s.stepperButtonLg : s.stepperButtonSm]}
        >
          <Feather name="minus" size={size === 'lg' ? 22 : 18} color={t.color.ink} />
        </Pressable>
        <View style={s.stepperValueWrap}>
          <Text
            style={[s.stepperValue, size === 'sm' && s.stepperValueTextSm]}
            numberOfLines={1}
            adjustsFontSizeToFit
          >
            {value.trim() === '' ? placeholder : value}
          </Text>
        </View>
        <Pressable
          onPress={() => bump(1)}
          accessibilityRole="button"
          accessibilityLabel={`Increase ${label}`}
          style={[s.stepperButton, size === 'lg' ? s.stepperButtonLg : s.stepperButtonSm]}
        >
          <Feather name="plus" size={size === 'lg' ? 22 : 18} color={t.color.ink} />
        </Pressable>
      </View>
    </View>
  );
}

/**
 * One exercise, expanded, with its steppers.
 *
 * Takes a `modality` separately from the `prescription` because an unplanned
 * session (ADR-0035) has the first and not the second: what you log is decided
 * by what the exercise MEASURES — reps, seconds, metres — and a prescription
 * only ever narrowed that further. Splitting them is what lets one logger serve
 * both kinds of session instead of a second copy that drifts.
 */
function ExerciseLogger({
  gymId,
  sessionId,
  exerciseId,
  templateExerciseId,
  name,
  index,
  modality,
  prescription,
  sets,
  editable,
  onLogged,
}: {
  gymId: string;
  sessionId: string;
  exerciseId: string;
  /** The prescription line being answered. Null in an unplanned session. */
  templateExerciseId: string | null;
  name: string;
  index: number;
  modality: ModalityKind;
  /** What was asked for, when anything was. */
  prescription: Prescription | null;
  sets: SessionDetail['sets'];
  editable: boolean;
  onLogged: () => void;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const queryClient = useQueryClient();
  const lastSet = sets[sets.length - 1];

  const [watchStartedAt, setWatchStartedAt] = useState<number | null>(null);
  const watchNow = useNow(500, watchStartedAt != null);

  const [reps, setReps] = useState(
    prescription?.kind === 'repetitions' ? `${prescription.target.max}` : '',
  );
  const [weight, setWeight] = useState(() => {
    const w =
      lastSet && lastSet.performed.kind === 'repetitions' ? lastSet.performed.weight_kg : null;
    return w != null ? `${w}` : '';
  });
  const [amount, setAmount] = useState('');
  const [rpe, setRpe] = useState('');
  const [error, setError] = useState<string | null>(null);

  const log = useMutation({
    mutationFn: () => {
      let performed: PerformedSetPayload;
      if (modality === 'repetitions') {
        performed = {
          kind: 'repetitions',
          reps: Number.parseInt(reps, 10),
          weight_kg: weight.trim() === '' ? null : Number.parseFloat(weight),
        };
      } else if (modality === 'duration') {
        performed = { kind: 'duration', seconds: Number.parseInt(amount, 10) };
      } else {
        performed = { kind: 'distance', metres: Number.parseInt(amount, 10) };
      }

      return logSet(gymId, sessionId, {
        id: Crypto.randomUUID(),
        exerciseId,
        templateExerciseId,
        setNumber: sets.length + 1,
        performed,
        rpe: rpe.trim() === '' ? null : Number.parseInt(rpe, 10),
      });
    },
    onSuccess: () => {
      setError(null);
      void queryClient.invalidateQueries({ queryKey: ['session', gymId, sessionId] });
      onLogged();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? e.message
          : 'Could not log the set. It is safe to try again.',
      );
    },
  });

  const numbersOk =
    modality === 'repetitions'
      ? /^\d+$/.test(reps) && (weight.trim() === '' || /^\d+(\.\d+)?$/.test(weight))
      : /^\d+$/.test(amount);
  const rpeOk = rpe.trim() === '' || /^(10|\d)$/.test(rpe);
  const ready = numbersOk && rpeOk && !log.isPending;

  // Nothing is owed when nothing was asked for — an unplanned session is done
  // when the member says it is, so the header counts up instead of down.
  const owed = prescription == null ? null : prescription.kind === 'distance' ? 1 : prescription.sets;
  const setNumber = sets.length + 1;

  // What to put on the bar. Only for loaded work — bodyweight reps and machines
  // have no plates to fetch, and a wrong answer there is worse than none.
  const load =
    modality === 'repetitions' && weight.trim() !== ''
      ? platesPerSide(Number.parseFloat(weight))
      : null;

  const lastTime =
    lastSet && lastSet.performed.kind === 'repetitions'
      ? `Last set ${describeSet(lastSet.performed)}${lastSet.rpe != null ? ` @ RPE ${lastSet.rpe}` : ''}`
      : null;

  return (
    <View style={s.focal}>
      <View style={s.focalHead}>
        <Text style={s.focalIndex}>{String(index).padStart(2, '0')}</Text>
        <Text style={s.focalName} numberOfLines={2}>
          {name}
        </Text>
        <Text style={s.focalSetOf}>
          {owed != null && setNumber <= owed
            ? `Set ${setNumber} of ${owed}`
            : `${sets.length} logged`}
        </Text>
      </View>

      {/* No target line in an unplanned session — there is no target. Showing
          "Target —" would be inventing a prescription that nobody wrote. */}
      {prescription ? (
        <Text style={s.target}>
          Target <Text style={s.targetStrong}>{prescriptionLabel(prescription)}</Text>
          {lastTime ? ` · ${lastTime}` : ''}
        </Text>
      ) : lastTime ? (
        <Text style={s.target}>{lastTime}</Text>
      ) : null}

      {sets.length > 0 ? (
        <View style={s.setList}>
          {sets.map((set) => (
            <View key={set.id} style={s.setRow}>
              <Text style={s.setIndex}>{set.set_number}</Text>
              <Text style={s.setText}>{describeSet(set.performed)}</Text>
              {set.rpe != null ? <Text style={s.setRpe}>RPE {set.rpe}</Text> : null}
              <Feather name="check" size={15} color={t.color.success} />
            </View>
          ))}
        </View>
      ) : null}

      {editable ? (
        <>
          {modality === 'repetitions' ? (
            <>
              <Stepper
                label="Weight · kg"
                value={weight}
                onChange={setWeight}
                step={2.5}
                max={1000}
                decimals={1}
              />
              {/* The number you log is not the thing you do — this is. */}
              {load ? <Text style={s.plates}>{`Per side: ${plateLabel(load)}`}</Text> : null}
              <View style={s.chipRow}>
                {[-5, -2.5, 2.5, 5, 10].map((delta) => (
                  <Pressable
                    key={delta}
                    onPress={() => {
                      const base = Number.parseFloat(weight);
                      const next = Math.max(0, (Number.isFinite(base) ? base : 0) + delta);
                      setWeight(`${Number(next.toFixed(1))}`);
                    }}
                    accessibilityRole="button"
                    accessibilityLabel={`${delta > 0 ? 'Add' : 'Remove'} ${Math.abs(delta)} kilograms`}
                    style={s.chip}
                  >
                    <Text style={s.chipText}>{delta > 0 ? `+${delta}` : `${delta}`}</Text>
                  </Pressable>
                ))}
              </View>
              <View style={s.pairRow}>
                <View style={s.pairCell}>
                  <Stepper label="Reps" value={reps} onChange={setReps} step={1} max={500} size="sm" />
                </View>
                <View style={s.pairCell}>
                  <Stepper label="RPE" value={rpe} onChange={setRpe} step={1} max={10} size="sm" />
                </View>
              </View>
            </>
          ) : modality === 'duration' ? (
            <>
              <Stepper
                label="Seconds"
                value={amount}
                onChange={setAmount}
                step={5}
                max={36_000}
              />
              <View style={s.pairRow}>
                <View style={s.pairCell}>
                  <Button
                    label={
                      watchStartedAt != null
                        ? clockLabel(elapsedSeconds(watchStartedAt, watchNow))
                        : 'Time it'
                    }
                    variant="secondary"
                    onPress={() => {
                      if (watchStartedAt == null) {
                        setWatchStartedAt(Date.now());
                      } else {
                        setAmount(`${Math.max(1, elapsedSeconds(watchStartedAt, Date.now()))}`);
                        setWatchStartedAt(null);
                      }
                    }}
                  />
                </View>
                <View style={s.pairCell}>
                  <Stepper label="RPE" value={rpe} onChange={setRpe} step={1} max={10} size="sm" />
                </View>
              </View>
            </>
          ) : (
            <>
              <Stepper label="Metres" value={amount} onChange={setAmount} step={100} max={1_000_000} />
              <View style={s.pairRow}>
                <View style={s.pairCell}>
                  <Stepper label="RPE" value={rpe} onChange={setRpe} step={1} max={10} size="sm" />
                </View>
                <View style={s.pairCell} />
              </View>
            </>
          )}

          <Button
            label={log.isPending ? 'Logging…' : `Log set ${setNumber}`}
            detail={previewLabel(modality, { reps, weight, amount })}
            disabled={!ready}
            onPress={() => log.mutate()}
          />
          {error ? <Text style={s.formError}>{error}</Text> : null}
        </>
      ) : null}
    </View>
  );
}

/**
 * Pick a movement to add to the session you are already in.
 *
 * Inline rather than a modal or a pushed route, for one reason: the rest timer
 * is running. Leaving the screen to choose an exercise means coming back to a
 * timer you could not see, and the timer is the thing this screen exists to
 * keep in front of you.
 *
 * Exercises already in the session are shown as `In this workout` rather than
 * hidden — a name that vanishes reads as a missing exercise, and the member
 * would go looking for it.
 */
function ExercisePicker({
  catalogue,
  loading,
  exclude,
  onPick,
  onCancel,
}: {
  catalogue: Exercise[];
  loading: boolean;
  exclude: Set<string>;
  onPick: (exercise: Exercise) => void;
  onCancel: () => void;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const [search, setSearch] = useState('');

  const matches = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const found = needle
      ? catalogue.filter((e) => e.name.toLowerCase().includes(needle))
      : catalogue;
    // Ones you have not done yet first: mid-session, the thing you are looking
    // for is almost never one already on the screen behind this list.
    return [...found].sort(
      (a, b) => Number(exclude.has(a.id)) - Number(exclude.has(b.id)),
    );
  }, [catalogue, search, exclude]);

  return (
    <View style={s.picker}>
      <Section label="Add an exercise" count={matches.length} />
      <Field
        label="Search"
        value={search}
        onChangeText={setSearch}
        placeholder="Bench, squat, row…"
        autoCapitalize="none"
        autoCorrect={false}
      />

      {loading ? (
        <ActivityIndicator color={t.color.accent} />
      ) : matches.length === 0 ? (
        <Text style={s.note}>
          {search.trim()
            ? 'Nothing in the catalogue matches that.'
            : 'The gym has no exercises in its catalogue yet.'}
        </Text>
      ) : (
        <View style={s.pickList}>
          {matches.slice(0, 40).map((exercise, i, arr) => {
            const already = exclude.has(exercise.id);
            return (
              <Touchable
                key={exercise.id}
                onPress={() => onPick(exercise)}
                accessibilityLabel={`Add ${exercise.name}, ${MEASURED_IN[exercise.modality.kind]}, to this workout`}
                style={[s.queueRow, i === arr.length - 1 && s.queueRowLast]}
              >
                <View style={s.queueBody}>
                  <Text style={s.queueName} numberOfLines={1}>
                    {exercise.name}
                  </Text>
                  <Text style={s.queuePrescription} numberOfLines={1}>
                    {already ? 'In this workout' : MEASURED_IN[exercise.modality.kind]}
                  </Text>
                </View>
                <Feather name="plus" size={16} color={t.color.mut} />
              </Touchable>
            );
          })}
          {matches.length > 40 ? (
            <Text style={s.noteHint}>
              {`${matches.length - 40} more — search to narrow it down.`}
            </Text>
          ) : null}
        </View>
      )}

      <Button label="Cancel" variant="ghost" onPress={onCancel} />
    </View>
  );
}

/** The CTA's right-hand preview: "70 kg × 8", "45 s", "2 km". */
function previewLabel(
  modality: ModalityKind,
  values: { reps: string; weight: string; amount: string },
): string | undefined {
  if (modality === 'repetitions') {
    if (!/^\d+$/.test(values.reps)) return undefined;
    return values.weight.trim() === ''
      ? `${values.reps} reps`
      : `${values.weight} kg × ${values.reps}`;
  }
  if (!/^\d+$/.test(values.amount)) return undefined;
  return modality === 'duration'
    ? secondsLabel(Number.parseInt(values.amount, 10))
    : `${values.amount} m`;
}

/** "5 × 62.5 kg", "45 s", "2000 m" — what a logged row says. */
function describeSet(performed: PerformedSetPayload): string {
  switch (performed.kind) {
    case 'repetitions':
      return performed.weight_kg != null
        ? `${performed.weight_kg} kg × ${performed.reps}`
        : `${performed.reps} reps`;
    case 'duration':
      return `${performed.seconds} s`;
    case 'distance':
      return `${performed.metres} m`;
    default:
      return 'set';
  }
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    flex: { flex: 1 },
    screen: { backgroundColor: t.color.surface, flex: 1 },
    content: {
      gap: t.space.lg,
      paddingBottom: t.space.huge,
      paddingHorizontal: t.space.gutter,
      paddingTop: t.space.md,
    },
    errorWrap: { padding: t.space.gutter },

    picker: { gap: t.space.sm },
    pickList: { gap: 0 },

    headRow: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    headSpacer: { flex: 1 },
    headKicker: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    clock: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.tight,
    },
    clockStale: { color: t.color.mut, fontFamily: fonts.semibold, fontSize: t.font.sm },
    headBar: { gap: t.space.sm },
    headMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    note: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.md },
    emptyBlock: { gap: t.space.sm },
    noteHint: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },

    // The one exercise you are actually doing.
    focal: {
      ...containerStyle(t, 'focal'),
      gap: t.space.md,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.lg,
    },
    focalHead: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    focalIndex: {
      backgroundColor: t.color.accentHi,
      borderRadius: t.radius.sm,
      color: t.color.accentBadgeInk,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      overflow: 'hidden',
      paddingHorizontal: 7,
      paddingVertical: 4,
    },
    focalName: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 29,
    },
    focalSetOf: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textAlign: 'right',
      textTransform: 'uppercase',
      width: 68,
    },
    target: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },
    targetStrong: { color: t.color.ink, fontFamily: fonts.bold },

    // What you have already banked, in a well of its own — separated from the
    // controls, because reading the last set and pressing + are different jobs.
    setList: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      paddingHorizontal: t.space.md,
    },
    setRow: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: 10,
    },
    setIndex: {
      color: t.color.faint,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      width: 14,
    },
    setText: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm + 0.5,
      fontVariant: ['tabular-nums'],
    },
    setRpe: { color: t.color.mut, fontFamily: fonts.medium, fontSize: t.font.xs },

    fieldLabel: {
      color: t.color.mut2,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
    },
    stepperGroup: { gap: 7 },
    // One trough with the keys inset, rather than three boxes stitched
    // together. The old version drew a 2px border round each third, so the
    // control read as a table; a chalked thumb wants one object.
    stepperRow: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.lg,
      flexDirection: 'row',
      padding: 5,
    },
    stepperRowLg: { minHeight: 66 },
    stepperRowSm: { minHeight: 56 },
    stepperButton: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderRadius: t.radius.md,
      justifyContent: 'center',
      ...t.elevation(1),
    },
    // 56pt tall: this gets pressed with a chalked thumb, mid-set.
    stepperButtonLg: { height: 56, width: 64 },
    stepperButtonSm: { height: 46, width: 46 },
    stepperValueWrap: { alignItems: 'center', flex: 1, justifyContent: 'center' },
    stepperValue: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: 32,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    stepperValueTextSm: { fontSize: 25 },

    plates: {
      color: t.color.accentDeep,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
      marginTop: -t.space.xs,
    },

    chipRow: { flexDirection: 'row', gap: t.space.sm },
    chip: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.pill,
      flex: 1,
      justifyContent: 'center',
      paddingVertical: 11,
    },
    chipText: {
      color: t.color.ink,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },

    pairRow: { flexDirection: 'row', gap: t.space.md },
    pairCell: { flex: 1, justifyContent: 'flex-end' },
    formError: { color: t.color.danger, fontFamily: fonts.semibold, fontSize: t.font.sm },

    queueRow: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.md,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      gap: t.space.md,
      marginBottom: t.space.sm,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.md,
    },
    queueRowLast: { marginBottom: 0 },
    queueIndex: {
      color: t.color.faint,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      width: 20,
    },
    queueBody: { flex: 1, gap: 2 },
    queueName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    queuePrescription: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
    queueCount: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },
    queueCountDone: { color: t.color.success },

    finishBlock: { gap: t.space.sm, paddingTop: t.space.sm },
    restDock: {
      backgroundColor: t.color.surface,
      paddingBottom: t.space.lg,
      paddingHorizontal: t.space.gutter,
      paddingTop: t.space.sm,
    },
  });
