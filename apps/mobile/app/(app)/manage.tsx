import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Stack, useRouter } from 'expo-router';
import { useState } from 'react';
import { ActivityIndicator, StyleSheet, Switch, Text, View } from 'react-native';

import { getGymSettings, setOpenRegistration } from '@/api/gym';
import { can, useActiveMembership, useSession } from '@/session/store';
import {
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  ListRow,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * The gym itself: settings, and the routes managers lost to the tab ceiling.
 *
 * Five tabs is a hard limit and Billing takes the slot Library would have had
 * for owners and admins (see `navigation/tabs.ts`), so the catalogue and the
 * programme builder need a home that is one tap from Today rather than
 * nowhere. Putting them here alongside the settings keeps "running the gym" in
 * one place instead of scattered through a tab bar that cannot hold them.
 */
export default function Manage() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];

  const canManageGym = can.manageGym(capacities);
  const canCurate = can.curateCatalogue(capacities);
  const [error, setError] = useState<string | null>(null);

  const settings = useQuery({
    queryKey: ['gym-settings', gymId],
    queryFn: () => getGymSettings(gymId!),
    // Owners and admins only, matching the server — a head coach asking would
    // get a 403 and paint an error over a screen that is otherwise useful.
    enabled: Boolean(gymId) && canManageGym,
  });

  const toggle = useMutation({
    mutationFn: (open: boolean) => setOpenRegistration(gymId!, open),
    onSuccess: (updated) => {
      setError(null);
      queryClient.setQueryData(['gym-settings', gymId], updated);
    },
    onError: () => setError('Could not change that setting. Please try again.'),
  });

  if (!canManageGym && !canCurate) {
    return (
      <Screen scroll edges={['bottom']}>
        <Stack.Screen options={{ title: 'Manage' }} />
        <EmptyState
          glyph="◍"
          title="Nothing to manage"
          hint="This is where owners and head coaches run the gym."
        />
      </Screen>
    );
  }

  const open = settings.data?.open_registration ?? false;

  return (
    <Screen scroll edges={['bottom']}>
      <Stack.Screen options={{ title: 'Manage' }} />

      {error ? <ErrorBanner message={error} /> : null}

      {canManageGym ? (
        <View>
          <Section label="Joining" />
          {settings.isLoading ? (
            <Centered>
              <ActivityIndicator color={t.color.accent} />
            </Centered>
          ) : (
            <Card>
              <View style={s.setting}>
              <View style={s.settingBody}>
                <Text style={s.settingTitle}>Anyone can join</Text>
                <Text style={s.settingBody2}>
                  {open
                    ? 'People can find this gym in the app and join as members without a code. Staff still need an invitation.'
                    : 'An invitation is the only way in. Turn this on to let people join as members on their own.'}
                </Text>
              </View>
              <Switch
                value={open}
                disabled={toggle.isPending}
                onValueChange={(next) => {
                  setError(null);
                  toggle.mutate(next);
                }}
                accessibilityLabel="Let anyone join this gym as a member"
                trackColor={{ false: t.color.track, true: t.color.accent }}
                thumbColor={t.color.surface2}
              />
              </View>
              {/* Said once, plainly, next to the switch rather than in a help
                  page nobody opens: the open door cannot hand out staff
                  standing, whatever it is set to. */}
              <Text style={s.footnote}>
                Turning this on never grants trainer, admin or owner standing — only
                membership. Staff are made afterwards: open People, tap somebody, and set
                what they hold.
              </Text>
            </Card>
          )}
        </View>
      ) : null}

      <View>
        <Section label="The gym" />
        <Card padded={false}>
        {/*
          `/(app)/library`, not `/(app)/(tabs)/library`. The tab is unmounted
          for anyone who can manage the gym, so the tab path matched nothing
          and dropped them back on the dashboard through expo-router's
          unmatched screen — which is where the stray "(tabs)" button came
          from. The pushed route renders the same catalogue.
        */}
        <ListRow
          title="Exercise library"
          subtitle="The movements this gym programmes"
          onPress={() => router.push('/(app)/library')}
        />
        <ListRow
          title="Programmes"
          subtitle="Write, review and publish"
          onPress={() => router.push('/(app)/programs')}
        />
        {can.setCapacities(capacities) ? (
          <ListRow
            title="People and standing"
            subtitle="Who is here, and who is staff"
            onPress={() => router.push('/(app)/(tabs)/people')}
          />
        ) : null}
        {can.setCapacities(capacities) ? (
          <ListRow
            title="Add a staff account"
            subtitle="Create the account and their standing together"
            onPress={() => router.push('/(app)/new-staff')}
          />
        ) : null}
        <ListRow
          title="Pair a coach with an athlete"
          subtitle="Opens that athlete's training to them"
          last
          onPress={() => router.push('/(app)/assign-coach')}
        />
        </Card>
      </View>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    setting: { alignItems: 'center', flexDirection: 'row', gap: t.space.lg },
    settingBody: { flex: 1, gap: 3 },
    settingTitle: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    settingBody2: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
    },
    footnote: {
      borderTopColor: t.color.line,
      borderTopWidth: StyleSheet.hairlineWidth,
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
      paddingTop: t.space.sm,
    },
  });
