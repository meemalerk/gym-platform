import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { assignProgram, listAssignments, listPrograms, type Program } from '@/api/gym';
import { useSession } from '@/session/store';
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
} from '@/ui/components';
import { DateField } from '@/ui/fields';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** Today, in the local calendar, as the API's YYYY-MM-DD. */
function localToday(): string {
  const d = new Date();
  const p = (n: number) => `${n}`.padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

const FOCUS_LABEL: Record<string, string> = {
  strength: 'Strength',
  hypertrophy: 'Hypertrophy',
  conditioning: 'Conditioning',
  general: 'General',
};

/**
 * Put **this athlete** on a programme.
 *
 * The mirror of `assign-program.tsx`, and the direction a coach actually works
 * in. That screen starts from a version and asks "who?"; this one starts from
 * a person — because a coach opens their client, sees they have nothing to do
 * this week, and wants to fix *that*, not to go and find a programme first and
 * remember who they were thinking about.
 *
 * Only **published** versions are offered. A draft binds nobody and cannot be
 * assigned (the server refuses it), so listing drafts here would be offering
 * a button that fails.
 *
 * The version is pinned by id, never "the latest": if this programme is edited
 * tomorrow, the athlete stays on the one they were actually given (ADR-0006).
 */
export default function AssignProgramFor() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const { athlete, name } = useLocalSearchParams<{ athlete: string; name?: string }>();

  const [chosen, setChosen] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(localToday());
  const [error, setError] = useState<string | null>(null);

  const programs = useQuery({
    queryKey: ['programs', gymId],
    queryFn: () => listPrograms(gymId!),
    enabled: Boolean(gymId),
  });

  const assignments = useQuery({
    queryKey: ['assignments', gymId],
    queryFn: () => listAssignments(gymId!),
    enabled: Boolean(gymId),
  });

  /** What they are on now — replaced, not added to, when a new one lands. */
  const currently = (assignments.data ?? []).find(
    (a) => a.athlete_id === athlete && a.is_active,
  );

  const publishable = useMemo(
    () =>
      (programs.data ?? []).filter(
        (p: Program) => p.latest_version.status.state === 'published',
      ),
    [programs.data],
  );

  const assign = useMutation({
    mutationFn: (versionId: string) =>
      assignProgram(gymId!, {
        athleteId: athlete,
        programVersionId: versionId,
        startDate,
      }),
    onSuccess: () => {
      // Today's focal card reads from these two, so both have to move or the
      // athlete is told to do a workout they were just taken off.
      void queryClient.invalidateQueries({ queryKey: ['assignments', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['sessions', gymId] });
      router.back();
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'auth.forbidden'
          ? 'You do not coach this athlete. A head coach can pair you with them.'
          : e instanceof ApiError && e.code === 'resource.conflict'
            ? 'They are already on this version.'
            : e instanceof ApiError && e.code === 'request.invalid'
              ? e.message
              : 'Could not assign that. Please try again.',
      );
    },
  });

  if (programs.isLoading) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.accent} />
      </Centered>
    );
  }

  return (
    <Screen scroll>
      <Stack.Screen options={{ title: name ? `Programme for ${name}` : 'Assign a programme' }} />

      {error ? <ErrorBanner message={error} /> : null}

      {currently ? (
        <Callout>
          {name ?? 'They'} are on <Text style={s.strong}>{currently.program_name}</Text> right
          now. Assigning another puts them on the new one from the start date; what they have
          already logged is untouched.
        </Callout>
      ) : null}

      {publishable.length === 0 ? (
        <EmptyState
          glyph="▤"
          title="Nothing published yet"
          hint="Only a published version can be assigned — a draft binds nobody. Write one and publish it, then come back."
          action={
            <Button
              label="Open programmes"
              variant="secondary"
              onPress={() => router.replace('/(app)/programs')}
            />
          }
        />
      ) : (
        <>
          <View>
            <Section label="Published programmes" count={publishable.length} />
            <View style={s.list}>
              {publishable.map((program) => {
                const version = program.latest_version;
                const on = chosen === version.id;
                const isCurrent = currently?.program_version_id === version.id;
                return (
                  <Pressable
                    key={program.id}
                    onPress={() => {
                      setError(null);
                      setChosen(on ? null : version.id);
                    }}
                    disabled={isCurrent}
                    accessibilityRole="radio"
                    accessibilityState={{ selected: on, disabled: isCurrent }}
                    accessibilityLabel={`${program.name}. ${FOCUS_LABEL[program.focus] ?? program.focus}, version ${version.version_number}.`}
                    style={[s.item, on && s.itemOn, isCurrent && s.itemCurrent]}
                  >
                    <View style={s.itemHead}>
                      <Text style={s.itemName} numberOfLines={1}>
                        {program.name}
                      </Text>
                      {isCurrent ? <Badge tone="success">On it now</Badge> : null}
                    </View>
                    <Text style={s.itemMeta} numberOfLines={2}>
                      {FOCUS_LABEL[program.focus] ?? program.focus} · v
                      {version.version_number}
                      {program.summary ? ` · ${program.summary}` : ''}
                    </Text>
                  </Pressable>
                );
              })}
            </View>
          </View>

          {chosen ? (
            <Card>
              <DateField
                label="Starts"
                value={startDate}
                onChange={setStartDate}
                minimumDate={new Date(Date.now() - 30 * 86_400_000)}
              />
              <Button
                label={assign.isPending ? 'Assigning…' : 'Put them on it'}
                disabled={assign.isPending || startDate.trim() === ''}
                onPress={() => assign.mutate(chosen)}
              />
              <Text style={s.footnote}>
                They see it on Today straight away — the next workout in the plan, ready to
                start.
              </Text>
            </Card>
          ) : null}
        </>
      )}
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    strong: { color: t.color.ink, fontFamily: fonts.bold },
    list: { gap: t.space.sm, paddingTop: t.space.sm },
    item: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.md,
      borderWidth: t.border.hair,
      gap: 3,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.md,
      ...t.elevation(1),
    },
    itemOn: { backgroundColor: t.color.accentHi, borderColor: t.color.accent },
    itemCurrent: { opacity: 0.6 },
    itemHead: { alignItems: 'center', flexDirection: 'row', gap: t.space.sm },
    itemName: {
      color: t.color.ink,
      flexShrink: 1,
      fontFamily: fonts.semibold,
      fontSize: t.font.md,
    },
    itemMeta: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs },
    footnote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
  });
