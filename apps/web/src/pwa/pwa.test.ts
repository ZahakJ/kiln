import type { KilnClient, Event as KilnEvent } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { unsavedDocuments, watchDocuments } from '../client/unsaved.ts';
import { pwaState, remember, remembered, setPwaState, subscribePwa } from './state.ts';

/** A client that only delivers the events a test sends. */
function fakeClient(): { client: KilnClient; send: (event: KilnEvent) => void } {
  const listeners = new Set<(event: KilnEvent) => void>();
  const client = {
    subscribe(listener: (event: KilnEvent) => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  } as unknown as KilnClient;
  return {
    client,
    send: (event) => {
      for (const l of listeners) l(event);
    },
  };
}

const summary = (doc: number, dirty: boolean) => ({ doc, dirty, path: null, title: 'Document1' }) as never;

describe('the installable web app', () => {
  it('knows which documents have unsaved changes from the cores events', () => {
    const { client, send } = fakeClient();
    const stop = watchDocuments(client);
    expect(unsavedDocuments()).toEqual([]);
    send({ Opened: { summary: summary(2, false) } });
    send({ Changed: { summary: summary(2, true), selection: null as never, status: null as never } });
    expect(unsavedDocuments()).toEqual([{ doc: 2, name: 'Document1' }]);
    send({ Saved: { summary: summary(2, false) } });
    expect(unsavedDocuments()).toEqual([]);
    send({ Changed: { summary: summary(6, true), selection: null as never, status: null as never } });
    send({ Closed: { doc: 6 } });
    expect(unsavedDocuments()).toEqual([]);
    stop();
  });

  it('notifies the notices of each change and keeps the user choices', () => {
    let calls = 0;
    const stop = subscribePwa(() => calls++);
    setPwaState({ update: true });
    expect(pwaState().update).toBe(true);
    expect(calls).toBe(1);
    stop();
    setPwaState({ update: false });
    expect(calls).toBe(1);
    remember('install-declined', '1');
    expect(remembered('install-declined')).toBe('1');
    expect(localStorage.getItem('kiln.pwa.install-declined')).toBe('1');
  });
});
