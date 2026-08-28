import { useQuery } from '@tanstack/react-query';
import { Stack, useLocalSearchParams } from 'expo-router';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { getClassRoster } from '@/api/gym';
import { useSession } from '@/session/store';
import {
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  InitialsSquare,
  ListRow,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Who is booked into one sitting.
 *
 * The class's own instructor or a manager — not every trainer. A roster is a
 * list of members by name, and a trainer with no involvement in that class has
 * no reason to read it; the server enforces that and this screen simply shows
 * whatever it is given.
 */
export default function ClassRosterScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const { classId, onDate, name } = useLocalSearchParams<{
    classId: string;
    onDate: string;
    name?: string;
  }>();
  const gymId = useSession((st) => st.membership?.gymId ?? null);

  const roster = useQuery({
    queryKey: ['class-roster', gymId, classId, onDate],
    queryFn: () => getClassRoster(gymId!, classId, onDate),
    enabled: Boolean(gymId && classId && onDate),
  });

  if (roster.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  const entries = roster.data ?? [];

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: name ?? 'Roster' }} />
      <View style={s.content}>
        {roster.isError ? (
          <ErrorBanner message="Could not load the roster for this class." />
        ) : entries.length === 0 ? (
          <EmptyState
            glyph="◍"
            title="Nobody booked yet"
            hint="Places taken for this sitting will appear here."
          />
        ) : (
          <View style={s.group}>
            <Section label="Booked in" count={entries.length} meta={humanDate(onDate)} />
            <Card padded={false}>
              {entries.map((entry, i) => (
                <ListRow
                  key={entry.member_id}
                  title={entry.member_name}
                  left={<InitialsSquare name={entry.member_name} />}
                  last={i === entries.length - 1}
                />
              ))}
            </Card>
            <Text style={s.footnote}>
              A member can give their place back until the class starts, so this
              can change right up to the hour.
            </Text>
          </View>
        )}
      </View>
    </Screen>
  );
}

/** "Mon 31 Aug" — the roster is for one sitting, so the date is the subject. */
function humanDate(date: string | undefined): string {
  if (!date) return '';
  const parts = date.split('-').map(Number);
  return new Date(parts[0] ?? 1970, (parts[1] ?? 1) - 1, parts[2] ?? 1).toLocaleDateString(
    undefined,
    { weekday: 'short', day: 'numeric', month: 'short' },
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    content: { gap: t.space.lg, paddingBottom: t.space.huge },
    group: { gap: t.space.sm },
    footnote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
  });
