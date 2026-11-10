/**
 * React bindings. The root component calls `useLanguage()` (or renders `LanguageRoot`), so a
 * language switch re-renders the whole tree and every `t` call reads the new language; memoised
 * values that hold translated text list the language among their dependencies.
 */
import { Fragment, type ReactNode, useSyncExternalStore } from 'react';
import { I18nextProvider, initReactI18next } from 'react-i18next';
import { direction, i18n, type Locale, language, onLanguageChange, t } from './index.ts';

i18n.use(initReactI18next);

/** The current interface language; re-renders on a switch. */
export function useLanguage(): Locale {
  return useSyncExternalStore(onLanguageChange, language, language);
}

/** `t`, re-rendering the component on a language switch. */
export function useT(): typeof t {
  useLanguage();
  return t;
}

/** The text direction of the interface (`rtl` in Arabic). */
export function useDirection(): 'ltr' | 'rtl' {
  return direction(useLanguage());
}

/**
 * Provides i18next to the tree and renders it again on every language switch. `render` builds the
 * tree (a function, so its elements are created anew in the new language).
 */
export function LanguageRoot({ render }: { render: (locale: Locale) => ReactNode }) {
  const locale = useLanguage();
  return <I18nextProvider i18n={i18n}>{render(locale)}</I18nextProvider>;
}

const MARK = '\u2063';

/**
 * A message whose placeholders hold markup (`Do you want to save the changes you made to
 * {name}?` with the name in bold): text values are formatted as usual, element values are put in
 * the translated sentence where their placeholder is.
 */
export function Message({ id, values }: { id: string; values: Readonly<Record<string, ReactNode>> }) {
  useLanguage();
  const nodes: ReactNode[] = [];
  const plain: Record<string, string | number> = {};
  for (const [k, v] of Object.entries(values)) {
    if (typeof v === 'string' || typeof v === 'number') plain[k] = v;
    else {
      plain[k] = `${MARK}${nodes.length}${MARK}`;
      nodes.push(v);
    }
  }
  const parts = t(id, plain).split(new RegExp(`${MARK}(\\d+)${MARK}`));
  return <>{parts.map((p, i) => (i % 2 ? <Fragment key={i}>{nodes[Number(p)]}</Fragment> : p))}</>;
}
