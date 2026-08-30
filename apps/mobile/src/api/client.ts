/**
 * API client for the Rust backend.
 *
 * The important part is `refreshOnce`. The backend rotates refresh tokens and
 * treats **reuse of an already-rotated token as theft**, revoking every session
 * for that user. So if two requests 401 at the same time and each independently
 * calls /refresh, the second call presents a token the first already rotated —
 * and the user is silently signed out of all their devices.
 *
 * `refreshOnce` therefore de-duplicates concurrent refreshes into a single
 * in-flight promise. This is not an optimisation; it is required for correctness
 * against this backend.
 */

import { API_URL } from '@/config';
import {
  clearRefreshToken,
  readRefreshToken,
  saveRefreshToken,
  useSession,
} from '@/session/store';

/** Problem Details shape returned by the backend. Branch on `code`, never text. */
export type ApiProblem = {
  title: string;
  status: number;
  code: string;
  detail?: string;
};

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(problem: ApiProblem) {
    super(problem.detail ?? problem.title);
    this.name = 'ApiError';
    this.status = problem.status;
    this.code = problem.code;
  }
}

type RequestOptions = {
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  body?: unknown;
  /** Attach the access token and retry once after refreshing. Default true. */
  authenticated?: boolean;
};

async function parseProblem(response: Response): Promise<ApiError> {
  try {
    const problem = (await response.json()) as ApiProblem;
    if (typeof problem?.code === 'string') return new ApiError(problem);
  } catch {
    // fall through to a synthetic problem
  }
  return new ApiError({
    title: response.statusText || 'Request failed',
    status: response.status,
    code: 'http.error',
  });
}

async function send(
  path: string,
  { method = 'GET', body, authenticated = true }: RequestOptions,
): Promise<Response> {
  const headers: Record<string, string> = { accept: 'application/json' };
  if (body !== undefined) headers['content-type'] = 'application/json';

  if (authenticated) {
    const token = useSession.getState().accessToken;
    if (token) headers.authorization = `Bearer ${token}`;
  }

  try {
    return await fetch(`${API_URL}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    // `fetch` rejects only when nothing answered — the API is stopped, the
    // phone is on a different network from the machine running it, or
    // EXPO_PUBLIC_API_URL points somewhere unroutable. Raised as a *coded*
    // ApiError rather than a bare TypeError so screens can branch on it the
    // way they branch on every other failure, and say which fault it was.
    throw new ApiError({
      title: 'Cannot reach the server',
      status: 0,
      code: 'network.unreachable',
    });
  }
}

// ------------------------------------------------------- single-flight refresh

let inFlightRefresh: Promise<string | null> | null = null;

/**
 * Exchange the stored refresh token for a new access token.
 * Concurrent callers share one request; see the module comment for why.
 *
 * @returns the new access token, or null if the session is no longer valid.
 */
export function refreshOnce(): Promise<string | null> {
  inFlightRefresh ??= (async () => {
    try {
      const refreshToken = await readRefreshToken();
      if (!refreshToken) return null;

      const response = await fetch(`${API_URL}/api/v1/auth/refresh`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ refresh_token: refreshToken }),
      });

      if (!response.ok) {
        // The token is dead (expired, rotated, or the family was revoked).
        await clearRefreshToken();
        return null;
      }

      const tokens = (await response.json()) as {
        access_token: string;
        refresh_token: string;
      };

      // Persist the rotated token *before* returning, so a crash right after
      // this point cannot leave us holding a token the server has retired.
      await saveRefreshToken(tokens.refresh_token);
      useSession.getState().setAccessToken(tokens.access_token);
      return tokens.access_token;
    } catch {
      // Network failure: keep the stored token, the session may still be valid.
      return null;
    } finally {
      inFlightRefresh = null;
    }
  })();

  return inFlightRefresh;
}

// ------------------------------------------------------------------ public API

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  let response = await send(path, options);

  if (response.status === 401 && (options.authenticated ?? true)) {
    const token = await refreshOnce();
    if (!token) {
      await signOutLocally();
      throw await parseProblem(response);
    }
    response = await send(path, options);
  }

  if (!response.ok) throw await parseProblem(response);

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export async function signOutLocally(): Promise<void> {
  await clearRefreshToken();
  useSession.getState().setSignedOut();
}
