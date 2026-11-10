/**
 * The interface language: File ▸ Options ▸ Language ▸ Interface language, persisted with
 * the other options. `System` follows the browser's (or the operating system's) languages.
 */
import { type Locale, setLanguage } from '@kiln/i18n';
import { useLanguage } from '@kiln/i18n/react';
import { useEffect } from 'react';
import { loadOptions, useOfficeOptions } from './options.ts';

/** Applies the stored choice before the first render (no flash of English). */
export function startInterfaceLanguage(): Promise<Locale> {
  return setLanguage(loadOptions().language.ui);
}

/**
 * Keeps the interface in the chosen language and re-renders the calling component (the root) on
 * every switch, so the whole window changes language at once.
 */
export function useInterfaceLanguage(): Locale {
  const [options] = useOfficeOptions();
  const pref = options.language.ui;
  useEffect(() => {
    void setLanguage(pref);
    if (pref !== 'system') return;
    const follow = () => void setLanguage('system');
    globalThis.addEventListener?.('languagechange', follow);
    return () => globalThis.removeEventListener?.('languagechange', follow);
  }, [pref]);
  return useLanguage();
}
