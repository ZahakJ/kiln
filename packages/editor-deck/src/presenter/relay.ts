/**
 * Presenter View on the web: the presenter's window and the audience's window each run the show
 * in their own core, and a `BroadcastChannel` relays what the presenter does (every show command)
 * so both stay on the same step. The audience window starts by asking for the presentation; the
 * presenter's window answers with it as `.pptx` bytes (`Query::Deck(Snapshot)`) and where the show
 * is. The presenter's window numbers every show command it does (its own and the audience's): the
 * snapshot says which number it reflects, so the audience replays the commands after it and asks
 * for a fresh snapshot when one is missing. On the desktop both windows share one core and follow
 * `Event::Show` instead.
 */

import type { DeckShowScreen, DeckShowTool } from '@kiln/protocol';

/** A message between the two windows of a show. */
export type RelayMessage =
  | { kind: 'hello' }
  | {
      kind: 'snapshot';
      bytes: Uint8Array;
      name: string;
      /** The slide (index) and clicks done on it. */
      slide: number;
      click: number;
      /** Black or white screen, and the pointer tool. */
      screen: DeckShowScreen;
      tool: DeckShowTool;
      /** The number of the last command the snapshot reflects. */
      seq: number;
    }
  | {
      kind: 'command';
      id: string;
      args?: unknown;
      /** The presenter's number of the command (none on the audience's). */
      seq?: number;
      /** The audience's own command, numbered by the presenter: done there already. */
      echo?: boolean;
    }
  | { kind: 'end' };

/** The ids of the show's commands that are relayed (the rest stay in their window). */
export function relays(id: string): boolean {
  return id.startsWith('deck.show.') && id !== 'deck.show.settings' && id !== 'deck.show.keep_timings';
}

/** A fresh channel name for a show. */
export function newChannelName(): string {
  const rnd =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID()
      : `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`;
  return `kiln-show-${rnd}`;
}

/** One end of the relay. */
export class ShowRelay {
  private readonly channel: BroadcastChannel | null;
  private readonly listeners = new Set<(m: RelayMessage) => void>();

  constructor(readonly name: string) {
    this.channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(name);
    if (this.channel) {
      this.channel.onmessage = (e: MessageEvent) => {
        const m = e.data as RelayMessage;
        if (!m || typeof m !== 'object' || typeof (m as { kind?: unknown }).kind !== 'string') return;
        for (const l of [...this.listeners]) l(m);
      };
    }
  }

  /** Whether the browser relays at all. */
  get available(): boolean {
    return this.channel !== null;
  }

  post(m: RelayMessage): void {
    this.channel?.postMessage(m);
  }

  subscribe(fn: (m: RelayMessage) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  close(): void {
    this.listeners.clear();
    this.channel?.close();
  }
}
