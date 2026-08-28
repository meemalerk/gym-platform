import { Tabs } from 'expo-router';

import { TAB_MANIFEST } from '@/navigation/tabs';
import { useActiveMembership } from '@/session/store';
import { GymTabBar } from '@/ui/tab-bar';
import { useTokens } from '@/ui/theme-context';

/**
 * The tab shell.
 *
 * Every tab comes from `TAB_MANIFEST` — there is no hand-written list here, so a
 * tab cannot exist in the bar without a visibility rule, and the rules stay
 * testable on their own (`scripts/verify-nav.mjs`).
 *
 * `Tabs.Protected` rather than `href: null`: `href: null` only hides the button,
 * leaving the route mounted and reachable by deep link, whereas a failed guard
 * unmounts it outright. Since the guard is a capacity check, "hidden" and
 * "unreachable" should mean the same thing. It also matches the `Stack.Protected`
 * boundary in the root layout, so there is one pattern to learn rather than two.
 *
 * Capacities come from the signed-in account's membership, so a different
 * account signing in re-shapes the bar. The server re-checks everything
 * regardless; this is what the user may see, not what they may do.
 */
export default function TabsLayout() {
  const t = useTokens();
  const active = useActiveMembership();
  const capacities = active?.capacities ?? [];

  return (
    <Tabs
      tabBar={(props) => <GymTabBar {...props} />}
      screenOptions={{
        headerShown: false,
        sceneStyle: { backgroundColor: t.color.surface },
      }}
    >
      {TAB_MANIFEST.map((tab) => (
        <Tabs.Protected key={tab.name} guard={tab.visible(capacities)}>
          <Tabs.Screen name={tab.name} options={{ title: tab.label }} />
        </Tabs.Protected>
      ))}
    </Tabs>
  );
}
