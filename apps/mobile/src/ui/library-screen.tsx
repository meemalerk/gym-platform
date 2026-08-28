import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  FlatList,
  RefreshControl,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { curateExercise, listExercises, type Exercise } from '@/api/gym';
import { can, useActiveMembership, useSession } from '@/session/store';
import {
  Badge,
  Centered,
  EmptyState,
  ErrorBanner,
  IconButton,
  Segmented,
  Touchable,
} from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

type Kind = Exercise['modality']['kind'];

/**
 * The extra filter a curator gets. Not a fifth modality — it cuts across all
 * of them — so it lives in its own union rather than being smuggled into
 * `Kind`, which would let "proposed reps" become unrepresentable nonsense.
 */
type Scope = Kind | 'all' | 'proposed';

/** How a movement is measured, as a word rather than a letter. */
const MODALITY: Record<Kind, string> = {
  repetitions: 'Reps',
  duration: 'Time',
  distance: 'Distance',
};

const FILTERS: { key: Scope; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'repetitions', label: 'Reps' },
  { key: 'duration', label: 'Time' },
  { key: 'distance', label: 'Distance' },
];

/**
 * The exercise catalogue.
 *
 * Lives here rather than in a route file because it is rendered from **two**
 * places: the Library tab (members and coaches) and a pushed stack screen
 * (`/(app)/library`). Managers trade the Library tab for Billing under the
 * five-tab ceiling, so for them the tab does not exist — and Manage used to
 * link at it anyway, which routed nowhere and dumped them back on the
 * dashboard by way of expo-router's unmatched screen.
 *
 * `inTabs` is the only difference between the two: the pushed copy has no tab
 * bar to pad the list around.
 */
export function LibraryScreen({ inTabs = true }: { inTabs?: boolean }) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const insets = useSafeAreaInsets();
  // Hooks cannot be called conditionally, so the height is always computed and
  // only spent when there is a bar to clear.
  const barHeight = useTabBarHeight();
  const tabBarHeight = inTabs ? barHeight : 0;
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];

  const [search, setSearch] = useState('');
  const [focused, setFocused] = useState(false);
  const [filter, setFilter] = useState<Scope>('all');
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: ['exercises', gymId],
    queryFn: () => listExercises(gymId!),
    enabled: Boolean(gymId),
  });

  // Reading the catalogue is open to everyone in the gym — looking up how a
  // movement is performed is not a privileged act. Only editing is gated.
  // Adding a movement is coach-level; it lands as a proposal unless the
  // caller curates. Curation itself stays at head coach (ADR-0024).
  const canManage = can.proposeExercises(capacities);
  const canCurate = can.curateCatalogue(capacities);
  const all = query.data ?? [];

  /*
    Proposals are listed alongside everything else rather than hidden until
    approved. The coach who raised one is prescribing from it today (ADR-0024),
    so a catalogue that omitted it would be lying about what this gym uses —
    the badge is the honest treatment, not concealment.
  */
  const pending = useMemo(() => all.filter((e) => e.status === 'proposed'), [all]);

  const curate = useMutation({
    mutationFn: ({ id, decision }: { id: string; decision: 'approve' | 'retire' }) =>
      curateExercise(gymId!, id, decision),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['exercises', gymId] }),
  });

  const items = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return all.filter((e) => {
      if (filter === 'proposed' && e.status !== 'proposed') return false;
      if (filter !== 'all' && filter !== 'proposed' && e.modality.kind !== filter) return false;
      if (needle === '') return true;
      // Cues are searchable too: people look for "hinge" more often than for
      // the name of a lift they cannot quite remember.
      return (
        e.name.toLowerCase().includes(needle) ||
        (e.notes ?? '').toLowerCase().includes(needle)
      );
    });
  }, [all, filter, search]);

  const filtering = search.trim() !== '' || filter !== 'all';

  // A curator sees one extra option, and only when there is something in it.
  // An always-present "Proposed 0" is a permanent reminder of a job nobody
  // has; one that appears when work arrives is a queue.
  const options: { key: Scope; label: string; count?: number }[] =
    canCurate && pending.length > 0
      ? [...FILTERS, { key: 'proposed' as Scope, label: 'Proposed', count: pending.length }]
      : FILTERS;

  return (
    <View style={s.screen}>
      <View style={[s.header, { paddingTop: insets.top + t.space.md }]}>
        <GymHeader
          title="Library"
          // Only when pushed: the tab copy has nothing behind it. Without this
          // the pushed Library was a dead end too — same defect as Programmes,
          // just older.
          onBack={inTabs ? undefined : () => router.back()}
          meta={`${all.length} ${all.length === 1 ? 'movement' : 'movements'}`}
          subtitle="Every movement this gym programmes. Tap one to see your own history with it."
          action={
            canManage ? (
              <IconButton
                icon="plus"
                tone="accent"
                label="Add a movement to the catalogue"
                onPress={() => router.push('/(app)/new-exercise')}
              />
            ) : null
          }
        />

        <View style={[s.searchWrap, focused && s.searchWrapFocused]}>
          <Feather name="search" size={16} color={t.color.faint} />
          <TextInput
            value={search}
            onChangeText={setSearch}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            placeholder="Search a movement or a cue…"
            placeholderTextColor={t.color.faint}
            autoCapitalize="none"
            autoCorrect={false}
            style={s.search}
            accessibilityLabel="Search the exercise catalogue"
            returnKeyType="search"
          />
        </View>

        <Segmented
          options={options}
          value={filter}
          onChange={setFilter}
          label="Filter the catalogue"
        />
      </View>

      {query.isError ? (
        <View style={s.bannerWrap}>
          <ErrorBanner message="Could not load the catalogue. Pull down to try again." />
        </View>
      ) : null}

      {query.isLoading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : (
        <FlatList
          data={items}
          keyExtractor={(item) => item.id}
          contentContainerStyle={[s.list, { paddingBottom: tabBarHeight + t.space.xl }]}
          showsVerticalScrollIndicator={false}
          keyboardShouldPersistTaps="handled"
          refreshControl={
            <RefreshControl
              refreshing={query.isRefetching}
              onRefresh={() => void query.refetch()}
              tintColor={t.color.mut}
            />
          }
          ListEmptyComponent={
            query.isError ? null : filtering ? (
              <EmptyState
                glyph="⌕"
                title="Nothing matches"
                hint="Try a different word, or set the filter back to All."
              />
            ) : (
              <EmptyState
                glyph="◎"
                title="The catalogue is empty"
                hint={
                  canManage
                    ? 'Add the first movement and this becomes the vocabulary every programme is written in.'
                    : 'Nothing has been added yet. Your coach fills this in.'
                }
              />
            )
          }
          renderItem={({ item }) => (
            /* Tapping a movement opens YOUR history with it — the catalogue
               doubles as the index into your own training data. */
            <Touchable
              onPress={() =>
                router.push({
                  pathname: '/(app)/exercise/[id]',
                  params: { id: item.id, name: item.name },
                })
              }
              accessibilityLabel={`${item.name}. Measured in ${MODALITY[item.modality.kind].toLowerCase()}. See your history and progress.`}
              style={s.row}
            >
              <View style={s.rowBody}>
                <View style={s.rowTitle}>
                  <Text style={s.rowName} numberOfLines={1}>
                    {item.name}
                  </Text>
                  {item.status === 'proposed' ? <Badge tone="warn">Proposed</Badge> : null}
                  {item.status === 'retired' ? <Badge tone="muted">Retired</Badge> : null}
                </View>
                <Text style={s.rowMeta} numberOfLines={1}>
                  {item.notes ? `${MODALITY[item.modality.kind]} · ${item.notes}` : MODALITY[item.modality.kind]}
                </Text>
              </View>
              {/* The decision sits ON the row, where the thing being decided
                  is — a separate review screen would mean holding a name in
                  your head while you go and look for its duplicate. */}
              {canCurate && item.status === 'proposed' ? (
                <View style={s.curate}>
                  <Touchable
                    onPress={() => curate.mutate({ id: item.id, decision: 'approve' })}
                    disabled={curate.isPending}
                    accessibilityRole="button"
                    accessibilityLabel={`Approve ${item.name} for the catalogue`}
                    style={s.curateOk}
                  >
                    <Feather name="check" size={16} color={t.color.onAccent} />
                  </Touchable>
                  <Touchable
                    onPress={() => curate.mutate({ id: item.id, decision: 'retire' })}
                    disabled={curate.isPending}
                    accessibilityRole="button"
                    accessibilityLabel={`Retire ${item.name} — a duplicate or a mistake`}
                    style={s.curateNo}
                  >
                    <Feather name="x" size={16} color={t.color.mut} />
                  </Touchable>
                </View>
              ) : (
                <Feather name="chevron-right" size={18} color={t.color.faint} />
              )}
            </Touchable>
          )}
        />
      )}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    header: { gap: t.space.md, paddingBottom: t.space.md, paddingHorizontal: t.space.gutter },

    searchWrap: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderColor: t.color.sunken,
      borderRadius: t.radius.md,
      borderWidth: t.border.ink,
      flexDirection: 'row',
      gap: t.space.sm,
      minHeight: 48,
      paddingHorizontal: 14,
    },
    searchWrapFocused: { backgroundColor: t.color.surface2, borderColor: t.color.accent },
    search: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.medium,
      fontSize: t.font.md,
      paddingVertical: 0,
    },

    bannerWrap: { paddingBottom: t.space.md, paddingHorizontal: t.space.gutter },
    list: { paddingHorizontal: t.space.gutter },

    // Cards rather than hairline rows: the catalogue is a browse surface, and
    // separate objects are easier to aim a thumb at than a continuous list.
    row: {
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
      ...t.elevation(1),
    },
    rowBody: { flex: 1, gap: 3 },
    rowTitle: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    rowName: {
      color: t.color.ink,
      flexShrink: 1,
      fontFamily: fonts.semibold,
      fontSize: t.font.md,
    },
    rowMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },

    curate: { flexDirection: 'row', gap: t.space.sm },
    curateOk: {
      alignItems: 'center',
      backgroundColor: t.color.accent,
      borderRadius: t.radius.sm,
      height: 36,
      justifyContent: 'center',
      width: 36,
    },
    curateNo: {
      alignItems: 'center',
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.sm,
      height: 36,
      justifyContent: 'center',
      width: 36,
    },
  });
