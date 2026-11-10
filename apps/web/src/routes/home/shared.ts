/**
 * Kiln Home ▸ Shared with me: the shared documents this device has joined from a link
 * (the collaboration rooms). Joining remembers the room, its server and the document's name in
 * the preference store; Home lists them, newest first, and opens one by its link again.
 */
import type { DocKind, RecentFile } from '@kiln/protocol';
import { loadTextPref, storeTextPref } from '../app-frame/prefs.ts';

const KEY = 'ui.home.shared';
/** At most this many rooms are remembered. */
const MAX = 30;

/** A shared document joined from a link. */
export interface SharedRoom {
  /** The link's route (`#/write?doc=<room>&server=<url>&token=<token>`). */
  link: string;
  /** The document's name when it was last opened. */
  name: string;
  kind: DocKind;
  /** When it was last opened, in milliseconds since the epoch. */
  at: number;
}

function parse(text: string | null): SharedRoom[] {
  if (!text) return [];
  try {
    const list: unknown = JSON.parse(text);
    if (!Array.isArray(list)) return [];
    return list.filter(
      (r): r is SharedRoom =>
        typeof r === 'object' &&
        r !== null &&
        typeof r.link === 'string' &&
        r.link.startsWith('#/') &&
        typeof r.name === 'string' &&
        typeof r.kind === 'string' &&
        typeof r.at === 'number',
    );
  } catch {
    return [];
  }
}

/** The remembered shared documents, newest first. */
export async function loadShared(): Promise<SharedRoom[]> {
  return parse(await loadTextPref(KEY)).sort((a, b) => b.at - a.at);
}

/** Remembers (or refreshes) a shared document after it was joined. */
export async function rememberShared(room: SharedRoom): Promise<void> {
  const rest = (await loadShared()).filter((r) => roomOf(r.link) !== roomOf(room.link));
  await storeTextPref(KEY, JSON.stringify([room, ...rest].slice(0, MAX)));
}

/** The room id of a link, so a room joined again replaces its entry. */
function roomOf(link: string): string {
  const q = new URLSearchParams(link.slice(link.indexOf('?') + 1));
  return `${q.get('server') ?? ''}\n${q.get('doc') ?? ''}`;
}

/** Shared documents as the rows Home lists (the link stands in for the path). */
export function sharedRows(rooms: readonly SharedRoom[]): RecentFile[] {
  return rooms.map((r) => ({
    path: r.link,
    name: r.name,
    kind: r.kind,
    opened_at_ms: r.at,
    pinned: false,
    thumbnail: null,
  }));
}
