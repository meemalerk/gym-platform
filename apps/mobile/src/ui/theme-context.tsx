/**
 * Scheme-aware theme access.
 *
 * The app follows the device. That is only safe because every screen reads its
 * colours through these hooks — the static palette export that used to pin the
 * app to dark is gone (see the note in theme.ts), so there is no longer any
 * file that would render dark inside a light app.
 *
 * Both schemes are verified against WCAG AA by `scripts/verify-contrast.mjs`,
 * which is what makes following the system a safe default rather than a
 * cosmetic one.
 */
import { createContext, useContext, useMemo } from 'react';
import { useColorScheme } from 'react-native';

import { makeTokens, type Scheme, type Tokens } from '@/ui/theme';

const FOLLOW_SYSTEM = true;

const ThemeContext = createContext<Tokens>(makeTokens('dark'));

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const system = useColorScheme();
  const scheme: Scheme = FOLLOW_SYSTEM ? (system === 'light' ? 'light' : 'dark') : 'dark';
  const value = useMemo(() => makeTokens(scheme), [scheme]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTokens(): Tokens {
  return useContext(ThemeContext);
}

/**
 * Style factories, cached per scheme — StyleSheet objects are created once per
 * (factory, scheme), not on every render, which matters in data-dense lists.
 */
const styleCache = new WeakMap<object, Partial<Record<Scheme, unknown>>>();

export function useStyles<T>(factory: (t: Tokens) => T): T {
  const t = useTokens();
  let bySheme = styleCache.get(factory as object);
  if (!bySheme) {
    bySheme = {};
    styleCache.set(factory as object, bySheme);
  }
  if (!bySheme[t.scheme]) {
    bySheme[t.scheme] = factory(t);
  }
  return bySheme[t.scheme] as T;
}
