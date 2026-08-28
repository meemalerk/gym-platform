import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import * as Crypto from 'expo-crypto';
import { Stack, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  archiveClass,
  bookClass,
  cancelClassBooking,
  listClasses,
  type ClassOccurrence,
} from '@/api/gym';
import {
  byDay,
  durationLabel,
  occupancyLabel,
  timeLabel,
  todayLocal,
  windowEnd,
  type Sitting,
} from '@/features/classes/timetable';
import { can, useSession } from '@/session/store';
import {
  Badge,
  Button,
  Callout,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  Screen,
  Section,
  Touchable,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** Two weeks: enough to plan around, short enough to scroll. */
const WINDOW_DAYS = 13;

/**
 * The class timetable — browse it, take a place, give one back.
 *
 * One screen for all three roles, because the timetable is the same object for
 * everybody and three copies of it would drift. What changes is the row's
 * trailing control: a member gets Book/Booked, the class's own instructor gets
 * a roster, a manager gets both plus a way to drop the class.
 */
export default function ClassesScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const userId = useSession((st) => st.user?.id);
  const capacities = useSession((st) => st.membership?.capacities ?? []);
  const manages = can.manageGym(capacities);

  const from = todayLocal(new Date());
  const to = windowEnd(from, WINDOW_DAYS);

  const [error, setError] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);

  const timetable = useQuery({
    queryKey: ['classes', gymId, from, to],
    queryFn: () => listClasses(gymId!, from, to),
    enabled: Boolean(gymId),
  });

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['classes', gymId] });
  };

  /*
    One mutation for both directions. Booking and cancelling are the same
    gesture from the row's point of view, and keeping them together means one
    busy state and one error surface rather than two that can disagree.

    The row carries `my_booking_id`, so cancelling needs no lookup — that field
    exists precisely because a screen that can show "Booked" must also be able
    to undo it.
  */
  const toggle = useMutation({
    mutationFn: (sitting: ClassOccurrence) =>
      sitting.my_booking_id
        ? cancelClassBooking(gymId!, sitting.my_booking_id)
        : bookClass(gymId!, sitting.class_id, {
            bookingId: Crypto.randomUUID(),
            onDate: sitting.on_date,
          }),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError
          ? e.message
          : 'Could not update that booking. It is safe to try again.',
      );
    },
    onSettled: () => setBusyKey(null),
  });

  const drop = useMutation({
    mutationFn: (classId: string) => archiveClass(gymId!, classId),
    onSuccess: () => {
      setError(null);
      refresh();
    },
    onError: () => setError('Could not take that class off the timetable.'),
  });

  const days = useMemo(
    () => byDay((timetable.data ?? []) as Sitting[]),
    [timetable.data],
  );

  if (timetable.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: 'Classes' }} />
      <View style={s.content}>
        {error ? <ErrorBanner message={error} /> : null}

        {manages ? (
          <Button
            label="Add a class"
            variant="secondary"
            onPress={() => router.push('/(app)/new-class')}
          />
        ) : null}

        {timetable.isError ? (
          <ErrorBanner message="Could not load the timetable." />
        ) : days.length === 0 ? (
          <EmptyState
            glyph="◱"
            title="No classes on the timetable"
            hint={
              manages
                ? 'Add one and it will show up here for every member.'
                : 'Nothing scheduled for the next two weeks.'
            }
          />
        ) : (
          days.map((day) => (
            <View key={day.date} style={s.group}>
              <Section label={day.label} meta={humanDate(day.date)} />
              <Card padded={false}>
                {day.sittings.map((sitting, i) => (
                  <ClassRow
                    key={`${sitting.class_id}-${sitting.on_date}`}
                    sitting={sitting}
                    last={i === day.sittings.length - 1}
                    canSeeRoster={manages || sitting.instructor_id === userId}
                    canDrop={manages}
                    busy={busyKey === rowKey(sitting)}
                    onBook={() => {
                      setBusyKey(rowKey(sitting));
                      toggle.mutate(sitting);
                    }}
                    onRoster={() =>
                      router.push({
                        pathname: '/(app)/class-roster',
                        params: {
                          classId: sitting.class_id,
                          onDate: sitting.on_date,
                          name: sitting.name,
                        },
                      })
                    }
                    onDrop={() => drop.mutate(sitting.class_id)}
                  />
                ))}
              </Card>
            </View>
          ))
        )}

        {days.length > 0 ? (
          <Callout tone="accent">
            A place can be given back any time before the class starts — from
            your bookings on Today.
          </Callout>
        ) : null}
      </View>
    </Screen>
  );
}

const rowKey = (s: ClassOccurrence) => `${s.class_id}-${s.on_date}`;

/** "26 Aug" — the weekday is already the section heading. */
function humanDate(date: string): string {
  const parts = date.split('-').map(Number);
  return new Date(parts[0] ?? 1970, (parts[1] ?? 1) - 1, parts[2] ?? 1).toLocaleDateString(
    undefined,
    { day: 'numeric', month: 'short' },
  );
}

function ClassRow({
  sitting,
  last,
  canSeeRoster,
  canDrop,
  busy,
  onBook,
  onRoster,
  onDrop,
}: {
  sitting: ClassOccurrence;
  last: boolean;
  canSeeRoster: boolean;
  canDrop: boolean;
  busy: boolean;
  onBook: () => void;
  onRoster: () => void;
  onDrop: () => void;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);

  return (
    <View style={[s.row, !last && s.rowLine]}>
      <View style={s.rowTime}>
        <Text style={s.rowTimeText}>{timeLabel(sitting.starts_at)}</Text>
        <Text style={s.rowDuration}>{durationLabel(sitting.duration_minutes)}</Text>
      </View>

      <View style={s.rowBody}>
        <Text style={s.rowName} numberOfLines={1}>
          {sitting.name}
        </Text>
        <Text style={s.rowMeta} numberOfLines={1}>
          {`${sitting.instructor_name} · ${occupancyLabel(sitting)}`}
        </Text>
      </View>

      <View style={s.rowActions}>
        {sitting.booked_by_me ? (
          // Tappable, because "Booked" with no way out is the state members
          // complain about. Styled quietly: giving a place back is not the
          // action the screen is encouraging.
          <Touchable
            onPress={onBook}
            accessibilityLabel={`Give up your place in ${sitting.name}, ${timeLabel(sitting.starts_at)}`}
            style={s.bookedButton}
          >
            <Text style={s.bookedText}>{busy ? '…' : 'Booked'}</Text>
          </Touchable>
        ) : sitting.is_full ? (
          <Badge tone="muted">Full</Badge>
        ) : (
          <Touchable
            onPress={onBook}
            accessibilityLabel={`Book ${sitting.name}, ${timeLabel(sitting.starts_at)}, ${sitting.places_left} places left`}
            style={s.bookButton}
          >
            <Text style={s.bookText}>{busy ? '…' : 'Book'}</Text>
          </Touchable>
        )}

        {canSeeRoster ? (
          <Touchable
            onPress={onRoster}
            accessibilityLabel={`See who is booked into ${sitting.name}`}
            style={s.iconTap}
          >
            <Feather name="users" size={16} color={t.color.mut} />
          </Touchable>
        ) : null}

        {canDrop ? (
          <Touchable
            onPress={onDrop}
            accessibilityLabel={`Take ${sitting.name} off the timetable`}
            style={s.iconTap}
          >
            <Feather name="trash-2" size={16} color={t.color.mut} />
          </Touchable>
        ) : null}
      </View>
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    content: { gap: t.space.lg, paddingBottom: t.space.huge },
    group: { gap: t.space.sm },

    row: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.md,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.md,
    },
    rowLine: {
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
    },
    rowTime: { alignItems: 'flex-start', width: 52 },
    rowTimeText: {
      color: t.color.ink,
      fontFamily: fonts.bold,
      fontSize: t.font.md,
      fontVariant: ['tabular-nums'],
    },
    rowDuration: { color: t.color.faint, fontFamily: fonts.regular, fontSize: t.font.xxs },
    rowBody: { flex: 1, gap: 2 },
    rowName: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    rowMeta: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
    rowActions: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },

    bookButton: {
      backgroundColor: t.color.accent,
      borderRadius: t.radius.pill,
      paddingHorizontal: t.space.md,
      paddingVertical: 7,
    },
    bookText: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },
    bookedButton: {
      backgroundColor: t.color.successHi,
      borderRadius: t.radius.pill,
      paddingHorizontal: t.space.md,
      paddingVertical: 7,
    },
    bookedText: {
      color: t.color.successInk,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },
    iconTap: { padding: 6 },
  });
