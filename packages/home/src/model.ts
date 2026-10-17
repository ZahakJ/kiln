import type { DocKind } from '@kiln/protocol';

/** The apps Home creates documents for. */
export type HomeApp = 'write' | 'grid' | 'deck' | 'notes';

export const HOME_APPS: readonly HomeApp[] = ['write', 'grid', 'deck', 'notes'];

/** The app that opens a document kind (scratch documents are Write's). */
export function kindApp(kind: DocKind): HomeApp {
  switch (kind) {
    case 'Grid':
      return 'grid';
    case 'Deck':
      return 'deck';
    case 'Notes':
      return 'notes';
    default:
      return 'write';
  }
}

/** The core's document kind for an app. */
export function appKind(app: HomeApp): DocKind {
  return app === 'write' ? 'Write' : app === 'grid' ? 'Grid' : app === 'deck' ? 'Deck' : 'Notes';
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * "10 minutes ago", "1 hour ago", "Yesterday, 18:40", "Monday", "3 Oct": how Home shows when a file
 * was last opened (presentation of a timestamp the core recorded).
 */
export function relativeTime(ms: number, now: Date, locale?: string): string {
  const diff = now.getTime() - ms;
  if (!Number.isFinite(ms) || ms <= 0) return '';
  if (diff < MINUTE) return 'Just now';
  if (diff < HOUR) {
    const m = Math.floor(diff / MINUTE);
    return `${m} minute${m === 1 ? '' : 's'} ago`;
  }
  const then = new Date(ms);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const time = then.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hour12: false });
  if (ms >= startOfToday) {
    const h = Math.floor(diff / HOUR);
    return h < 6 ? `${h} hour${h === 1 ? '' : 's'} ago` : `Today, ${time}`;
  }
  if (ms >= startOfToday - DAY) return `Yesterday, ${time}`;
  if (ms >= startOfToday - 6 * DAY) return then.toLocaleDateString(locale, { weekday: 'long' });
  const sameYear = then.getFullYear() === now.getFullYear();
  return then.toLocaleDateString(locale, {
    day: 'numeric',
    month: 'short',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
}
