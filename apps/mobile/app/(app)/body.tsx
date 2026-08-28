import Feather from '@expo/vector-icons/Feather';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack } from 'expo-router';
import { useMemo, useState } from 'react';
import { ActivityIndicator, Alert, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  deleteMeasurement,
  getMyProfiles,
  listMeasurements,
  saveMeasurement,
  type BodyMeasurement,
} from '@/api/gym';
import { bmi } from '@/features/progress/metrics';
import {
  Button,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  Section,
  StatRow,
  Touchable,
} from '@/ui/components';
import { NumberField } from '@/ui/fields';
import { TrendChart } from '@/ui/trend-chart';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** Today as the API's YYYY-MM-DD — the member's local calendar, deliberately. */
function localToday(): string {
  const d = new Date();
  const mm = `${d.getMonth() + 1}`.padStart(2, '0');
  const dd = `${d.getDate()}`.padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

const shortDate = (iso: string) => {
  const d = new Date(`${iso}T00:00:00`);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
};

/**
 * Body tracking: weight, BMI, body fat, girths. BMI is computed from profile
 * height and shown WITH its formula — a number, not a judgement.
 */
export default function BodyScreen() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const queryClient = useQueryClient();

  const measurements = useQuery({ queryKey: ['measurements'], queryFn: listMeasurements });
  const profiles = useQuery({ queryKey: ['my-profiles'], queryFn: getMyProfiles });

  const [weight, setWeight] = useState('');
  const [bodyFat, setBodyFat] = useState('');
  const [waist, setWaist] = useState('');
  const [error, setError] = useState<string | null>(null);

  const rows: BodyMeasurement[] = useMemo(() => measurements.data ?? [], [measurements.data]);
  const latest = rows[0];
  const heightCm = profiles.data?.athlete?.height_cm ?? null;
  const currentBmi =
    latest?.weight_kg != null && heightCm != null ? bmi(latest.weight_kg, heightCm) : null;

  // Oldest → newest for the trend, last 14 entries carrying a weight.
  const weightPoints = useMemo(
    () => [...rows].reverse().filter((m) => m.weight_kg != null).slice(-14),
    [rows],
  );

  /** "↘ 2.4 in 3 wks" — the change over the window actually charted. */
  const weightDelta = useMemo(() => {
    if (weightPoints.length < 2) return null;
    const first = weightPoints[0]!;
    const last = weightPoints[weightPoints.length - 1]!;
    const delta = (last.weight_kg ?? 0) - (first.weight_kg ?? 0);
    const days = Math.round(
      (new Date(`${last.measured_on}T00:00:00`).getTime() -
        new Date(`${first.measured_on}T00:00:00`).getTime()) /
        86_400_000,
    );
    const span = days >= 14 ? `${Math.round(days / 7)} wks` : `${days} days`;
    return `${delta > 0 ? '↗ +' : delta < 0 ? '↘ ' : ''}${Math.abs(Number(delta.toFixed(1)))} in ${span}`;
  }, [weightPoints]);

  const save = useMutation({
    mutationFn: () =>
      saveMeasurement(localToday(), {
        weight_kg: weight.trim() === '' ? null : Number.parseFloat(weight),
        body_fat_percent: bodyFat.trim() === '' ? null : Number.parseFloat(bodyFat),
        waist_cm: waist.trim() === '' ? null : Number.parseFloat(waist),
      }),
    onSuccess: () => {
      setError(null);
      setWeight('');
      setBodyFat('');
      setWaist('');
      void queryClient.invalidateQueries({ queryKey: ['measurements'] });
    },
    onError: (e: Error) => {
      setError(
        e instanceof ApiError && e.code === 'request.invalid'
          ? e.message
          : 'Could not save. Please try again.',
      );
    },
  });

  const remove = useMutation({
    mutationFn: (date: string) => deleteMeasurement(date),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['measurements'] }),
  });

  const numeric = (v: string) => v.trim() === '' || /^\d+(\.\d+)?$/.test(v.trim());
  const anyValue = [weight, bodyFat, waist].some((v) => v.trim() !== '');
  const ready = anyValue && numeric(weight) && numeric(bodyFat) && numeric(waist) && !save.isPending;

  if (measurements.isLoading) {
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
      keyboardShouldPersistTaps="handled"
    >
      <Stack.Screen options={{ title: 'Body' }} />

      {measurements.isError ? <ErrorBanner message="Could not load your measurements." /> : null}
      {error ? <ErrorBanner message={error} /> : null}

      <StatRow
        stats={[
          {
            label: 'Weight',
            value: latest?.weight_kg != null ? `${latest.weight_kg}` : '—',
            unit: latest?.weight_kg != null ? 'kg' : undefined,
            context: weightDelta ?? (latest ? shortDate(latest.measured_on) : undefined),
            delta: Boolean(weightDelta),
          },
          {
            label: 'BMI',
            value: currentBmi != null ? `${currentBmi}` : '—',
            context: heightCm != null ? 'weight ÷ height²' : 'set height in profile',
          },
          {
            label: 'Body fat',
            value: latest?.body_fat_percent != null ? `${latest.body_fat_percent}` : '—',
            unit: latest?.body_fat_percent != null ? '%' : undefined,
            context: latest?.body_fat_percent != null ? shortDate(latest.measured_on) : undefined,
          },
        ]}
      />

      {weightPoints.length >= 2 ? (
        <View>
          <Section label="Weight trend" meta={`${weightPoints.length} entries`} />
          <Card>
            <TrendChart
              height={96}
              values={weightPoints.map((p) => p.weight_kg ?? 0)}
              startLabel={`${shortDate(weightPoints[0]!.measured_on)} · ${weightPoints[0]!.weight_kg}`}
              endLabel={`${shortDate(weightPoints[weightPoints.length - 1]!.measured_on)} · ${weightPoints[weightPoints.length - 1]!.weight_kg}`}
              accessibilityLabel={`Weight trend over ${weightPoints.length} entries`}
            />
          </Card>
        </View>
      ) : null}

      <View>
        <Section label="Log today" meta={shortDate(localToday())} />
        <Card style={s.form}>
          {/* Steppers sized for thumbs; scales rarely disagree by more than 0.5. */}
          <NumberField
            label="Weight"
            value={weight}
            onChange={setWeight}
            unit="kg"
            step={0.5}
            min={20}
            max={500}
            placeholder={latest?.weight_kg != null ? `${latest.weight_kg}` : '81.4'}
          />
          <View style={s.formPair}>
            <View style={s.formCell}>
              <NumberField
                label="Body fat"
                value={bodyFat}
                onChange={setBodyFat}
                unit="%"
                step={0.5}
                min={1}
                max={75}
                placeholder="—"
              />
            </View>
            <View style={s.formCell}>
              <NumberField
                label="Waist"
                value={waist}
                onChange={setWaist}
                unit="cm"
                step={0.5}
                min={10}
                max={300}
                placeholder="—"
              />
            </View>
          </View>
          <Button
            label={save.isPending ? 'Saving…' : 'Save measurement'}
            disabled={!ready}
            onPress={() => save.mutate()}
          />
        </Card>
      </View>

      {rows.length === 0 && !measurements.isError ? (
        <EmptyState
          glyph="◔"
          title="No measurements yet"
          hint="A morning weigh-in is enough to start the trend."
        />
      ) : (
        <View>
          <Section label="History" meta={`${rows.length} entries`} />
          <Card padded={false}>
          {rows.map((m, i) => (
            <View key={m.measured_on} style={[s.row, i === rows.length - 1 && s.rowLast]}>
              <View style={s.rowBody}>
                <Text style={s.rowDate}>{shortDate(m.measured_on)}</Text>
                <Text style={s.rowDetail} numberOfLines={1}>
                  {[
                    m.weight_kg != null ? `${m.weight_kg} kg` : null,
                    m.body_fat_percent != null ? `${m.body_fat_percent}%` : null,
                    m.waist_cm != null ? `waist ${m.waist_cm} cm` : null,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </Text>
              </View>
              <Touchable
                onPress={() =>
                  Alert.alert('Delete this entry?', `${m.measured_on} will be removed.`, [
                    { text: 'Keep', style: 'cancel' },
                    {
                      text: 'Delete',
                      style: 'destructive',
                      onPress: () => remove.mutate(m.measured_on),
                    },
                  ])
                }
                accessibilityRole="button"
                accessibilityLabel={`Delete the entry for ${m.measured_on}`}
                style={s.delete}
              >
                <Feather name="trash-2" size={15} color={t.color.mut} />
              </Touchable>
            </View>
          ))}
          </Card>
        </View>
      )}
    </ScrollView>
  );
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
    form: { gap: t.space.lg, marginTop: t.space.sm },
    formPair: { flexDirection: 'row', gap: t.space.md },
    formCell: { flex: 1 },

    row: {
      alignItems: 'center',
      borderBottomColor: t.color.line,
      borderBottomWidth: StyleSheet.hairlineWidth,
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: 11,
    },
    rowLast: { borderBottomWidth: 0 },
    rowBody: { flex: 1, gap: 2 },
    rowDate: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.sm + 0.5 },
    rowDetail: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },
    delete: {
      alignItems: 'center',
      borderRadius: t.radius.pill,
      justifyContent: 'center',
      minHeight: 44,
      minWidth: 44,
    },
  });
