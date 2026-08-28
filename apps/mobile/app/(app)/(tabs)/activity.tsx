import Feather from '@expo/vector-icons/Feather';
import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  RefreshControl,
  SectionList,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { listAudit, type AuditEntry } from '@/api/gym';
import {
  CATEGORIES,
  CATEGORY_LABEL,
  countsByCategory,
  filterByCategory,
  groupByDay,
  metaString,
  timeAgo,
  type Category,
} from '@/features/activity/format';
import { useSession } from '@/session/store';
import { Centered, EmptyState, ErrorBanner, Segmented } from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

type IconName = React.ComponentProps<typeof Feather>['name'];

/** "19:22" — the clock time an entry was recorded, in the reader's zone. */
const timeOfDay = (iso: string): string => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? '--:--'
    : d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
};

/**
 * How each recorded action reads to a human.
 *
 * The server writes machine actions (`exercise.created`). Showing those raw is
 * how audit trails end up unread — the value of the log is that a gym owner can
 * scan it without a key to the schema.
 */
const ACTIONS: Record<string, { icon: IconName; describe: (e: AuditEntry) => string }> = {
  'gym.created': { icon: 'home', describe: () => 'created this gym' },
  'exercise.created': {
    icon: 'plus-circle',
    describe: (e) => `added ${metaString(e, 'name') ?? 'an exercise'} to the catalogue`,
  },
  'invitation.created': {
    icon: 'user-plus',
    describe: (e) => `invited ${metaString(e, 'email') ?? 'someone'}`,
  },
  'invitation.accepted': {
    icon: 'check-circle',
    describe: (e) => `${metaString(e, 'email') ?? 'someone'} accepted an invitation`,
  },
  'capacity.granted': { icon: 'shield', describe: () => 'was granted new capacities' },

  'program.created': {
    icon: 'clipboard',
    describe: (e) => `created the programme ${metaString(e, 'name') ?? ''}`.trim(),
  },
  'program_version.created': { icon: 'copy', describe: () => 'started a new programme draft' },
  'program_version.submitted': {
    icon: 'send',
    describe: () => 'submitted a programme version for review',
  },
  'program_version.approved': {
    icon: 'thumbs-up',
    describe: () => 'approved a programme version',
  },
  'program_version.returned_to_draft': {
    icon: 'corner-up-left',
    describe: () => 'sent a programme version back to draft',
  },
  'program_version.published': {
    icon: 'award',
    describe: () => 'published a programme version',
  },
  'program_version.archived': {
    icon: 'archive',
    describe: () => 'archived a programme version',
  },

  'coach_relationship.created': { icon: 'link', describe: () => 'paired a coach with an athlete' },
  'coach_relationship.ended': {
    icon: 'link-2',
    describe: () => 'ended a coaching relationship',
  },
  'program.assigned': {
    icon: 'user-check',
    describe: () => 'put an athlete on a programme',
  },
  'program_assignment.withdrawn': {
    icon: 'user-minus',
    describe: () => 'took an athlete off a programme',
  },

  'workout_session.started': { icon: 'play-circle', describe: () => 'started a workout' },
  'workout_session.completed': {
    icon: 'check-circle',
    describe: () => 'completed a workout',
  },
  'workout_session.abandoned': {
    icon: 'x-circle',
    describe: () => 'cut a workout short',
  },

  'goal.created': { icon: 'target', describe: () => 'set a goal' },
  'goal.achieved': { icon: 'award', describe: () => 'confirmed a goal achieved' },
  'goal.abandoned': { icon: 'slash', describe: () => 'let a goal go' },
};

export default function Activity() {
  const t = useTokens();
  const styles = useStyles(styleFactory);
  const insets = useSafeAreaInsets();
  const tabBarHeight = useTabBarHeight();
  const gymId = useSession((s) => (s.membership?.gymId ?? null));
  const [category, setCategory] = useState<Category>('all');

  const query = useQuery({
    queryKey: ['audit', gymId],
    queryFn: () => listAudit(gymId!),
    enabled: Boolean(gymId),
  });

  const entries = useMemo(() => query.data ?? [], [query.data]);
  const counts = useMemo(() => countsByCategory(entries), [entries]);

  /*
    `now` is captured once per data change rather than read inside the render.
    Reading the clock during render makes every row's "2h ago" recompute on any
    unrelated re-render, and makes the whole screen impossible to reason about
    when a row is one minute from flipping label.
  */
  const sections = useMemo(() => {
    const now = new Date();
    return groupByDay(filterByCategory(entries, category), now).map((section) => ({
      ...section,
      now,
    }));
  }, [entries, category]);

  return (
    <View style={styles.container}>
      <View style={[styles.headerPad, { paddingTop: insets.top + 12 }]}>
        <GymHeader
          title="Activity"
          meta="Managers only"
          subtitle="Every change anyone made here, written in the same transaction as the change itself."
        />
      </View>

      {/*
        Filters are only worth their vertical space once there is something to
        filter. Below a handful of entries the chips are pure noise.
      */}
      {entries.length > 4 ? (
        <View style={styles.filters}>
          <Segmented
            options={CATEGORIES.map((option) => ({
              key: option,
              label: CATEGORY_LABEL[option],
              // The count travels with the label rather than hiding an empty
              // filter: a row that reshuffles as entries arrive is harder to
              // aim at than one that stays put and says "0".
              count: counts[option],
            }))}
            value={category}
            onChange={setCategory}
            label="Filter the audit trail"
          />
        </View>
      ) : null}

      {query.isError ? (
        <View style={styles.bannerWrap}>
          <ErrorBanner message="Could not load the audit trail." />
        </View>
      ) : null}

      {query.isLoading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : (
        <SectionList
          sections={sections}
          keyExtractor={(item) => item.id}
          contentContainerStyle={[styles.list, { paddingBottom: tabBarHeight + t.space.xl }]}
          showsVerticalScrollIndicator={false}
          stickySectionHeadersEnabled
          refreshControl={
            <RefreshControl
              refreshing={query.isRefetching}
              onRefresh={() => void query.refetch()}
              tintColor={t.color.mut}
            />
          }
          renderSectionHeader={({ section }) => (
            <View style={styles.dayHeader}>
              <Text style={styles.dayHeaderText}>{section.title}</Text>
              <Text style={styles.dayHeaderCount}>
                {section.data.length} {section.data.length === 1 ? 'change' : 'changes'}
              </Text>
            </View>
          )}
          renderSectionFooter={() => <View style={styles.daySpacer} />}
          ListEmptyComponent={
            query.isError ? null : (
              <EmptyState
                glyph="◌"
                title={category === 'all' ? 'Nothing recorded yet' : 'Nothing in this filter'}
                hint={
                  category === 'all'
                    ? 'Changes to this gym will appear here as they happen.'
                    : 'Try another filter to see other changes.'
                }
              />
            )
          }
          renderItem={({ item, index, section }) => {
            const known = ACTIONS[item.action];
            const actor = item.actor_name ?? 'Someone';
            // Unknown actions still render: a trail that silently drops rows it
            // does not recognise is worse than a slightly ugly one.
            const description = known ? known.describe(item) : `performed ${item.action}`;
            const last = index === section.data.length - 1;

            const first = index === 0;

            return (
              /*
                A time column beside a sentence, not a timeline rail of icon
                bubbles. An audit trail is read by scanning down the left edge
                for "when", and clock times line up where relative ones ("2h
                ago") do not.

                The day's entries share one card, so the eye gets a block per
                day rather than one unbroken ribbon of rows from last Tuesday
                to this morning.
              */
              <View
                style={[
                  styles.row,
                  first && styles.rowFirst,
                  last && styles.rowLast,
                ]}
              >
                <Text style={styles.time}>{timeOfDay(item.occurred_at)}</Text>
                <Text style={styles.text}>
                  <Text style={styles.actor}>{actor}</Text> {description}
                </Text>
              </View>
            );
          }}
        />
      )}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    container: { backgroundColor: t.color.surface, flex: 1 },
    headerPad: { paddingHorizontal: t.space.gutter },
    bannerWrap: { paddingHorizontal: t.space.gutter },

    /*
      A plain wrapping row, NOT a horizontal ScrollView.

      The scroller cost two bugs for no benefit. It sized itself to exactly its
      content height, and with `overflow-y: hidden` any sub-pixel rounding
      sheared the chips' 1px top border clean off; before that, its cross-axis
      stretch squashed the chips until their labels vanished. There are four
      short labels here — they fit on one line and wrap if the type scales up.
      Library's filter row has always been a plain row; this now matches it.
    */
    filters: {
      paddingBottom: t.space.md,
      paddingHorizontal: t.space.gutter,
      paddingTop: t.space.sm,
    },

    list: { paddingHorizontal: t.space.gutter },
    dayHeader: {
      alignItems: 'center',
      backgroundColor: t.color.surface,
      flexDirection: 'row',
      gap: t.space.sm,
      justifyContent: 'space-between',
      paddingBottom: t.space.sm,
      paddingTop: t.space.lg,
    },
    dayHeaderText: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.md,
      letterSpacing: t.tracking.tight,
    },
    dayHeaderCount: { color: t.color.mut, fontFamily: fonts.medium, fontSize: t.font.xs },
    daySpacer: { height: t.space.xs },

    row: {
      backgroundColor: t.color.surface2,
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      borderLeftColor: t.color.line,
      borderLeftWidth: t.border.hair,
      borderRightColor: t.color.line,
      borderRightWidth: t.border.hair,
      flexDirection: 'row',
      gap: t.space.md,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.md,
    },
    rowFirst: {
      borderTopColor: t.color.line,
      borderTopLeftRadius: t.radius.lg,
      borderTopRightRadius: t.radius.lg,
      borderTopWidth: StyleSheet.hairlineWidth,
    },
    rowLast: {
      borderBottomLeftRadius: t.radius.lg,
      borderBottomRightRadius: t.radius.lg,
      borderBottomWidth: StyleSheet.hairlineWidth,
    },
    time: {
      color: t.color.mut,
      fontFamily: fonts.medium,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      paddingTop: 1,
      width: 42,
    },
    text: {
      color: t.color.mut2,
      flex: 1,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 19,
    },
    actor: { color: t.color.ink, fontFamily: fonts.semibold },
  });
