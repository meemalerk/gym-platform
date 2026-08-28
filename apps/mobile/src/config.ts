/**
 * Runtime configuration.
 *
 * `EXPO_PUBLIC_*` variables are inlined into the JS bundle at build time, so they
 * are **public**. Never put secrets here — anything bundled into a mobile app must
 * be considered readable by anyone who downloads it.
 */

const DEFAULT_API_URL = 'http://localhost:8080';

/**
 * Where the API lives.
 *
 * An **empty string is meaningful**, not a missing value: it means "same origin
 * as this page", which is why `??` alone is not enough — `??` would keep `''`,
 * but only by accident, and a later `||` refactor would silently turn it back
 * into the default. It is spelled out here because that is the setting the
 * browser demo runs on: nginx proxies `/api` to the backend, so the page and
 * the API share an origin and a single tunnel can expose both. Same-origin also
 * means no CORS is involved at all.
 *
 * Native builds always set an absolute URL — a phone has no origin to be
 * relative to.
 */
export const API_URL =
  process.env.EXPO_PUBLIC_API_URL === undefined
    ? DEFAULT_API_URL
    : process.env.EXPO_PUBLIC_API_URL;

/**
 * A physical device cannot reach the host's `localhost`. Point
 * `EXPO_PUBLIC_API_URL` at your machine's LAN address (e.g. http://192.168.1.5:8080)
 * when running on hardware.
 */
export const isLoopbackApi = /localhost|127\.0\.0\.1/.test(API_URL);

/**
 * The one gym this build serves, named rather than discovered.
 *
 * ADR-0023 caps a deployment to a single gym, so "which gym do you want to
 * join?" has exactly one answer and asking it is a screen between somebody and
 * the product. Naming it here removes the question.
 *
 * It also removes a real failure: onboarding used to list every gym with open
 * registration, and in a development database that is every throwaway gym the
 * verification suites ever created — so a new member was offered "Programme
 * Gym" and "Solo Box 1787848112219154700" alongside the real one. Nineteen
 * suites open a door they never close; no amount of tidying the data fixes that
 * for good, whereas not asking the question does.
 *
 * Unset (the browser demo, or a dev who has not set it) falls back to
 * discovery: the single open gym if there is exactly one, otherwise the picker.
 */
export const GYM_ID = process.env.EXPO_PUBLIC_GYM_ID?.trim() || null;
