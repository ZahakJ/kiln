/**
 * Document Recovery speaks in written words: every way it can fail is forced here, through the
 * events and refusals the core gives, and no text of the core's reaches the page.
 */
import type { DocSummary, KilnClient, Event as KilnEvent, RecoveryEntry } from '@kiln/protocol';
import { KilnError } from '@kiln/protocol';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useStorage } from './storage.tsx';

vi.setConfig({ testTimeout: 20_000 });
afterEach(cleanup);

/** Text the core writes that must never show. */
const RAW = [
  'Document Recovery:',
  'kiln-journal-',
  'QuotaExceededError',
  'the marker',
  'journal format',
  'not found: the sheet',
  'planted',
  'locked by window',
];

function fakeClient(refuse: Record<string, KilnError>) {
  const listeners = new Set<(e: KilnEvent) => void>();
  const client = {
    bridge: 'wasm',
    subscribe(l: (e: KilnEvent) => void) {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    execute: vi.fn(async (_doc: number | null, cmd: unknown) => {
      const name = JSON.stringify(cmd);
      for (const [key, error] of Object.entries(refuse)) if (name.includes(key)) throw error;
      return {};
    }),
    query: vi.fn(async () => ({})),
  };
  const emit = (e: KilnEvent) => {
    for (const l of listeners) l(e);
  };
  return { client: client as unknown as KilnClient, emit, execute: client.execute };
}

const summary = {
  doc: 7,
  kind: 'Grid',
  app: 'Grid',
  title: 'Budget',
  path: null,
  format: null,
  revision: 1,
  dirty: true,
  read_only: false,
  can_undo: false,
  can_redo: false,
  undo_label: null,
  redo_label: null,
} as unknown as DocSummary;

function Frame({ client }: { client: KilnClient }) {
  const s = useStorage({ client, summary, appName: 'Kiln Grid' });
  return (
    <div className="kiln-appframe__canvas">
      {s.pane}
      {s.messageBar}
    </div>
  );
}

/** Text without its bidi isolation marks. */
const plain = (s: string) => s.replace(/[\u2068\u2069]/g, '');

function noRawText() {
  const text = document.body.textContent ?? '';
  for (const raw of RAW) expect(text).not.toContain(raw);
}

const error = (kind: KilnError['kind'], message: string) => new KilnError({ kind, message });

describe('Document Recovery failures', () => {
  it('says each in written words', async () => {
    const entry: RecoveryEntry = {
      key: 'kiln-journal-42',
      kind: 'Grid',
      title: 'Budget',
      path: null,
      format: null,
      saved_at_ms: 0,
      problem: null,
    };
    const { client, emit } = fakeClient({
      RecoveryOpen: error('Format', 'autosave journal: the marker: expected value'),
      RecoveryDelete: error('Io', 'kiln-journal-42 is locked by window 3'),
      PutBackDraft: error('NotFound', 'not found: the sheet the text was typed on'),
    });
    render(<Frame client={client} />);
    const click = async (el: Element) => {
      await act(async () => {
        fireEvent.click(el);
        await Promise.resolve();
      });
    };
    const notice = (kind: string) =>
      plain(document.querySelector(`[data-notice="${kind}"]`)?.textContent ?? '');

    // The core cannot open or delete a copy (another window has it; it is gone).
    act(() =>
      emit({
        Error: {
          doc: null,
          error: { kind: 'Unavailable', message: 'Document Recovery: locked by window 3' },
        },
      }),
    );
    expect(notice('recovery')).toMatch(/could not open that recovery copy/);
    act(() =>
      emit({
        Error: { doc: null, error: { kind: 'NotFound', message: 'Document Recovery: kiln-journal-42' } },
      }),
    );
    expect(notice('recovery')).toMatch(/no longer here/);
    noRawText();

    // The document's recovery copy, and the app storage, cannot be written.
    act(() =>
      emit({
        Error: {
          doc: 7,
          error: { kind: 'Io', message: 'Kiln could not keep a recovery copy of Budget: QuotaExceededError' },
        },
      }),
    );
    expect(notice('journal')).toMatch(/could not keep a recovery copy of Budget\. Save the document/);
    act(() =>
      emit({
        Error: {
          doc: null,
          error: { kind: 'Io', message: 'Kiln could not write its app storage: QuotaExceededError' },
        },
      }),
    );
    expect(notice('storage')).toMatch(/could not write its recovery copies/);
    noRawText();

    // Open and Delete in the pane, refused.
    act(() => emit({ Recovery: { entries: [entry] } }));
    await click(await screen.findByTestId('recovery-open'));
    expect(notice('recovery')).toMatch(/damaged/);
    await click(screen.getByTestId('recovery-delete'));
    expect(notice('recovery')).toMatch(/could not delete that recovery copy/);
    noRawText();

    // A change that did not replay; text being typed, put back onto a sheet deleted since.
    act(() =>
      emit({
        Recovered: {
          doc: 7,
          note: {
            stopped: { command: 'journal.unrepeatable', after: 3, at_ms: 0 },
            draft: { cell: "'A1'!E7", text: '=SUM(B2:B9)', sheet: 4, row: 6, col: 4 },
          },
        },
      }),
    );
    expect(notice('recovered-stop')).toMatch(/before a change Kiln could not repeat.*3 changes after it/);
    expect(notice('recovered-draft')).toMatch(/“=SUM\(B2:B9\)” was being typed in 'A1'!E7/);
    await click(screen.getByRole('button', { name: 'Put it back' }));
    expect(notice('recovered-draft')).toMatch(/'A1'!E7 is no longer in this workbook: its sheet was deleted/);
    // The text stays, whole, to select and copy.
    expect(screen.getByTestId('storage-notice-text').textContent).toBe('=SUM(B2:B9)');
    // The reference and the text are isolated, so a right-to-left sentence keeps their order.
    expect(document.querySelector('[data-notice="recovered-draft"]')?.textContent).toContain(
      "\u2068'A1'!E7\u2069",
    );
    // Copy says what it did, and when it could not.
    const writes: string[] = [];
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: async (t: string) => void writes.push(t) },
    });
    await click(screen.getByRole('button', { name: 'Copy' }));
    expect(writes).toEqual(['=SUM(B2:B9)']);
    expect(screen.getByTestId('storage-notice-copy-status').textContent).toBe('Copied.');
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: async () => Promise.reject(new Error('NotAllowedError: denied')) },
    });
    await click(screen.getByRole('button', { name: 'Copied' }));
    await act(async () => Promise.resolve());
    expect(screen.getByTestId('storage-notice-copy-status').textContent).toBe(
      'Kiln could not copy the text. Select it and press Ctrl+C.',
    );
    expect(document.body.textContent).not.toContain('NotAllowedError');
    noRawText();
  });

  it('puts the text back by the sheet id, and words any other refusal', async () => {
    const { client, emit, execute } = fakeClient({
      PutBackDraft: error('Unavailable', 'grid.editing.put_back_draft is not available: protected'),
    });
    render(<Frame client={client} />);
    act(() =>
      emit({
        Recovered: {
          doc: 7,
          note: { stopped: null, draft: { cell: 'Plan!B2', text: 'x', sheet: 9, row: 1, col: 1 } },
        },
      }),
    );
    const put = screen.getByRole('button', { name: 'Put it back' });
    await act(async () => {
      fireEvent.click(put);
      await Promise.resolve();
    });
    expect(execute).toHaveBeenCalledWith(7, {
      Grid: { PutBackDraft: { sheet: 9, row: 1, col: 1, text: 'x' } },
    });
    expect(plain(document.querySelector('[data-notice="recovered-draft"]')?.textContent ?? '')).toMatch(
      /could not put the text back into Plan!B2/,
    );
    expect(document.body.textContent).not.toContain('protected');
  });
});
