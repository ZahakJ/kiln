/**
 * Where a recent file is, for Kiln Home's Location column: the last two folders of a
 * path on the device; "This device" for a file picked in the browser (`handle:` paths name no
 * folder the page may know); "This browser" for a file Kiln saved in the browser's own storage
 * (`opfs:`).
 */
import { t } from '@kiln/i18n';
export function locationOf(path: string): string {
  if (path.startsWith('handle:')) return t('This device');
  if (path.startsWith('opfs:')) return t('This browser');
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.slice(-3, -1).join(' › ') || t('This device');
}
