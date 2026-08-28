import {
  BricolageGrotesque_600SemiBold,
  BricolageGrotesque_700Bold,
  BricolageGrotesque_800ExtraBold,
} from '@expo-google-fonts/bricolage-grotesque';
import {
  PlusJakartaSans_400Regular,
  PlusJakartaSans_500Medium,
  PlusJakartaSans_600SemiBold,
  PlusJakartaSans_700Bold,
  PlusJakartaSans_800ExtraBold,
  useFonts,
} from '@expo-google-fonts/plus-jakarta-sans';
import { QueryClientProvider } from '@tanstack/react-query';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { restoreSession } from '@/api/gym';

import { queryClient, startQueryPlatformListeners } from '@/query';
import { mountedGroup, needsPlanChoice } from '@/session/routing';
import { useSession } from '@/session/store';
import { Centered, Muted } from '@/ui/components';
import { ThemeProvider, useTokens } from '@/ui/theme-context';

export default function RootLayout() {
  const status = useSession((s) => s.status);
  const hasGym = useSession((s) => s.membership !== null);
  const userId = useSession((s) => s.user?.id ?? null);
  const planPendingFor = useSession((s) => s.planPendingFor);
  // Mid-registration, for THIS account — never a billing state.
  const needsPlan = needsPlanChoice({ planPendingFor, userId });
  const [restoreFailed, setRestoreFailed] = useState(false);
  // Two families carry the whole language (ADR-0030); rendering system-font
  // fallbacks for a flash and then swapping reads as a glitch, so hold on the
  // loading state until both are in.
  const [fontsReady] = useFonts({
    PlusJakartaSans_400Regular,
    PlusJakartaSans_500Medium,
    PlusJakartaSans_600SemiBold,
    PlusJakartaSans_700Bold,
    PlusJakartaSans_800ExtraBold,
    BricolageGrotesque_600SemiBold,
    BricolageGrotesque_700Bold,
    BricolageGrotesque_800ExtraBold,
  });

  useEffect(() => {
    restoreSession().catch(() => setRestoreFailed(true));
  }, []);

  // Native needs explicit connectivity/foreground wiring for TanStack Query.
  useEffect(() => startQueryPlatformListeners(), []);

  const signedIn = status === 'signedIn';

  return (
    // ThemeProvider is outermost so *everything* below can read tokens through
    // the hook — including the root background. It used to sit inside the
    // gesture root, which forced that one style to import the palette
    // statically and was the last thing keeping the compatibility shim alive.
    <ThemeProvider>
      <RootShell
        status={status}
        signedIn={signedIn}
        hasGym={hasGym}
        needsPlan={needsPlan}
        fontsReady={fontsReady}
        restoreFailed={restoreFailed}
      />
    </ThemeProvider>
  );
}

/**
 * The router, and the one thing that decides which of it you get.
 *
 * Split out of `RootShell` deliberately, and kept split. The routing decision
 * once needed a server query (it asked whether the member held `gym_access`),
 * and a hook consuming the query client cannot live in the component that
 * RENDERS `QueryClientProvider` — it would sit outside its own context and
 * crash with "No QueryClient set". The decision no longer needs a query, but
 * the boundary stays: anything added here later is on the right side of the
 * provider by construction rather than by memory.
 */
function Routes({
  status,
  signedIn,
  hasGym,
  needsPlan,
  fontsReady,
  restoreFailed,
}: {
  status: string;
  signedIn: boolean;
  hasGym: boolean;
  needsPlan: boolean;
  fontsReady: boolean;
  restoreFailed: boolean;
}) {
  const t = useTokens();
  // The SAME function `app/index.tsx` picks its landing route from, so the
  // group it sends you to and the group mounted here cannot disagree — which
  // they did, and the router answered "Do you have a route named '(app)'?".
  const group = mountedGroup({ signedIn, hasGym, needsPlan, settling: false });

  return status === 'restoring' || !fontsReady ? (
      <Centered>
        <Muted>{restoreFailed ? 'Could not reach the server' : 'Loading…'}</Muted>
      </Centered>
    ) : (
      /*
        FOUR states, one declarative guard each — no imperative redirects,
        and a failed guard drops its history so nothing can be reached by
        going "back". (It said "three" while listing four, which is the same
        arithmetic slip `app/index.tsx` was making in code.)

          signed out                    → sign in / sign up
          signed in, no gym             → onboarding: join
          signed in, gym, no membership → onboarding: choose a plan
          signed in, gym, membership    → the app

        Client-side routing only: it decides what to SHOW. The server
        re-checks capacities on every request and remains the authority.
      */
      <Stack
        screenOptions={{
          headerStyle: { backgroundColor: t.color.surface },
          headerTintColor: t.color.accent,
          // Only font props are allowed here; letterSpacing is rejected.
          headerTitleStyle: { color: t.color.ink, fontFamily: t.fonts.display },
          headerShadowVisible: false,
          // Chevron only, matching the authenticated stack — otherwise
          // iOS labels the back button with the previous ROUTE name.
          headerBackButtonDisplayMode: 'minimal',
          contentStyle: { backgroundColor: t.color.surface },
          animation: 'slide_from_right',
        }}
      >
        <Stack.Screen name="index" options={{ headerShown: false }} />

        <Stack.Protected guard={group === 'auth'}>
          <Stack.Screen name="sign-in" options={{ headerShown: false }} />
          <Stack.Screen name="sign-up" options={{ title: '', headerTransparent: true }} />
          <Stack.Screen
            name="forgot-password"
            options={{ title: '', headerTransparent: true }}
          />
        </Stack.Protected>

        {/*
          Onboarding covers TWO states now: no gym, and a gym but no
          membership. Both are "you are not ready for the app yet", and
          keeping them in one group means the plan step is a real gate —
          force-quitting on it and reopening lands back on it, because the
          guard is recomputed from the server rather than remembered.
        */}
        <Stack.Protected guard={group === '(onboarding)'}>
          <Stack.Screen name="(onboarding)" options={{ headerShown: false }} />
        </Stack.Protected>

        <Stack.Protected guard={group === '(app)'}>
          <Stack.Screen name="(app)" options={{ headerShown: false }} />
        </Stack.Protected>
      </Stack>
    );
}

function RootShell({
  status,
  signedIn,
  hasGym,
  needsPlan,
  fontsReady,
  restoreFailed,
}: {
  status: string;
  signedIn: boolean;
  hasGym: boolean;
  needsPlan: boolean;
  fontsReady: boolean;
  restoreFailed: boolean;
}) {
  const t = useTokens();
  return (
    // GestureHandlerRootView must wrap the app for native stack gestures;
    // SafeAreaProvider supplies the insets every screen pads with.
    <GestureHandlerRootView style={{ flex: 1, backgroundColor: t.color.surface }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          {/* The bar's contrast follows the scheme, not a hard-coded guess. */}
          <StatusBar style={t.scheme === 'light' ? 'dark' : 'light'} />

          {/*
            Everything that needs the query client lives BELOW the provider.

            The membership gate asks the server whether this account still owes
            the gym a plan, so it calls useQuery — and calling that here, in the
            component that RENDERS the provider, put it outside its own context
            and crashed with "No QueryClient set". Hence a child component.
          */}
          <Routes
            status={status}
            signedIn={signedIn}
            hasGym={hasGym}
            needsPlan={needsPlan}
            fontsReady={fontsReady}
            restoreFailed={restoreFailed}
          />
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
