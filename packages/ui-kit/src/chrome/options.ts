import { useCallback, useSyncExternalStore } from 'react';

/** User preferences edited in File ▸ Options (PLAN.md §1.5). Theme lives in the ThemeProvider. */
export interface OfficeOptions {
  general: {
    userName: string;
    initials: string;
    /** Show the mini toolbar on selection. */
    showMiniToolbar: boolean;
    /** Live Preview of gallery choices. */
    livePreview: boolean;
    /** "Classic Office look": colourful icons (PLAN.md §1.9). */
    classicLook: boolean;
  };
  display: {
    ribbonLayout: 'auto' | 'classic' | 'simplified';
    /** ScreenTips with descriptions (Office "ScreenTip style"). */
    screenTips: 'full' | 'compact' | 'off';
    showShortcutsInTips: boolean;
  };
  proofing: {
    checkSpellingAsYouType: boolean;
    ignoreUppercase: boolean;
    ignoreWordsWithNumbers: boolean;
    ignoreInternetAddresses: boolean;
    autoCorrect: boolean;
    /** Markdown shortcuts as you type (`# `, `- `, `**bold**`…; Kiln Write, WP-66). */
    markdownShortcuts: boolean;
    /** "Straight quotes" with “smart quotes”. */
    smartQuotes: boolean;
    /** Hyphens (--) with dashes (—). */
    smartDashes: boolean;
    /** `...` with an ellipsis (…). */
    smartEllipsis: boolean;
    /** Fractions (1/2) with fraction characters (½). */
    smartFractions: boolean;
  };
  save: {
    /** AutoSave on by default for files that support it. */
    autoSave: boolean;
    /** Seconds between AutoSaves. */
    autoSaveInterval: number;
    keepRecoveryInfo: boolean;
    /** Minutes between recovery snapshots. */
    recoveryInterval: number;
    /** Format for new documents. */
    defaultFormat: 'office' | 'odf';
  };
  language: {
    /** Interface language. */
    ui: string;
    /** Default editing (proofing) language for new documents. */
    editing: string;
  };
  privacy: {
    /** AI assistant provider: none (default), or the local sample. No network calls by default. */
    assistant: 'none' | 'echo';
  };
}

export const DEFAULT_OPTIONS: OfficeOptions = {
  general: { userName: '', initials: '', showMiniToolbar: true, livePreview: true, classicLook: false },
  display: { ribbonLayout: 'auto', screenTips: 'full', showShortcutsInTips: true },
  proofing: {
    checkSpellingAsYouType: true,
    ignoreUppercase: true,
    ignoreWordsWithNumbers: true,
    ignoreInternetAddresses: true,
    autoCorrect: true,
    markdownShortcuts: true,
    smartQuotes: true,
    smartDashes: true,
    smartEllipsis: true,
    smartFractions: true,
  },
  save: {
    autoSave: true,
    autoSaveInterval: 5,
    keepRecoveryInfo: true,
    recoveryInterval: 10,
    defaultFormat: 'office',
  },
  language: { ui: 'en', editing: 'en-US' },
  privacy: { assistant: 'none' },
};

export type OptionsPatch = { [S in keyof OfficeOptions]?: Partial<OfficeOptions[S]> };

export function mergeOptions(base: OfficeOptions, patch: OptionsPatch): OfficeOptions {
  const out = { ...base } as OfficeOptions;
  for (const key of Object.keys(patch) as (keyof OfficeOptions)[]) {
    (out as unknown as Record<string, unknown>)[key] = { ...base[key], ...(patch[key] ?? {}) };
  }
  return out;
}

const STORAGE_KEY = 'kiln.options';
const listeners = new Set<() => void>();
let cache: OfficeOptions | null = null;

function read(): OfficeOptions {
  if (cache) return cache;
  let stored: OptionsPatch = {};
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (raw) stored = JSON.parse(raw) as OptionsPatch;
  } catch {
    stored = {};
  }
  cache = mergeOptions(DEFAULT_OPTIONS, stored && typeof stored === 'object' ? stored : {});
  return cache;
}

/** Reads the stored options (defaults merged in). */
export function loadOptions(): OfficeOptions {
  return read();
}

/** Applies a patch, persists it and notifies `useOfficeOptions` users in this window. */
export function saveOptions(patch: OptionsPatch): OfficeOptions {
  cache = mergeOptions(read(), patch);
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // Preferences are best-effort.
  }
  for (const l of listeners) l();
  return cache;
}

/** Forgets the in-memory copy (tests, or after another window changed storage). */
export function resetOptionsCache(): void {
  cache = null;
  for (const l of listeners) l();
}

/** The user's options, persisted to localStorage, shared by every component in the window. */
export function useOfficeOptions(): [OfficeOptions, (patch: OptionsPatch) => void] {
  const value = useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      const onStorage = (e: StorageEvent) => {
        if (e.key === STORAGE_KEY) resetOptionsCache();
      };
      globalThis.addEventListener?.('storage', onStorage);
      return () => {
        listeners.delete(cb);
        globalThis.removeEventListener?.('storage', onStorage);
      };
    },
    read,
    read,
  );
  const update = useCallback((patch: OptionsPatch) => {
    saveOptions(patch);
  }, []);
  return [value, update];
}
