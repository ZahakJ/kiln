/**
 * Kiln's interface languages: i18next with ICU messages, the English source
 * locale and its German and Arabic translations, the language preference, the text direction of the
 * chrome and locale-aware number and date formatting through `Intl`.
 *
 * Keys are either a command's `cmd.<id>` (its title; `cmd.<id>.description` its ScreenTip text) or
 * the English text itself, so the English locale is the identity and a missing translation falls
 * back to readable English. Messages with arguments use ICU syntax (`{count, plural, one {# cell}
 * other {# cells}}`). Only interface strings live here: document text comes from the core, and
 * Excel function names stay as Excel spells them.
 */
import i18next, { type i18n as I18n } from 'i18next';
import ICU from 'i18next-icu';
import en from '../locales/en.json' with { type: 'json' };

/** The interface languages. `en` is the source. */
export const LOCALES = ['en', 'de', 'ar'] as const;
export type Locale = (typeof LOCALES)[number];

/** The Options ▸ Language choice: a language, or the system's. */
export type LanguagePref = 'system' | Locale;

/** Each language in its own name, as the language menu lists it (never translated). */
export const LANGUAGE_NAMES: Readonly<Record<Locale, string>> = {
  en: 'English',
  de: 'Deutsch',
  ar: 'العربية',
};

/** Right-to-left interface languages. */
const RTL: ReadonlySet<Locale> = new Set<Locale>(['ar']);

export type Messages = Readonly<Record<string, string>>;

/** The header entry of a locale file (status and provenance); not a message. */
export const HEADER_KEY = '//';

const loaders: Readonly<Record<Exclude<Locale, 'en'>, () => Promise<Messages>>> = {
  de: () => import('../locales/de.json').then((m) => m.default as Messages),
  ar: () => import('../locales/ar.json').then((m) => m.default as Messages),
};

const listeners = new Set<(locale: Locale) => void>();

function createInstance(): I18n {
  const instance = i18next.createInstance();
  instance.use(new ICU({ memoize: true, bindI18n: 'languageChanged', bindI18nStore: 'added' }));
  void instance.init({
    lng: 'en',
    fallbackLng: 'en',
    supportedLngs: [...LOCALES],
    resources: { en: { translation: en as Messages } },
    // Keys are English sentences: dots and colons are part of them.
    keySeparator: false,
    nsSeparator: false,
    returnNull: false,
    returnEmptyString: false,
    initAsync: false,
    interpolation: { escapeValue: false },
    react: { bindI18n: 'languageChanged', bindI18nStore: 'added' },
  });
  return instance;
}

/** The i18next instance of the window (react-i18next binds to it). */
export const i18n: I18n = createInstance();

let current: Locale = 'en';
let pending: Promise<Locale> | null = null;

/** Whether `value` names an interface language. */
export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (LOCALES as readonly string[]).includes(value);
}

/** Whether `value` is a valid Options ▸ Language choice. */
export function isLanguagePref(value: unknown): value is LanguagePref {
  return value === 'system' || isLocale(value);
}

/**
 * The language a preference means: the language itself, or for `system` the first of the
 * browser's (or operating system's) languages Kiln has, else English.
 */
export function resolveLanguage(
  pref: LanguagePref | string | undefined,
  system: readonly string[] = systemLanguages(),
): Locale {
  if (isLocale(pref)) return pref;
  for (const tag of system) {
    const primary = tag.toLowerCase().split(/[-_]/)[0];
    if (isLocale(primary)) return primary;
  }
  return 'en';
}

function systemLanguages(): readonly string[] {
  const nav = globalThis.navigator;
  if (!nav) return [];
  return nav.languages?.length ? nav.languages : nav.language ? [nav.language] : [];
}

/** The interface language in use. */
export function language(): Locale {
  return current;
}

/** The text direction of an interface language (default: the current one). */
export function direction(locale: Locale = current): 'ltr' | 'rtl' {
  return RTL.has(locale) ? 'rtl' : 'ltr';
}

/** Whether the interface is right to left. */
export function isRtl(): boolean {
  return direction() === 'rtl';
}

/** Loads a language's messages (English is built in; German and Arabic load on first use). */
async function load(locale: Locale): Promise<void> {
  if (locale === 'en' || i18n.hasResourceBundle(locale, 'translation')) return;
  const messages = await loaders[locale]();
  const { [HEADER_KEY]: _header, ...rest } = messages;
  i18n.addResourceBundle(locale, 'translation', rest, true, true);
}

/**
 * Switches the interface language (a preference, resolved with `resolveLanguage`), loading its
 * messages first. Components re-render through `useLanguage`/`useT`; the root element's `lang` and
 * `dir` follow when `applyToDocument` is on (the default in a browser).
 */
export function setLanguage(
  pref: LanguagePref | string | undefined,
  options: { applyToDocument?: boolean } = {},
): Promise<Locale> {
  const locale = resolveLanguage(pref);
  const apply = options.applyToDocument ?? typeof document !== 'undefined';
  const run = (async () => {
    await load(locale);
    if (locale !== current || i18n.language !== locale) {
      current = locale;
      await i18n.changeLanguage(locale);
      formatters.clear();
      for (const l of [...listeners]) l(locale);
    }
    if (apply) applyToDocument();
    return locale;
  })();
  pending = run;
  return run.finally(() => {
    if (pending === run) pending = null;
  });
}

/** Resolves when a language switch in progress has finished. */
export function languageReady(): Promise<Locale> {
  return pending ?? Promise.resolve(current);
}

/** Sets `lang` and `dir` on the root element (the chrome mirrors in right-to-left languages). */
export function applyToDocument(doc: Document | undefined = globalThis.document): void {
  const root = doc?.documentElement;
  if (!root) return;
  root.lang = current;
  root.dir = direction();
}

/** Calls `cb` after every language switch; returns the unsubscribe. */
export function onLanguageChange(cb: (locale: Locale) => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Message arguments: ICU `{name}`, `{count, plural, …}`, `{n, number}`, `{d, date}`. */
export type MessageValues = Readonly<Record<string, string | number | boolean | Date | null | undefined>>;

/**
 * The message for `key` in the interface language. Keys are `cmd.<id>`-style ids or the English
 * text; an unknown key is returned as it is (so an English key reads as English).
 */
export function t(key: string, values?: MessageValues): string {
  const out = i18n.t(key, values as Record<string, unknown> | undefined);
  return typeof out === 'string' ? out : key;
}

/** `t` with a fallback for keys that may not exist (`cmd.<id>` of a command added at run time). */
export function tOr(key: string, fallback: string, values?: MessageValues): string {
  if (!i18n.exists(key)) return values ? t(fallback, values) : t(fallback);
  return t(key, values);
}

/**
 * Translates text that may or may not be a message: interface labels kept as data (ribbon labels,
 * `msg` tables) are translated, anything else (a document's layout name, a file name) is returned
 * as it is, never parsed as ICU.
 */
export function tx(text: string): string;
export function tx(text: string | undefined): string | undefined;
export function tx(text: string | undefined): string | undefined {
  if (text === undefined || text === '') return text;
  return Object.hasOwn(en, text) ? t(text) : text;
}

/** Option lists kept as data (`{ value, label: msg('…') }`) with their labels translated. */
export function txOptions<O extends { label?: string }>(options: readonly O[]): O[] {
  return options.map((o) => (o.label ? { ...o, label: tx(o.label) } : o));
}

/** `[value, label]` pairs kept as data (`['Sum', msg('Sum')]`) with their labels translated. */
export function txPairs<V>(pairs: readonly (readonly [V, string])[]): [V, string][] {
  return pairs.map(([v, l]) => [v, tx(l)]);
}

/** Whether a message exists for `key` (in English, the source). */
export function hasMessage(key: string): boolean {
  return Object.hasOwn(en, key);
}

/**
 * Marks a string constant as a message without translating it (module-level tables whose entries
 * are translated where they are shown, with `t(entry.label)`). The extractor collects these.
 */
export function msg<const S extends string>(text: S): S {
  return text;
}

/** The English messages (the source locale). */
export const SOURCE_MESSAGES: Messages = en as Messages;

/** The messages of a locale file, header removed (tests, tools). */
export async function messagesOf(locale: Locale): Promise<Messages> {
  const all = locale === 'en' ? (en as Messages) : await loaders[locale]();
  const { [HEADER_KEY]: _header, ...rest } = all;
  return rest;
}

// ── Numbers and dates (Intl, in the interface language) ─────────────────────────────────────────

/** The `Intl` locale of each interface language (English is written the British way). */
const INTL_LOCALES: Readonly<Record<Locale, string>> = { en: 'en-GB', de: 'de-DE', ar: 'ar' };

/** The `Intl` locale of the interface language, for formatting numbers, dates and lists. */
export function intlLocale(locale: Locale = current): string {
  return INTL_LOCALES[locale];
}

const formatters = new Map<string, Intl.NumberFormat | Intl.DateTimeFormat | Intl.ListFormat>();

function cached<T extends Intl.NumberFormat | Intl.DateTimeFormat | Intl.ListFormat>(
  kind: string,
  options: object | undefined,
  make: () => T,
): T {
  const key = `${kind}|${current}|${options ? JSON.stringify(options) : ''}`;
  let f = formatters.get(key) as T | undefined;
  if (!f) {
    f = make();
    formatters.set(key, f);
  }
  return f;
}

/** A number in the interface language (`1.234,5` in German, `١٬٢٣٤٫٥` in Arabic). */
export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return cached('n', options, () => new Intl.NumberFormat(intlLocale(), options)).format(value);
}

/** An integer count in the interface language. */
export function formatCount(value: number): string {
  return formatNumber(value, { maximumFractionDigits: 0 });
}

/** A percentage given as a whole number (`100` → `100 %` in German). */
export function formatPercent(value: number, options?: Intl.NumberFormatOptions): string {
  return formatNumber(value / 100, { style: 'percent', maximumFractionDigits: 0, ...options });
}

/** A date or time in the interface language. */
export function formatDate(value: Date | number, options?: Intl.DateTimeFormatOptions): string {
  return cached('d', options, () => new Intl.DateTimeFormat(intlLocale(), options)).format(value);
}

/** A list in the interface language ("A, B and C", "A, B und C"). */
export function formatList(
  items: readonly string[],
  type: 'conjunction' | 'disjunction' = 'conjunction',
): string {
  return cached('l', { type }, () => new Intl.ListFormat(intlLocale(), { type })).format(items);
}

const displayNames = new Map<string, Intl.DisplayNames>();

/** A language tag's name in the interface language (`de-DE` → "German (Germany)", "Deutsch (Deutschland)"). */
export function languageName(tag: string): string {
  let names = displayNames.get(current);
  if (!names) {
    names = new Intl.DisplayNames([intlLocale()], { type: 'language', languageDisplay: 'standard' });
    displayNames.set(current, names);
  }
  try {
    return names.of(tag) ?? tag;
  } catch {
    // Not a language tag (a name already): shown as it is.
    return tag;
  }
}

/** Compares two strings in the interface language (sorted lists of the chrome). */
export function compareText(a: string, b: string): number {
  return a.localeCompare(b, intlLocale());
}

const PLAIN_NUMBER = /^(-?)(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d+))?(%?)$/;

/**
 * A number the core formatted in its neutral form (`1,234.50`, `-12.5%`, `7`) shown in the
 * interface language's separators and digits; any other text (currency, dates, errors, text) is
 * returned unchanged. Used for chrome read-outs such as the Grid status bar.
 */
export function localizeNumberText(text: string): string {
  if (current === 'en') return text;
  const m = PLAIN_NUMBER.exec(text.trim());
  if (!m) return text;
  const [, sign, int = '', frac = '', pct] = m;
  const value = Number(`${sign}${int.replaceAll(',', '')}${frac ? `.${frac}` : ''}`);
  if (!Number.isFinite(value)) return text;
  const digits = frac.length;
  const out = formatNumber(pct ? value / 100 : value, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    useGrouping: int.includes(','),
    ...(pct ? { style: 'percent' } : {}),
  });
  return out;
}

/**
 * The step an arrow key takes along a row, in reading order: ArrowRight is the next item and
 * ArrowLeft the previous one, the other way round when `el` (or the interface) is right to left.
 */
export function readingStep(key: string, el?: Element | null): 1 | -1 | 0 {
  const step = key === 'ArrowRight' ? 1 : key === 'ArrowLeft' ? -1 : 0;
  if (!step) return 0;
  const rtl =
    el && typeof getComputedStyle === 'function' ? getComputedStyle(el).direction === 'rtl' : isRtl();
  return rtl ? (-step as 1 | -1) : step;
}
