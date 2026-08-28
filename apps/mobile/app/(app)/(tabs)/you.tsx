import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Alert, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { getMyProfiles, listInvoices, listMeasurements, signOut } from '@/api/gym';
import { bmi } from '@/features/progress/metrics';
import { can, capacityLabel, useActiveMembership, useSession } from '@/session/store';
import { Appear, Badge, Button, Card, InitialsSquare, ListRow, Section } from '@/ui/components';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * The account screen.
 *
 * Restructured around a single identity card at the top — avatar, name, what
 * you are at this gym — because the old version opened with a bare name and
 * then a run of unlabelled rows, and "who am I signed in as" is the question
 * people actually come here to answer.
 */
export default function You() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const tabBarHeight = useTabBarHeight();

  const user = useSession((st) => st.user);
  const active = useActiveMembership();

  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const isManager = can.manageGym(active?.capacities ?? []);

  const profiles = useQuery({ queryKey: ['my-profiles'], queryFn: getMyProfiles });
  const measurements = useQuery({ queryKey: ['measurements'], queryFn: listMeasurements });
  // Managers already have the full Billing tab — this is the self-service
  // surface for everyone else, so only fetch it for them.
  const invoices = useQuery({
    queryKey: ['invoices', gymId],
    queryFn: () => listInvoices(gymId!),
    enabled: Boolean(gymId) && !isManager,
  });

  const headline = profiles.data?.trainer?.headline;
  const heightCm = profiles.data?.athlete?.height_cm ?? null;
  const dob = profiles.data?.athlete?.date_of_birth ?? null;
  const latest = (measurements.data ?? [])[0];
  const currentBmi =
    latest?.weight_kg != null && heightCm != null ? bmi(latest.weight_kg, heightCm) : null;

  const measured = measurements.data?.length ?? 0;
  const due = (invoices.data ?? []).filter((i) => i.status.state === 'due');
  const overdue = due.some((i) => i.is_overdue);

  return (
    <ScrollView
      style={s.screen}
      contentContainerStyle={[
        s.content,
        { paddingBottom: tabBarHeight + 24, paddingTop: insets.top + 12 },
      ]}
      showsVerticalScrollIndicator={false}
    >
      <Appear>
        <Card>
          <View style={s.identity}>
            <InitialsSquare name={user?.displayName ?? 'You'} size={54} />
            <View style={s.identityText}>
              <Text style={s.name} numberOfLines={2}>
                {user?.displayName ?? 'You'}
              </Text>
              <Text style={s.email} numberOfLines={1}>
                {user?.email ?? ''}
              </Text>
            </View>
          </View>

          {active ? (
            <View style={s.badges}>
              {active.capacities.map((c) => (
                <Badge key={c} tone="accent">
                  {capacityLabel(c)}
                </Badge>
              ))}
              <Text style={s.at} numberOfLines={1}>
                at {active.gymName}
              </Text>
            </View>
          ) : null}

          {headline ? <Text style={s.headline}>{headline}</Text> : null}

          {/*
            Height and birth year are the two facts every other screen derives
            from — BMI needs one, age-banded guidance the other — so they sit
            here as data rather than being hidden behind the edit form.
          */}
          <View style={s.facts}>
            <Fact label="Height" value={heightCm != null ? `${heightCm} cm` : 'Not set'} />
            <Fact label="Born" value={dob ? dob.slice(0, 4) : 'Not set'} />
            <Fact
              label="BMI"
              value={currentBmi != null ? String(currentBmi) : '—'}
            />
          </View>

          <Button
            label="Edit profile"
            variant="secondary"
            onPress={() => router.push('/(app)/edit-profile')}
          />
        </Card>
      </Appear>

      <View style={s.group}>
        <Section label="Body" meta={measured > 0 ? `${measured} logged` : undefined} />
        <Card padded={false}>
          <ListRow
            title={
              latest?.weight_kg != null ? `${latest.weight_kg} kg` : 'Log your first measurement'
            }
            subtitle={
              measured > 0
                ? `Last measured ${latest?.measured_on ?? ''} · weight, BMI, body fat, girths`
                : 'Weight, BMI, body fat and girths, tracked over time'
            }
            last
            onPress={() => router.push('/(app)/body')}
          />
        </Card>
      </View>

      {!isManager ? (
        <View style={s.group}>
          <Section label="Membership" />
          <Card padded={false}>
            <ListRow
              title={due.length > 0 ? `${due.length} invoice${due.length === 1 ? '' : 's'} due` : 'All settled'}
              subtitle="Your plan, invoices, and paying by card"
              subtitleTone={overdue ? 'reason' : 'muted'}
              right={
                overdue ? (
                  <Badge tone="danger">Overdue</Badge>
                ) : due.length > 0 ? (
                  <Badge tone="warn">Due</Badge>
                ) : (
                  <Badge tone="success">Paid</Badge>
                )
              }
              last
              onPress={() => router.push('/(app)/membership')}
            />
          </Card>
        </View>
      ) : null}

      <View style={s.group}>
        <Section label="Account" />
        <Card padded={false}>
          <ListRow
            title="Change password"
            subtitle="Especially if somebody else set this account up for you"
            onPress={() => router.push('/(app)/change-password')}
          />
          <ListRow
            title="Sign out"
            subtitle="Your training history stays on the account"
            last
            right={null}
            onPress={() =>
              Alert.alert('Sign out?', 'You will need your password to get back in.', [
                { text: 'Stay signed in', style: 'cancel' },
                { text: 'Sign out', style: 'destructive', onPress: () => void signOut() },
              ])
            }
          />
        </Card>
      </View>
    </ScrollView>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  const s = useStyles(styleFactory);
  return (
    <View style={s.fact}>
      <Text style={s.factLabel}>{label}</Text>
      <Text style={s.factValue} numberOfLines={1}>
        {value}
      </Text>
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    content: { gap: t.space.xl, paddingHorizontal: t.space.gutter },
    group: { gap: 0 },

    identity: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    identityText: { flex: 1, gap: 2 },
    name: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 30,
    },
    email: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.sm },
    badges: { alignItems: 'center', flexDirection: 'row', flexWrap: 'wrap', gap: t.space.sm },
    at: { color: t.color.mut, fontFamily: fonts.medium, fontSize: t.font.xs },
    headline: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 19,
    },

    facts: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      paddingHorizontal: t.space.sm,
      paddingVertical: t.space.md,
    },
    fact: { alignItems: 'center', flex: 1, gap: 2 },
    factLabel: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    factValue: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.md,
      fontVariant: ['tabular-nums'],
    },
  });
