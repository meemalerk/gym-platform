import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { ActivityIndicator, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { listPrograms, type Program } from '@/api/gym';
import { can, useActiveMembership, useSession } from '@/session/store';
import {
  Badge,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  IconButton,
  ListRow,
  Section,
} from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * The programmes this gym has written.
 *
 * Shows the LATEST version's state, because that is what a coach is deciding
 * about: a programme whose newest version is a draft is unfinished work, even
 * if an older version is published and in use. The two facts are different and
 * the row says which it is showing.
 */

/** State → how it reads, and how loud it should be. */
function stateBadge(state: string): {
  label: string;
  tone: 'accent' | 'success' | 'warn' | 'outline' | 'muted';
} {
  switch (state) {
    case 'published':
      return { label: 'Published', tone: 'success' };
    case 'approved':
      return { label: 'Approved', tone: 'accent' };
    case 'in_review':
      return { label: 'In review', tone: 'warn' };
    case 'archived':
      return { label: 'Archived', tone: 'muted' };
    default:
      return { label: 'Draft', tone: 'outline' };
  }
}

const FOCUS_LABEL: Record<string, string> = {
  strength: 'Strength',
  hypertrophy: 'Hypertrophy',
  conditioning: 'Conditioning',
  general: 'General',
};

export default function Programs() {
  const t = useTokens();
  // This screen draws its own GymHeader, so the NATIVE header is hidden (see
  // the layout) — which means nothing else is reserving the status bar and the
  // notch. Without this the title sits under the clock. Same reason and the
  // same numbers as `library-screen.tsx`, which hides its header for the same
  // reason.
  const insets = useSafeAreaInsets();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const active = useActiveMembership();
  // Writing a programme is coach-level (ADR-0024); only publishing is not.
  const canAuthor = can.authorPrograms(active?.capacities ?? []);

  const query = useQuery({
    queryKey: ['programs', gymId],
    queryFn: () => listPrograms(gymId!),
    enabled: Boolean(gymId),
  });

  const programs = query.data ?? [];

  return (
    <View style={s.screen}>
      <View style={[s.header, { paddingTop: insets.top + t.space.md }]}>
        <GymHeader
          title="Programmes"
          // Pushed, and the native header is hidden — so this header carries
          // the only way back.
          onBack={() => router.back()}
          meta={`${programs.length} ${programs.length === 1 ? 'programme' : 'programmes'}`}
          action={
            canAuthor ? (
              <IconButton
                icon="plus"
                tone="accent"
                label="Write a new programme"
                onPress={() => router.push('/(app)/new-program')}
              />
            ) : null
          }
        />
      </View>

      {query.isLoading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : (
        <ScrollView
          contentContainerStyle={s.body}
          showsVerticalScrollIndicator={false}
          refreshControl={
            <RefreshControl
              refreshing={query.isRefetching}
              onRefresh={() => void query.refetch()}
              tintColor={t.color.mut}
            />
          }
        >
          {query.isError ? <ErrorBanner message="Could not load programmes." /> : null}

          {programs.length === 0 && !query.isError ? (
            <EmptyState
              glyph="▤"
              title="No programmes yet"
              hint={
                canAuthor
                  ? 'A programme is weeks of workouts. Write one, publish it, then put athletes on it.'
                  : 'Your coaches have not published anything here yet.'
              }
            />
          ) : (
            <View>
              <Section label="All programmes" count={programs.length} />
              <Card padded={false}>
              {programs.map((program: Program, i, arr) => {
                const badge = stateBadge(program.latest_version.status.state);
                const version = program.latest_version;
                return (
                  <ListRow
                    key={program.id}
                    title={program.name}
                    subtitle={`${FOCUS_LABEL[program.focus] ?? program.focus} · v${version.version_number}${
                      program.summary ? ` · ${program.summary}` : ''
                    }`}
                    last={i === arr.length - 1}
                    right={<Badge tone={badge.tone}>{badge.label}</Badge>}
                    onPress={() =>
                      router.push({
                        pathname: '/(app)/program/[version]',
                        params: {
                          version: version.id,
                          name: program.name,
                          programId: program.id,
                        },
                      })
                    }
                  />
                );
              })}
              </Card>
            </View>
          )}

          {canAuthor && programs.length > 0 ? (
            <Text style={s.note}>
              A published version can never be edited — editing one starts a new draft, and
              everyone already on the old version stays on it.
            </Text>
          ) : null}
        </ScrollView>
      )}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    // paddingTop comes from the safe-area inset at the call site.
    header: { paddingHorizontal: t.space.gutter },
    body: {
      gap: t.space.lg,
      paddingBottom: t.space.xl,
      paddingHorizontal: t.space.gutter,
      paddingTop: t.space.md,
    },
    note: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 18,
    },
  });
