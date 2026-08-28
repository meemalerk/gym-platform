/**
 * Session state.
 *
 * Storage split is deliberate:
 *  - **refresh token** → `expo-secure-store` (Keychain / Android Keystore). It is
 *    long-lived and must survive app restarts, so it needs OS-level protection.
 *  - **access token** → memory only. It is short-lived (15 min); persisting it
 *    would widen the attack surface for no benefit.
 *
 * Never put either in AsyncStorage — that is unencrypted.
 */

import { create } from 'zustand';

import { secureStorage } from '@/session/secure-storage';

const REFRESH_TOKEN_KEY = 'gym.refresh_token';

export type SessionUser = {
  id: string;
  email: string;
  displayName: string;
};

/**
 * Standing in the one gym this deployment serves: every capacity held there,
 * not a single role (ADR-0014). At most one — this is a single-gym deployment
 * (ADR-0023), so there is no gym to switch between any more.
 */
export type Membership = {
  gymId: string;
  gymName: string;
  isPersonal: boolean;
  capacities: string[];
};

/*
 * Capability rules live in `@/session/capabilities` — pure, React-free and
 * testable on their own. Re-exported here so existing screens keep importing
 * from one place.
 */
export {
  can,
  capacityLabel,
  mayPrescribeFor,
  type Capacity,
} from '@/session/capabilities';

export type SessionStatus = 'restoring' | 'signedOut' | 'signedIn';

type SessionState = {
  status: SessionStatus;
  accessToken: string | null;
  user: SessionUser | null;
  /** Null before the account has joined the gym. */
  membership: Membership | null;
  /**
   * The account id that is mid-registration and still owes a plan choice, or
   * null. Read from storage on restore; see `@/session/onboarding` for why this
   * is a marker set by the sign-up journey rather than something derived from
   * whether the member currently holds `gym_access`.
   */
  planPendingFor: string | null;

  setSignedIn: (args: {
    accessToken: string;
    user: SessionUser;
    membership: Membership | null;
  }) => void;
  setAccessToken: (token: string) => void;
  /** Update the signed-in user in place — e.g. after a rename. */
  setUser: (user: SessionUser) => void;
  setMembership: (membership: Membership | null) => void;
  setPlanPendingFor: (userId: string | null) => void;
  setSignedOut: () => void;
};

export const useSession = create<SessionState>((set) => ({
  status: 'restoring',
  accessToken: null,
  user: null,
  membership: null,
  planPendingFor: null,

  setSignedIn: ({ accessToken, user, membership }) =>
    set({ status: 'signedIn', accessToken, user, membership }),

  setAccessToken: (accessToken) => set({ accessToken }),

  setUser: (user) => set({ user }),

  setMembership: (membership) => set({ membership }),

  setPlanPendingFor: (planPendingFor) => set({ planPendingFor }),

  setSignedOut: () =>
    set({
      status: 'signedOut',
      accessToken: null,
      user: null,
      membership: null,
      // Cleared in memory but NOT in storage: somebody who signs out halfway
      // through registering and signs back in should land where they left off,
      // and the marker is keyed by user id so it cannot apply to anyone else.
      planPendingFor: null,
    }),
}));

/**
 * The membership for the one gym, if the account has joined it.
 *
 * Kept as its own hook (rather than inlining `useSession((s) => s.membership)`
 * everywhere) so call sites read the same either way this ends up modelled —
 * that indirection is what made the ADR-0023 migration a one-file change here.
 */
export const useActiveMembership = (): Membership | null =>
  useSession((s) => s.membership);

// ---------------------------------------------------------------- persistence

export async function saveRefreshToken(token: string): Promise<void> {
  await secureStorage.set(REFRESH_TOKEN_KEY, token);
}

export async function readRefreshToken(): Promise<string | null> {
  try {
    return await secureStorage.get(REFRESH_TOKEN_KEY);
  } catch {
    // A corrupt or inaccessible keychain entry must not brick the app —
    // treat it as "not signed in".
    return null;
  }
}

export async function clearRefreshToken(): Promise<void> {
  try {
    await secureStorage.remove(REFRESH_TOKEN_KEY);
  } catch {
    // Nothing useful to do; the token is unusable either way.
  }
}
