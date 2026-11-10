/**
 * What the routes read from a location hash, apart from the routes themselves: `App.tsx` parses
 * a hash without loading the screens it may lazily show (the slide show, the shared document's
 * collaboration code), so the page's first script stays small (budget B12).
 */

/** A shared document's link, from the route's query. */
export interface ShareLink {
  room: string;
  server: string;
  token: string;
}

/** The link of a route query (`doc` is a room, not a number), if it is one. */
export function shareLinkOf(query: string): ShareLink | null {
  const p = new URLSearchParams(query);
  const room = p.get('doc');
  const server = p.get('server');
  if (!room || !server || /^\d+$/.test(room)) return null;
  return { room, server, token: p.get('token') ?? '' };
}

export interface PresentParams {
  doc: number | null;
  view: 'show' | 'presenter' | 'audience';
  channel: string | null;
  tab: boolean;
  manualClock: boolean;
}

/** The parameters of a `#/present` hash. */
export function parsePresent(query: string): PresentParams {
  const p = new URLSearchParams(query);
  const doc = Number(p.get('doc'));
  const view = p.get('view');
  return {
    doc: Number.isInteger(doc) && doc > 0 ? doc : null,
    view: view === 'presenter' || view === 'audience' ? view : 'show',
    channel: p.get('channel'),
    tab: p.get('tab') === '1',
    manualClock: p.get('clock') === 'manual',
  };
}
