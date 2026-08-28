/**
 * TanStack Query configured for React Native.
 *
 * On the web, Query detects reconnects and window focus for free. On native it
 * cannot — you must wire `onlineManager` and `focusManager` yourself, or queries
 * will not refetch when the device regains connectivity or the app returns to
 * the foreground. This is the setup TanStack's React Native guide prescribes.
 */

import { QueryClient, focusManager, onlineManager } from '@tanstack/react-query';
import * as Network from 'expo-network';
import { AppState, Platform, type AppStateStatus } from 'react-native';

import { ApiError } from '@/api/client';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (failureCount, error) => {
        // Never retry client errors. A 401/403/404 will not fix itself, and
        // retrying a 401 fights the single-flight refresh in the API client.
        if (error instanceof ApiError && error.status < 500) return false;
        return failureCount < 2;
      },
    },
  },
});

/**
 * Wire native connectivity + foreground detection.
 * Returns a cleanup function; call once from the root layout.
 */
export function startQueryPlatformListeners(): () => void {
  // `setEventListener` returns void; the cleanup is the *setup* function's return
  // value, which onlineManager invokes itself when the listener is replaced.
  onlineManager.setEventListener((setOnline) => {
    let initialised = false;

    const subscription = Network.addNetworkStateListener((state) => {
      initialised = true;
      setOnline(Boolean(state.isConnected));
    });

    // The listener only fires on *change*, so seed the current state — otherwise
    // an app launched offline is treated as online until connectivity flips.
    Network.getNetworkStateAsync()
      .then((state) => {
        if (!initialised) setOnline(Boolean(state.isConnected));
      })
      .catch(() => {
        // Unknown connectivity: leave Query's default (online) rather than
        // wrongly pausing every request.
      });

    return () => subscription.remove();
  });

  const onAppStateChange = (status: AppStateStatus) => {
    // Web has native focus handling; overriding it there causes double refetches.
    if (Platform.OS !== 'web') focusManager.setFocused(status === 'active');
  };

  const appStateSubscription = AppState.addEventListener('change', onAppStateChange);

  return () => appStateSubscription.remove();
}
