import type { CommandRegistry } from '@kiln/commands';
import type { DocId, GridState, KilnClient } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { GridEditorModel } from './model';

/** A core whose entry holds `=SUM(` until a command changes it to `=SUM(Amounts`. */
function model() {
  let text = '=SUM(';
  const client = {
    subscribe: () => () => undefined,
    execute: () => {
      text = '=SUM(Amounts';
      return Promise.resolve(null);
    },
    query: () =>
      Promise.resolve({
        Grid: {
          State: {
            layout: { content_w: 1000, content_h: 1000, header_w: 40, header_h: 20 },
            mode: 'Edit',
            editor: { text, start: text.length, end: text.length, formula_bar: false },
          } as unknown as GridState,
        },
      }),
  } as unknown as KilnClient;
  const registry = { setContext: () => undefined } as unknown as CommandRegistry;
  return new GridEditorModel({ client, doc: 1 as unknown as DocId, registry });
}

describe('editing from a dialog', () => {
  it('the surface has the new text before the dialog gives the keys back', async () => {
    const m = model();
    const seen: string[] = [];
    m.onState((s) => seen.push(s.editor?.text ?? ''));
    // Paste Names: the next key typed in the cell must add to the pasted name, not to `=SUM(`.
    await m.edit('grid.editing.set_text', { text: '=SUM(Amounts', start: 12, end: 12 });
    expect(seen.at(-1)).toBe('=SUM(Amounts');
  });
});
