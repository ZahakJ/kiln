/**
 * Selection announcements, as Office reads them with a screen reader: selected text in a
 * document ("Hello world, selected") and shapes on a slide by their names, when the selection
 * changes. Ranges of cells ("A1 to C4 selected") are the grid's own announcement, from the core.
 */
import { formatList, t } from '@kiln/i18n';
import type { DocId, KilnClient, SelKind } from '@kiln/protocol';
import { useEffect } from 'react';
import { announce } from './announce.ts';

const MAX_TEXT = 160;

/** A key that changes exactly when what is selected changes. */
export function selectionKey(kind: SelKind): string {
  if (kind === 'None') return 'none';
  if ('Text' in kind) {
    const { anchor, head } = kind.Text;
    return `t:${anchor.block}:${anchor.offset}:${head.block}:${head.offset}`;
  }
  if ('Cells' in kind)
    return `c:${kind.Cells.ranges.map((r) => `${r.start.row},${r.start.col},${r.end.row},${r.end.col}`).join(';')}`;
  return `s:${kind.Shapes.ids.join(',')}`;
}

/** Selected text, shortened, as read out. */
export function textAnnouncement(text: string): string {
  const s = text.replace(/\s+/g, ' ').trim();
  if (!s) return '';
  return t('{text}, selected', { text: s.length > MAX_TEXT ? `${s.slice(0, MAX_TEXT)}…` : s });
}

/** Reads selection changes of a document window (shapes by their names in the slide's tree). */
export function useSelectionAnnouncements(client: KilnClient | null, doc: DocId | null): void {
  useEffect(() => {
    if (!client || doc === null) return;
    let last = '';
    let seq = 0;
    return client.subscribe((e) => {
      if (typeof e !== 'object' || e === null || !('Changed' in e) || e.Changed.summary.doc !== doc) return;
      const kind = e.Changed.selection.kind;
      const key = selectionKey(kind);
      if (key === last) return;
      const first = last === '';
      last = key;
      if (first || kind === 'None') return;
      const mine = ++seq;
      // Cells: the grid's own tree says the range and the active cell (the core names them).
      if ('Cells' in kind) return;
      if ('Text' in kind) {
        const { anchor, head } = kind.Text;
        if (anchor.block === head.block && anchor.offset === head.offset) return;
        void client.query(doc, 'SelectedText').then(
          (r) => {
            if (mine === seq && typeof r === 'object' && r !== null && 'Text' in r)
              announce(textAnnouncement(r.Text));
          },
          () => undefined,
        );
        return;
      }
      if (kind.Shapes.ids.length === 0) return;
      const ids = new Set(kind.Shapes.ids);
      void client.a11y(doc).then(
        (tree) => {
          if (mine !== seq) return;
          const names = tree.nodes.filter((n) => ids.has(n.id)).map((n) => n.name ?? t('Shape'));
          if (names.length) announce(t('{names} selected', { names: formatList(names) }));
        },
        () => undefined,
      );
    });
  }, [client, doc]);
}
