import { useQuery } from '@tanstack/react-query';
import { Stack, useLocalSearchParams } from 'expo-router';
import { useMemo } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet, Text, View } from 'react-native';

import { getExerciseHistory, type ExerciseHistoryEntry } from '@/api/gym';
import {
  bestOf,
  sessionPoints,
  trendOf,
  type PerformedLike,
} from '@/features/progress/metrics';
import { clockLabel } from '@/features/timer/clock';
import { useSession } from '@/session/store';
import { Card, Centered, EmptyState, ErrorBanner, Section, StatRow } from '@/ui/components';
import { TrendChart } from '@/ui/trend-chart';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Your history with one exercise. Every number here is computed from raw sets —
 * nothing on this screen exists anywhere but the maths (ADR-0018).
 */
export default function ExerciseProgressScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const { id: exerciseId, name } = useLocalSearchParams<{ id: string; name?: string }>();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));

  const history = useQuery({
    queryKey: ['exercise-history', gymId, exerciseId],
    queryFn: () => getExerciseHistory(gymId!, exerciseId),
    enabled: Boolean(gymId && exerciseId),
  });

  const entries: ExerciseHistoryEntry[] = useMemo(() => history.data ?? [], [history.data]);
  const points = useMemo(() => sessionPoints(entries), [entries]);
  const best = bestOf(points);
  const trend = trendOf(points);
  // The last twelve sessions: older points shrink the recent ones into noise
  // at phone width.
  const chartPoints = points.slice(-12);
  const kind = best?.best.kind;
  const latest = points[points.length - 1];

  if (history.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  return (
    <ScrollView
      style={s.screen}
      contentContainerStyle={s.content}
      showsVerticalScrollIndicator={false}
    >
      <Stack.Screen options={{ title: name ?? 'Progress' }} />

      {history.isError ? <ErrorBanner message="Could not load your history." /> : null}

      {!history.isError && entries.length === 0 ? (
        <EmptyState
          glyph="◔"
          title="Nothing logged yet"
          hint="Sets you log for this exercise will build your history here."
        />
      ) : null}

      {points.length > 0 ? (
        <>
          <StatRow
            stats={[
              {
                label: kind === 'repetitions' ? 'Est. 1RM' : 'Best',
                value: latest ? scoreLabel(latest.score, kind, true) : '—',
                unit: kind === 'repetitions' ? 'kg' : undefined,
                context: trend
                  ? `${trend.delta >= 0 ? '↗ +' : '↘ '}${scoreLabel(Math.abs(trend.delta), kind, true)} since the first`
                  : undefined,
                delta: Boolean(trend),
              },
              {
                label: 'Best set',
                value: best ? setLabel(best.best) : '—',
                context: best ? shortDate(best.startedAt) : undefined,
              },
            ]}
          />

          <View>
            <Section
              label={kind === 'repetitions' ? 'Est. 1RM trend' : 'Trend'}
              meta={`${chartPoints.length} sessions`}
            />
            <Card>
              <TrendChart
                values={chartPoints.map((p) => p.score)}
                startLabel={
                  chartPoints[0]
                    ? `${shortDate(chartPoints[0].startedAt)} · ${scoreLabel(chartPoints[0].score, kind, true)}`
                    : undefined
                }
                endLabel={
                  chartPoints[chartPoints.length - 1]
                    ? `${shortDate(chartPoints[chartPoints.length - 1]!.startedAt)} · ${scoreLabel(chartPoints[chartPoints.length - 1]!.score, kind, true)}`
                    : undefined
                }
                accessibilityLabel={`Trend over ${chartPoints.length} sessions, now ${latest ? scoreLabel(latest.score, kind) : 'unknown'}`}
              />
            </Card>
          </View>

          <View>
            <Section label="History" meta={`${entries.length} sessions`} />
            <Card padded={false}>
              {[...entries].reverse().map((entry, i, arr) => (
                <View
                  key={entry.session_id}
                  style={[s.entry, i === arr.length - 1 && s.entryLast]}
                >
                  <View style={s.entryHead}>
                    <Text style={s.entryDate}>{longDate(entry.started_at)}</Text>
                    {entry.session_status !== 'completed' ? (
                      <Text style={s.entryStatus}>
                        {entry.session_status.replace('_', ' ')}
                      </Text>
                    ) : null}
                  </View>
                  <Text style={s.entrySets} numberOfLines={2}>
                    {summariseSets(entry)}
                  </Text>
                </View>
              ))}
            </Card>
          </View>
        </>
      ) : entries.length > 0 ? (
        // History exists but nothing is comparable (bodyweight-only, failures).
        <EmptyState
          glyph="◔"
          title="Logged, but nothing to chart"
          hint="Sets without a load or a time cannot be compared across sessions yet."
        />
      ) : null}
    </ScrollView>
  );
}

/** "4 sets · top 72.5 kg × 6 · RPE 8" — a session in one line. */
function summariseSets(entry: ExerciseHistoryEntry): string {
  const count = entry.sets.length;
  if (count === 0) return 'No sets';

  const top = [...entry.sets].sort((a, b) => {
    const av = a.performed as PerformedLike;
    const bv = b.performed as PerformedLike;
    const score = (p: PerformedLike) =>
      p.kind === 'repetitions' ? (p.weight_kg ?? 0) : p.kind === 'duration' ? p.seconds : p.metres;
    return score(bv) - score(av);
  })[0]!;

  const rpe = top.rpe != null ? ` · RPE ${top.rpe}` : '';
  return `${count} ${count === 1 ? 'set' : 'sets'} · top ${setLabel(top.performed as PerformedLike)}${rpe}`;
}

/** A score in the unit its modality speaks: kg, m:ss, or metres. */
function scoreLabel(score: number, kind: PerformedLike['kind'] | undefined, terse = false): string {
  if (kind === 'duration') return clockLabel(score);
  if (kind === 'distance') return score >= 1000 ? `${Math.round(score / 100) / 10} km` : `${score} m`;
  return terse ? `${score}` : `${score} kg`;
}

/** "72.5 kg × 5", "1:15 hold", "2000 m" — one logged set, as said aloud. */
function setLabel(performed: PerformedLike): string {
  switch (performed.kind) {
    case 'repetitions':
      return performed.weight_kg != null
        ? `${performed.weight_kg} kg × ${performed.reps}`
        : `${performed.reps} reps`;
    case 'duration':
      return `${clockLabel(performed.seconds)} hold`;
    case 'distance':
      return `${performed.metres} m`;
    default:
      return 'set';
  }
}

function shortDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ''
    : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

function longDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? 'Unknown date'
    : d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'long' });
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    content: {
      gap: t.space.xl,
      paddingBottom: t.space.huge,
      paddingHorizontal: t.space.gutter,
      paddingTop: t.space.md,
    },
    entry: {
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      gap: 3,
      paddingVertical: t.space.md,
    },
    entryLast: { borderBottomWidth: 0 },
    entryHead: { alignItems: 'baseline', flexDirection: 'row', gap: t.space.sm },
    entryDate: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm + 0.5,
    },
    entryStatus: {
      color: t.color.warn,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
    entrySets: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },
  });
