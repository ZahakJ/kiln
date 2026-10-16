// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  applyTheme,
  createThemeStore,
  loadThemePreference,
  resolveTheme,
  saveThemePreference,
  THEME_BOOT_SCRIPT,
  THEME_STORAGE_KEY,
} from './theme.ts';

let dark = false;
const mqlListeners = new Set<() => void>();

beforeEach(() => {
  dark = false;
  mqlListeners.clear();
  vi.stubGlobal('matchMedia', (query: string) => ({
    get matches() {
      return query.includes('dark') ? dark : false;
    },
    media: query,
    addEventListener: (_: string, l: () => void) => mqlListeners.add(l),
    removeEventListener: (_: string, l: () => void) => mqlListeners.delete(l),
  }));
  document.documentElement.removeAttribute('data-theme');
  document.documentElement.removeAttribute('data-app');
  localStorage.clear();
});
afterEach(() => vi.unstubAllGlobals());

describe('theme runtime', () => {
  it('resolves system against prefers-color-scheme', () => {
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('hc', false)).toBe('hc');
  });

  it('applyTheme sets data-theme/data-app and removes data-theme for system', () => {
    const html = document.documentElement;
    expect(applyTheme({ theme: 'dark', app: 'grid' })).toBe('dark');
    expect(html.dataset.theme).toBe('dark');
    expect(html.dataset.app).toBe('grid');
    dark = true;
    expect(applyTheme({ theme: 'system' })).toBe('dark');
    expect(html.hasAttribute('data-theme')).toBe(false);
    expect(html.dataset.app).toBe('grid');
  });

  it('updates <meta name="theme-color">', () => {
    const meta = document.createElement('meta');
    meta.name = 'theme-color';
    document.head.append(meta);
    applyTheme({ theme: 'dark' });
    expect(meta.content).toBe('#17110E'); // the kit's dark window background
    meta.remove();
  });

  it('persists the preference and survives blocked storage', () => {
    saveThemePreference('hc');
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('hc');
    expect(loadThemePreference()).toBe('hc');
    saveThemePreference('system');
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBeNull();
    localStorage.setItem(THEME_STORAGE_KEY, 'neon');
    expect(loadThemePreference()).toBe('system');
    const broken = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
      removeItem: () => {
        throw new Error('blocked');
      },
    } as unknown as Storage;
    expect(loadThemePreference(broken)).toBe('system');
    expect(() => saveThemePreference('dark', broken)).not.toThrow();
  });

  it('store notifies subscribers and follows OS changes only while on system', () => {
    const store = createThemeStore('system');
    const seen: string[] = [];
    const unsubscribe = store.subscribe((s) => seen.push(`${s.preference}:${s.resolved}`));
    dark = true;
    for (const l of mqlListeners) l();
    expect(store.getState()).toEqual({ preference: 'system', resolved: 'dark' });
    store.setPreference('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    dark = false;
    for (const l of mqlListeners) l();
    expect(seen).toEqual(['system:dark', 'light:light']);
    unsubscribe();
    expect(mqlListeners.size).toBe(0);
  });

  it('boot script applies a saved explicit theme before React mounts', () => {
    localStorage.setItem(THEME_STORAGE_KEY, 'dark');
    new Function(THEME_BOOT_SCRIPT)();
    expect(document.documentElement.dataset.theme).toBe('dark');
  });
});
