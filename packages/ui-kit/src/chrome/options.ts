import { type Locale, resolveLanguage } from '@kiln/i18n';
import type { CoreOptions } from '@kiln/protocol';
import { useCallback, useSyncExternalStore } from 'react';

/** A unit lengths are shown in (rulers, the ribbon's boxes, dialogs). */
export type MeasureUnit = 'in' | 'cm' | 'mm' | 'pt';

/** User preferences edited in File ▸ Options. Theme lives in the ThemeProvider. */
export interface OfficeOptions {
  general: {
    userName: string;
    initials: string;
    /** Show the mini toolbar on selection. */
    showMiniToolbar: boolean;
    /** Live Preview of gallery choices. */
    livePreview: boolean;
    /** "Classic Office look": colourful icons and a solid title bar in the app's colour. */
    classicLook: boolean;
  };
  display: {
    ribbonLayout: 'auto' | 'classic' | 'simplified';
    /** ScreenTips with descriptions (Office "ScreenTip style"). */
    screenTips: 'full' | 'compact' | 'off';
    showShortcutsInTips: boolean;
    /** Advanced ▸ Show measurements in units of (`auto`: inches in the US, else centimetres). */
    measurementUnit: 'auto' | MeasureUnit;
  };
  proofing: {
    checkSpellingAsYouType: boolean;
    ignoreUppercase: boolean;
    ignoreWordsWithNumbers: boolean;
    ignoreInternetAddresses: boolean;
    autoCorrect: boolean;
    /** Markdown shortcuts as you type (`# `, `- `, `**bold**`…; Kiln Write). */
    markdownShortcuts: boolean;
    /** "Straight quotes" with “smart quotes”. */
    smartQuotes: boolean;
    /** Hyphens (--) with dashes (—). */
    smartDashes: boolean;
    /** `...` with an ellipsis (…). */
    smartEllipsis: boolean;
    /** Fractions (1/2) with fraction characters (½). */
    smartFractions: boolean;
    /** AutoCorrect ▸ Capitalise first letter of sentences. */
    capitalizeSentences: boolean;
    /** AutoCorrect ▸ Capitalise names of days. */
    capitalizeDays: boolean;
    /** AutoCorrect ▸ Correct TWo INitial CApitals. */
    twoInitialCaps: boolean;
    /** AutoCorrect entries the user added: [typed, replacement]. */
    autoCorrectAdded: [string, string][];
    /** Built-in AutoCorrect entries the user deleted (what is typed). */
    autoCorrectRemoved: string[];
  };
  save: {
    /** Turn AutoSave on when a file that can be saved automatically opens. */
    autoSave: boolean;
    /** Seconds AutoSave waits after the last change before it writes the file. */
    autoSaveInterval: number;
    /** Keep a recovery journal of every change (documents opened from now on). */
    keepRecoveryInfo: boolean;
    /** Format a new document is saved in the first time. */
    defaultFormat: 'office' | 'odf';
  };
  advanced: {
    /** Show this number of recent documents (Home, File ▸ Open). */
    recentCount: number;
    /** Allow text to be dragged and dropped (Kiln Write). */
    dragAndDrop: boolean;
    /** After pressing Enter, move the selection (Kiln Grid). */
    enterMoves: boolean;
    /** Which way Enter moves the selection. */
    enterDirection: 'down' | 'right' | 'up' | 'left';
  };
  language: {
    /** Interface language. */
    ui: string;
    /**
     * Default editing (proofing) language for new documents: a language tag (`de-DE`), or `auto`
     * to follow the interface language, as Office does until the user picks one. Read it through
     * `editingLanguage`.
     */
    editing: 'auto' | string;
  };
  privacy: {
    /** AI assistant provider: none (default), or the local sample. No network calls by default. */
    assistant: 'none' | 'echo';
  };
}

export const DEFAULT_OPTIONS: OfficeOptions = {
  general: { userName: '', initials: '', showMiniToolbar: true, livePreview: true, classicLook: false },
  display: { ribbonLayout: 'auto', screenTips: 'full', showShortcutsInTips: true, measurementUnit: 'auto' },
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
    capitalizeSentences: true,
    capitalizeDays: true,
    twoInitialCaps: true,
    autoCorrectAdded: [],
    autoCorrectRemoved: [],
  },
  save: {
    autoSave: false,
    autoSaveInterval: 2,
    keepRecoveryInfo: true,
    defaultFormat: 'office',
  },
  advanced: { recentCount: 25, dragAndDrop: true, enterMoves: true, enterDirection: 'down' },
  language: { ui: 'en', editing: 'auto' },
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

/** Where Word uses inches by default (the US and the few countries that measure as it does). */
function localeInches(): boolean {
  const lang = typeof navigator !== 'undefined' ? navigator.language : 'en-US';
  return /^en-(US|PH)$|^en$/i.test(lang) || /-(US|LR|MM)$/i.test(lang);
}

/** The unit lengths are shown in: the user's choice, else the locale's (Word's default). */
export function measurementUnit(options: OfficeOptions = read()): MeasureUnit {
  const u = options.display.measurementUnit;
  if (u && u !== 'auto') return u;
  return localeInches() ? 'in' : 'cm';
}

/** The editing languages Options ▸ Language offers (proofing, names and direction of new files). */
export const EDITING_LANGUAGES = [
  'en-US',
  'en-GB',
  'de-DE',
  'fr-FR',
  'es-ES',
  'ar-SA',
  'he-IL',
  'fa-IR',
  'ur-PK',
] as const;

/** The editing language each interface language brings with it (Office's pairing). */
const EDITING_OF_UI: Readonly<Record<Locale, string>> = { en: 'en-US', de: 'de-DE', ar: 'ar-SA' };

/**
 * The editing language new documents are made in: the one chosen in Options ▸ Language, else the
 * interface language's (Arabic → ar-SA, German → de-DE, English → en-US). When the interface
 * follows the system, a system language Kiln offers for editing is taken as it is (en-GB).
 */
export function editingLanguage(
  options: OfficeOptions = read(),
  system: readonly string[] = systemLanguages(),
): string {
  const chosen = options.language.editing;
  if (chosen && chosen !== 'auto') return chosen;
  const ui = resolveLanguage(options.language.ui, system);
  if (options.language.ui === 'system' || !options.language.ui) {
    for (const tag of system) {
      const norm = tag.replace('_', '-');
      if (norm.split('-')[0]?.toLowerCase() !== ui) continue;
      const known = EDITING_LANGUAGES.find((l) => l.toLowerCase() === norm.toLowerCase());
      if (known) return known;
    }
  }
  return EDITING_OF_UI[ui];
}

function systemLanguages(): readonly string[] {
  const nav = globalThis.navigator;
  if (!nav) return [];
  return nav.languages?.length ? nav.languages : nav.language ? [nav.language] : [];
}

/** Calls `cb` whenever the options change in this window; returns the unsubscribe. */
export function subscribeOptions(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** The measurement unit, re-rendering when the user changes it. */
export function useMeasurementUnit(): MeasureUnit {
  const [o] = useOfficeOptions();
  return measurementUnit(o);
}

/** The options the core applies (`SuiteCommand::SetOptions`), from the user's options. */
export function coreOptionsOf(o: OfficeOptions = read()): CoreOptions {
  const delay = Number.isFinite(o.save.autoSaveInterval) ? Math.round(o.save.autoSaveInterval) : 2;
  return {
    autosave_delay_s: Math.min(600, Math.max(1, delay)),
    recovery: o.save.keepRecoveryInfo,
    spell_ignore_uppercase: o.proofing.ignoreUppercase,
    spell_ignore_numbers: o.proofing.ignoreWordsWithNumbers,
    spell_ignore_addresses: o.proofing.ignoreInternetAddresses,
    // What the core draws in the interface language (Deck's placeholder prompts).
    ui_language: resolveLanguage(o.language.ui, systemLanguages()),
  };
}

/**
 * Options ▸ Advanced ▸ Show this number of Recent Documents: the pinned files, then the most
 * recent others up to the count (the list keeps its order).
 */
export function limitRecents<T extends { pinned: boolean }>(files: readonly T[], count: number): T[] {
  let others = 0;
  const n = Math.max(0, Math.round(count));
  return files.filter((f) => f.pinned || others++ < n);
}
