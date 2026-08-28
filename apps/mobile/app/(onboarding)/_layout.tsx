import { Stack } from 'expo-router';

import { useTokens } from '@/ui/theme-context';

/**
 * Shown once, to an account that belongs to no gym yet.
 *
 * The root layout routes here based on `membership === null`, so this area
 * disappears by itself the moment the person joins.
 *
 * One screen, because there is one door. ADR-0026 shipped two — open
 * registration and an invite code — and ADR-0031 removed the code half, so
 * joining is "the gym opened its doors" or it is nothing. Bootstrapping the
 * gym itself is still an ops action and is not offered here (ADR-0023).
 */
export default function OnboardingLayout() {
  const t = useTokens();
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: t.color.surface },
        headerTintColor: t.color.ink,
        contentStyle: { backgroundColor: t.color.surface },
        headerShadowVisible: false,
      }}
    >
      <Stack.Screen name="start" options={{ headerShown: false }} />
      {/* No header, and no back: choosing a membership is the last gate before
          the app, and there is nothing useful behind it. */}
      <Stack.Screen name="choose-plan" options={{ headerShown: false }} />
    </Stack>
  );
}
