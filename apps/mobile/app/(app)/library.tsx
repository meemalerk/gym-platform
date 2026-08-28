import { Stack } from 'expo-router';

import { LibraryScreen } from '@/ui/library-screen';

/**
 * The catalogue, pushed rather than tabbed.
 *
 * This route exists for managers. The Library *tab* is unmounted for anyone
 * who can manage the gym (`Tabs.Protected` — hidden and unreachable mean the
 * same thing), so Manage's "Exercise library" row used to push a route that
 * could not match: expo-router fell through to its built-in unmatched screen,
 * which is what put the stray "(tabs)" button on screen and bounced people
 * back to the dashboard.
 */
export default function LibraryPushed() {
  return (
    <>
      <Stack.Screen options={{ title: 'Library', headerShown: false }} />
      <LibraryScreen inTabs={false} />
    </>
  );
}
