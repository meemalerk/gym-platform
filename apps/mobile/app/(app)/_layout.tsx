import { Stack } from 'expo-router';

import { useTokens } from '@/ui/theme-context';

/**
 * Authenticated area: a tab shell, plus the screens that sit *over* it.
 *
 * No redirect guard here on purpose — access is enforced declaratively by
 * `<Stack.Protected guard={signedIn}>` in the root layout. Duplicating the check
 * would be the older pattern and risks the two disagreeing.
 *
 * The modals stay in the stack rather than becoming tabs: each is a short task
 * you finish and dismiss, and tabs are for places you dwell. Presenting them as
 * modals also means they cover the tab bar, which is the honest signal that you
 * are in the middle of something.
 */
export default function AppLayout() {
  const t = useTokens();
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: t.color.surface },
        // The chevron takes the accent while the title stays ink: back is the
        // control on the bar, and it should read as one at a glance.
        headerTintColor: t.color.accent,
        // Chevron only — no back LABEL.
        //
        // iOS puts the previous screen's title beside the arrow, and for a
        // route GROUP that title is the folder name: people were seeing
        // "(tabs)" and "(program)" on the back button, which is a filesystem
        // detail leaking into the interface.
        //
        // `headerBackButtonDisplayMode: 'minimal'` is the react-navigation v7
        // spelling. `headerBackTitleVisible` was the v6 one and no longer
        // exists in the types here, so setting it would silently do nothing.
        headerBackButtonDisplayMode: 'minimal',
        headerTitleStyle: { color: t.color.ink, fontFamily: t.fonts.display },
        headerShadowVisible: false,
        contentStyle: { backgroundColor: t.color.surface },
      }}
    >
      <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      <Stack.Screen
        name="new-exercise"
        options={{ title: 'New exercise', presentation: 'modal' }}
      />
      {/* Standing replaced invitations (ADR-0031): a short task you finish and
          dismiss, which is exactly what a modal is for. */}
      <Stack.Screen name="standing" options={{ title: 'Standing', presentation: 'modal' }} />
      {/* Creating an account is the other half of standing: one makes staff
          out of a member, the other makes both at once (ADR-0032). */}
      <Stack.Screen
        name="new-staff"
        options={{ title: 'New staff account', presentation: 'modal' }}
      />
      <Stack.Screen
        name="change-password"
        options={{ title: 'Change password', presentation: 'modal' }}
      />
      <Stack.Screen
        name="join-plan"
        options={{ title: 'Join a plan', presentation: 'modal' }}
      />
      <Stack.Screen
        name="assign-coach"
        options={{ title: 'Pair a coach', presentation: 'modal' }}
      />
      {/* Two directions on the same act: pick a version and choose who, or
          open a client and choose what. Coaches work the second way. */}
      <Stack.Screen
        name="assign-program-for"
        options={{ title: 'Assign a programme', presentation: 'modal' }}
      />
      <Stack.Screen
        name="edit-profile"
        options={{ title: 'Your profile', presentation: 'modal' }}
      />
      <Stack.Screen name="entry-pass" options={{ title: 'Entry pass', presentation: 'modal' }} />
      {/* Full-bleed camera view — a header would just eat into the frame. */}
      <Stack.Screen
        name="scan-entry"
        options={{ title: 'Scan entry', presentation: 'modal', headerShown: false }}
      />
      {/* Pushed, not modal: a plan is a place you read, not a task you dismiss. */}
      <Stack.Screen name="program/[version]" options={{ title: 'Programme' }} />
      <Stack.Screen name="session/[id]" options={{ title: 'Workout' }} />
      {/* Pushed rather than modal, and it `replace`s itself on start — this is
          a step on the way into a workout, not a dialog to dismiss. */}
      <Stack.Screen name="new-session" options={{ title: 'Your own workout' }} />
      <Stack.Screen name="exercise/[id]" options={{ title: 'Progress' }} />
      <Stack.Screen name="athlete/[id]" options={{ title: 'Client' }} />
      <Stack.Screen name="body" options={{ title: 'Body' }} />
      <Stack.Screen name="membership" options={{ title: 'Membership' }} />
      <Stack.Screen name="find-coach" options={{ title: 'Choose a coach' }} />
      <Stack.Screen name="training-style" options={{ title: '' }} />
      <Stack.Screen name="manage" options={{ title: 'Manage' }} />
      {/* The catalogue, pushed. Managers have no Library tab — see the note in
          library.tsx for what that used to do to them. */}
      <Stack.Screen name="library" options={{ title: 'Library', headerShown: false }} />
      {/* Same shape as `library`, and it was missing: programs.tsx draws its own
          GymHeader, so an undeclared native header sat above it showing the raw
          route name — "programs" over "Programmes", with a back button labelled
          after whatever pushed it. */}
      <Stack.Screen name="programs" options={{ title: 'Programmes', headerShown: false }} />
    </Stack>
  );
}
