/**
 * The text of apps/desktop/src-tauri/menu.json: {@link buildDesktopMenu} over the locales of
 * `@kiln/i18n` (read from disk, so this module runs in Node only: the script and the unit test).
 */
import { readdirSync, readFileSync } from 'node:fs';
import { buildDesktopMenu, type LocaleTables } from './desktop-menu.ts';

const LOCALES_DIR = new URL('../../i18n/locales/', import.meta.url);

/** The interface locales, English first. */
export function readLocales(): LocaleTables {
  const names = readdirSync(LOCALES_DIR)
    .filter((f) => f.endsWith('.json'))
    .map((f) => f.slice(0, -'.json'.length))
    .sort((a, b) => (a === 'en' ? -1 : b === 'en' ? 1 : a.localeCompare(b)));
  return Object.fromEntries(
    names.map((n) => [
      n,
      JSON.parse(readFileSync(new URL(`${n}.json`, LOCALES_DIR), 'utf8')) as Record<string, string>,
    ]),
  );
}

export function desktopMenuText(): string {
  return `${JSON.stringify(buildDesktopMenu(readLocales()), null, 2)}\n`;
}
