/**
 * The console's API client.
 *
 * Deliberately NOT shared with the mobile app. They have genuinely different
 * needs — the phone stores its refresh token in the OS keychain and
 * de-duplicates concurrent refreshes across a backgrounded app; a browser tab
 * has neither problem and cannot use expo-secure-store anyway. A shared client
 * would be a pile of `if (platform)`.
 *
 * What IS shared is the contract: `schema.d.ts` is generated from the same
 * OpenAPI document as the app's, so backend drift breaks this build too.
 */

import type { paths, components } from './schema';

export type Schemas = components['schemas'];

/**
 * Same-origin. The dev server proxies `/api`, and the deployed console is
 * served from the same host as the API (nginx, as in the demo). An absolute
 * URL here would be a promise about a machine we have not met — the same
 * reasoning that governs the mobile app's `EXPO_PUBLIC_API_URL`.
 */
const BASE = '';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

type Tokens = { access: string; refresh: string };

const STORAGE_KEY = 'gym.console.session';

/**
 * The refresh token lives in `localStorage`, and that is a real trade-off
 * worth naming rather than hiding: it is readable by any script that gets
 * onto this origin, so an XSS here is an account takeover.
 *
 * The alternative — an HttpOnly cookie set by the API — is genuinely better
 * and is what `docs/research-2026.md` §5 recommends for a browser client. It
 * needs a server-side session endpoint and CSRF handling that do not exist
 * yet, and building half of it would be worse than neither. This is the known
 * gap, written down, not an oversight.
 */
function load(): Tokens | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Tokens) : null;
  } catch {
    // Private mode, blocked storage, corrupt value. Signed out is the correct
    // reading of "I cannot tell", and it must not throw on boot.
    return null;
  }
}

function store(tokens: Tokens | null) {
  try {
    if (tokens) localStorage.setItem(STORAGE_KEY, JSON.stringify(tokens));
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // A session that survives only in memory is still a usable session.
  }
}

let tokens: Tokens | null = load();
let onSignedOut: (() => void) | null = null;

export function setSignedOutHandler(handler: () => void) {
  onSignedOut = handler;
}

export const isSignedIn = () => tokens !== null;

/**
 * One in-flight refresh, shared by every caller.
 *
 * The mobile client learned this the hard way (`refreshOnce`): two
 * simultaneous 401s rotating the token twice means the second rotation
 * presents an already-rotated token, which the server correctly treats as
 * theft and revokes the whole family. A dashboard fires five queries at once
 * on load, so this is not a theoretical race here — it is the common case.
 */
let refreshing: Promise<boolean> | null = null;

async function refreshOnce(): Promise<boolean> {
  refreshing ??= (async () => {
    const current = tokens?.refresh;
    if (!current) return false;
    try {
      const response = await fetch(`${BASE}/api/v1/auth/refresh`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ refresh_token: current }),
      });
      if (!response.ok) return false;
      const body = (await response.json()) as Schemas['TokensResponse'];
      tokens = { access: body.access_token, refresh: body.refresh_token };
      store(tokens);
      return true;
    } catch {
      return false;
    } finally {
      // Cleared inside the same async function so the next caller starts a
      // fresh attempt rather than awaiting a settled promise forever.
      refreshing = null;
    }
  })();

  return refreshing;
}

type RequestOptions = {
  method?: string;
  body?: unknown;
  /** Internal: stops a refreshed request from recursing. */
  retried?: boolean;
};

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> = {};
  if (tokens) headers.authorization = `Bearer ${tokens.access}`;
  if (options.body !== undefined) headers['content-type'] = 'application/json';

  const response = await fetch(`${BASE}${path}`, {
    method: options.method ?? 'GET',
    headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });

  if (response.status === 401 && tokens && !options.retried) {
    if (await refreshOnce()) {
      return request<T>(path, { ...options, retried: true });
    }
    signOut();
    throw new ApiError(401, 'auth.unauthenticated', 'Signed out');
  }

  if (response.status === 204) return undefined as T;

  const text = await response.text();
  const payload: unknown = text ? JSON.parse(text) : null;

  if (!response.ok) {
    const problem = payload as { code?: string; detail?: string; title?: string } | null;
    throw new ApiError(
      response.status,
      problem?.code ?? 'internal.error',
      problem?.detail ?? problem?.title ?? 'Something went wrong',
    );
  }

  return payload as T;
}

export async function signIn(email: string, password: string): Promise<void> {
  const response = await fetch(`${BASE}/api/v1/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email, password, device_label: 'Console' }),
  });

  const text = await response.text();
  const payload: unknown = text ? JSON.parse(text) : null;

  if (!response.ok) {
    const problem = payload as { code?: string; detail?: string } | null;
    throw new ApiError(
      response.status,
      problem?.code ?? 'auth.unauthenticated',
      problem?.detail ?? 'Could not sign in',
    );
  }

  const body = payload as Schemas['TokensResponse'];
  tokens = { access: body.access_token, refresh: body.refresh_token };
  store(tokens);
}

export function signOut() {
  tokens = null;
  store(null);
  onSignedOut?.();
}

// ------------------------------------------------------------------ reads

export type Me = Schemas['MeResponse'];
export type GymMember = Schemas['GymMemberResponse'];
export type Invoice = Schemas['InvoiceResponse'];
export type Subscription = Schemas['SubscriptionResponse'];
export type Plan = Schemas['PlanResponse'];
export type Exercise = Schemas['ExerciseResponse'];
export type CoachRelationship = Schemas['CoachRelationshipResponse'];
export type CoachingRequest = Schemas['CoachingRequestResponse'];
export type WorkoutSession = Schemas['SessionResponse'];
export type AuditEntry = Schemas['AuditEntryResponse'];
export type GymSettings = Schemas['GymSettingsResponse'];
export type CreatedStaff = Schemas['CreatedStaffResponse'];

export const api = {
  me: () => request<Me>('/api/v1/me'),

  members: (gym: string) => request<GymMember[]>(`/api/v1/gyms/${gym}/members`),
  relationships: (gym: string) =>
    request<CoachRelationship[]>(`/api/v1/gyms/${gym}/coach-relationships`),
  coachingRequests: (gym: string) =>
    request<CoachingRequest[]>(`/api/v1/gyms/${gym}/coaching-requests`),

  plans: (gym: string) => request<Plan[]>(`/api/v1/gyms/${gym}/plans`),
  subscriptions: (gym: string) => request<Subscription[]>(`/api/v1/gyms/${gym}/subscriptions`),
  invoices: (gym: string) => request<Invoice[]>(`/api/v1/gyms/${gym}/invoices`),

  exercises: (gym: string) => request<Exercise[]>(`/api/v1/gyms/${gym}/exercises`),
  pendingExercises: (gym: string) => request<Exercise[]>(`/api/v1/gyms/${gym}/exercises/pending`),
  curate: (gym: string, id: string, decision: 'approve' | 'retire') =>
    request<Exercise>(`/api/v1/gyms/${gym}/exercises/${id}/curate`, {
      method: 'POST',
      body: { decision },
    }),

  sessions: (gym: string, params?: { athlete_id?: string; from?: string; limit?: number }) => {
    const query = new URLSearchParams();
    if (params?.athlete_id) query.set('athlete_id', params.athlete_id);
    if (params?.from) query.set('from', params.from);
    if (params?.limit != null) query.set('limit', String(params.limit));
    const suffix = query.toString();
    return request<WorkoutSession[]>(
      `/api/v1/gyms/${gym}/workout-sessions${suffix ? `?${suffix}` : ''}`,
    );
  },

  audit: (gym: string) => request<AuditEntry[]>(`/api/v1/gyms/${gym}/audit`),

  /**
   * Change what somebody holds here (ADR-0031).
   *
   * The whole set, not a delta: `['member']` for a trainer demotes them. This
   * is what invitations became — everyone joins as a member and a manager
   * promotes them from the roster.
   */
  setCapacities: (gym: string, user: string, capacities: string[]) =>
    request<GymMember>(`/api/v1/gyms/${gym}/members/${user}/capacities`, {
      method: 'PUT',
      body: { capacities },
    }),

  /**
   * Create a staff account outright (ADR-0032).
   *
   * `temporary_password` in the response is shown **once** — it is not stored
   * in plaintext, not retrievable, and not in the audit trail. A screen that
   * displays it must not navigate away before somebody has written it down.
   */
  createStaff: (
    gym: string,
    input: { email: string; display_name: string; capacities: string[] },
  ) =>
    request<CreatedStaff>(`/api/v1/gyms/${gym}/staff`, { method: 'POST', body: input }),

  settings: (gym: string) => request<GymSettings>(`/api/v1/gyms/${gym}/settings`),
  setOpenRegistration: (gym: string, open: boolean) =>
    request<GymSettings>(`/api/v1/gyms/${gym}/settings/registration`, {
      method: 'PUT',
      body: { open_registration: open },
    }),

  recordPayment: (
    gym: string,
    invoice: string,
    input: { amount_minor: number; provider: string; received_on: string; note?: string },
  ) =>
    request<unknown>(`/api/v1/gyms/${gym}/invoices/${invoice}/payments`, {
      method: 'POST',
      body: input,
    }),
};

export type { paths };
