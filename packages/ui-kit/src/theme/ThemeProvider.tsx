import {
  type AppId,
  applyTheme,
  createThemeStore,
  loadThemePreference,
  type ResolvedTheme,
  saveThemePreference,
  type ThemePreference,
  type ThemeState,
  type ThemeStore,
} from '@officina/tokens';
import { createContext, type ReactNode, useContext, useEffect, useMemo, useSyncExternalStore } from 'react';

export type IconStyle = 'mono' | 'colorful';

interface ThemeContextValue {
  store: ThemeStore;
  app: AppId;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export interface ThemeProviderProps {
  children?: ReactNode;
  /** Active app; sets `data-app` (accent colour). Default `hub`. */
  app?: AppId;
  /** Initial preference when nothing is stored. Default `system`. */
  defaultTheme?: ThemePreference;
  /** Persist the preference to localStorage. Default true. */
  persist?: boolean;
  /** Element receiving `data-theme`/`data-app`; default `<html>`. */
  root?: HTMLElement;
}

/**
 * Owns the user's theme choice: reads it from localStorage, applies `data-theme`/`data-app` to `<html>`,
 * follows the OS while on `system`, and persists every change.
 */
export function ThemeProvider({
  children,
  app = 'hub',
  defaultTheme = 'system',
  persist = true,
  root,
}: ThemeProviderProps) {
  const store = useMemo(() => {
    const stored = persist ? loadThemePreference() : 'system';
    return createThemeStore(stored === 'system' ? defaultTheme : stored, root);
    // The store is created once per provider; later prop changes go through effects below.
  }, []);

  useEffect(() => {
    applyTheme({ app, root });
  }, [app, root]);

  useEffect(() => {
    if (!persist) return;
    return store.subscribe((s) => saveThemePreference(s.preference));
  }, [store, persist]);

  const value = useMemo(() => ({ store, app }), [store, app]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export interface UseThemeResult extends ThemeState {
  app: AppId;
  setPreference: (preference: ThemePreference) => void;
  /** Convenience: true when the rendered theme is dark or high contrast. */
  isDark: boolean;
}

const fallback: { store: ThemeStore | null } = { store: null };

/**
 * Current theme preference and resolved theme, re-rendering on change. Works without a provider
 * (a module-level store bound to `<html>`, not persisted) so isolated components and tests still render.
 */
export function useTheme(): UseThemeResult {
  const ctx = useContext(ThemeContext);
  if (!ctx && !fallback.store) fallback.store = createThemeStore('system');
  const store = ctx?.store ?? (fallback.store as ThemeStore);
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const resolved: ResolvedTheme = state.resolved;
  return {
    ...state,
    app: ctx?.app ?? 'hub',
    setPreference: store.setPreference,
    isDark: resolved !== 'light',
  };
}
