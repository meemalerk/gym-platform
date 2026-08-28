/**
 * Platform-aware storage for the refresh token.
 *
 * **Native (the real target):** `expo-secure-store`, backed by the iOS Keychain and
 * Android Keystore. This is the only path that ships to users.
 *
 * **Web:** `expo-secure-store` does not exist on web. The web target here is a
 * *development and verification* convenience so the app can be driven in a browser
 * without a simulator — it is NOT a supported production surface.
 *
 * Per ADR-0009 the real browser client is a separate React + Vite app, and per the
 * security research a browser client must use the BFF pattern (tokens held
 * server-side, browser gets an HttpOnly cookie) rather than storing a long-lived
 * refresh token in JS-reachable storage, which is XSS-exposed.
 *
 * So the web implementation deliberately uses `sessionStorage`, not
 * `localStorage`: it dies with the tab, which limits the blast radius and makes it
 * obviously unsuitable for production — a property we want to be loud, not subtle.
 */

import { Platform } from 'react-native';

const isWeb = Platform.OS === 'web';

type Store = {
  set: (key: string, value: string) => Promise<void>;
  get: (key: string) => Promise<string | null>;
  remove: (key: string) => Promise<void>;
};

const nativeStore: Store = {
  async set(key, value) {
    const SecureStore = await import('expo-secure-store');
    await SecureStore.setItemAsync(key, value);
  },
  async get(key) {
    const SecureStore = await import('expo-secure-store');
    return SecureStore.getItemAsync(key);
  },
  async remove(key) {
    const SecureStore = await import('expo-secure-store');
    await SecureStore.deleteItemAsync(key);
  },
};

const webDevStore: Store = {
  async set(key, value) {
    globalThis.sessionStorage?.setItem(key, value);
  },
  async get(key) {
    return globalThis.sessionStorage?.getItem(key) ?? null;
  },
  async remove(key) {
    globalThis.sessionStorage?.removeItem(key);
  },
};

export const secureStorage: Store = isWeb ? webDevStore : nativeStore;
