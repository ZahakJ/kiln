/**
 * The slide show (`#/present`): the core's show of a presentation full screen
 * (`?doc=3`), in the browser tab (`&tab=1`, Present in Browser Tab and "Browsed by an individual"),
 * as Presenter View in this window with the show in a second one (`&view=presenter&channel=…`),
 * or as that second window, which opens the presentation from the presenter's window
 * (`?view=audience&channel=…`). Esc or the end of the show goes back to the editor.
 *
 * On the desktop both windows talk to the one native core: the editor asks the shell for
 * the audience's window (`suite.view.open_window`, `#/present?doc=3&view=audience`) and goes to
 * Presenter View here (`&view=presenter` without a channel); each window shows the core's show
 * and follows `Event::Show`, so a step taken in either window shows in both.
 *
 * The show's clock is the page's; `&clock=manual` in development builds stops it and exposes it
 * as `window.__kilnShowClock` so tests can show any moment of a transition.
 */

import { buildCommand, createDefaultKeymap, type Keymap } from '@kiln/commands';
import {
  deckResult,
  ManualClock,
  PresenterView,
  type RelayMessage,
  realClock,
  relays,
  type ShowClock,
  type ShowEngine,
  ShowRelay,
  ShowView,
  unlockOrientation,
} from '@kiln/editor-deck';
import { t } from '@kiln/i18n';
import { type DeckShowStatus, type DocId, isTauri, type KilnClient } from '@kiln/protocol';
import { Button, Dialog, DialogContent } from '@kiln/ui-kit';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useClient } from '../../client/useClient.ts';
import { closeThisWindow } from '../../desktop.ts';
import './present.css';

declare global {
  interface Window {
    /** Development builds with `&clock=manual`: the show's clock (e2e tests). */
    __kilnShowClock?: ManualClock;
    /** Development builds: the show's engine (e2e tests read its frames). */
    __kilnShowEngine?: ShowEngine | null;
  }
}

import type { PresentParams } from '../params.ts';

export { type PresentParams, parsePresent } from '../params.ts';

/** The editor's hash for a document. */
const editorHash = (doc: number | null) => (doc ? `#/deck?doc=${doc}` : '#/home');

function leave(doc: number | null): void {
  // A phone turned to landscape for the show turns back.
  unlockOrientation();
  if (typeof document !== 'undefined' && document.fullscreenElement)
    void document.exitFullscreen().catch(() => undefined);
  // Development builds keep a stopped clock for the next show (tests).
  const manual = import.meta.env.DEV && /[?&]clock=manual\b/.test(window.location.hash);
  window.location.hash = `${editorHash(doc)}${manual && doc ? '&clock=manual' : ''}`;
}

/** The page's one manual clock (a module value: development renders run memos twice). */
let manualClock: ManualClock | null = null;

function useShowClock(manual: boolean): ShowClock {
  if (!(manual && import.meta.env.DEV)) return realClock;
  manualClock ??= new ManualClock(0);
  window.__kilnShowClock = manualClock;
  return manualClock;
}

/** Runs a show command in this window's core. */
function exec(client: KilnClient, doc: DocId, id: string, args?: unknown) {
  return client.execute(doc, buildCommand(id, args, { app: 'deck' }));
}

export function PresentRoute({ params }: { params: PresentParams }) {
  const state = useClient('deck');
  const keymap = useMemo(() => createDefaultKeymap(), []);
  const clock = useShowClock(params.manualClock);
  if (state.status === 'error')
    return (
      <div className="kiln-present-message" role="alert">
        {t('The slide show could not start: {message}', { message: state.error.message })}
      </div>
    );
  if (state.status !== 'ready') return <div className="kiln-present-black" />;
  if (params.view === 'audience' && params.doc !== null && !params.channel && isTauri())
    return <NativeAudience client={state.client} doc={params.doc} keymap={keymap} clock={clock} />;
  if (params.view === 'audience')
    return <Audience client={state.client} channel={params.channel} keymap={keymap} clock={clock} />;
  if (params.doc === null) {
    leave(null);
    return null;
  }
  if (params.view === 'presenter' && params.channel)
    return (
      <Presenter
        client={state.client}
        doc={params.doc}
        channel={params.channel}
        keymap={keymap}
        clock={clock}
      />
    );
  if (params.view === 'presenter' && isTauri())
    return <NativePresenter client={state.client} doc={params.doc} keymap={keymap} clock={clock} />;
  return <Show client={state.client} doc={params.doc} keymap={keymap} clock={clock} />;
}

/**
 * Desktop: a step of the show taken in the other window reaches this one through the core's
 * `Event::Show`; the engine draws the new state at once.
 */
function useFollowShow(client: KilnClient, doc: DocId, engine: { current: ShowEngine | null }) {
  useEffect(
    () =>
      client.subscribe((e) => {
        if (typeof e === 'object' && 'Show' in e && e.Show.doc === doc) engine.current?.kick();
      }),
    [client, doc, engine],
  );
}

/** The window's title in the interface language. */
function useWindowTitle(title: string) {
  useEffect(() => {
    document.title = title;
  }, [title]);
}

/** Desktop: Presenter View here, the show in the audience's window, one core for both. */
function NativePresenter({
  client,
  doc,
  keymap,
  clock,
}: {
  client: KilnClient;
  doc: DocId;
  keymap: Keymap;
  clock: ShowClock;
}) {
  const engine = useRef<ShowEngine | null>(null);
  useFollowShow(client, doc, engine);
  useWindowTitle(`${t('Presenter View')} – Kiln Deck`);
  const send = useCallback((id: string, args?: unknown) => exec(client, doc, id, args), [client, doc]);
  return (
    <PresenterView
      client={client}
      doc={doc}
      keymap={keymap}
      clock={clock}
      send={send}
      audience="connected"
      onEngine={(e) => {
        engine.current = e;
        onEngine(e);
      }}
      onEnd={() => leave(doc)}
    />
  );
}

/** Desktop: the audience's window, the show of the presenter's document; it closes at the end. */
function NativeAudience({
  client,
  doc,
  keymap,
  clock,
}: {
  client: KilnClient;
  doc: DocId;
  keymap: Keymap;
  clock: ShowClock;
}) {
  const engine = useRef<ShowEngine | null>(null);
  useFollowShow(client, doc, engine);
  useWindowTitle(`${t('Slide Show')} – Kiln Deck`);
  const send = useCallback((id: string, args?: unknown) => exec(client, doc, id, args), [client, doc]);
  return (
    <ShowView
      client={client}
      doc={doc}
      keymap={keymap}
      clock={clock}
      send={send}
      onEngine={(e) => {
        engine.current = e;
      }}
      onEnd={() => closeThisWindow()}
    />
  );
}

function onEngine(e: ShowEngine | null) {
  if (import.meta.env.DEV) window.__kilnShowEngine = e;
}

/** The show in this window. */
function Show({
  client,
  doc,
  keymap,
  clock,
}: {
  client: KilnClient;
  doc: DocId;
  keymap: Keymap;
  clock: ShowClock;
}) {
  const [ended, setEnded] = useState<DeckShowStatus | null>(null);
  const send = useCallback((id: string, args?: unknown) => exec(client, doc, id, args), [client, doc]);
  const [mode, setMode] = useState<DeckShowStatus['mode']>('Show');
  if (ended?.mode === 'Rehearse') return <KeepTimings client={client} doc={doc} />;
  return (
    <ShowView
      client={client}
      doc={doc}
      keymap={keymap}
      clock={clock}
      send={send}
      onEngine={onEngine}
      onStatus={(s) => setMode(s.mode)}
      onEnd={(s) => {
        if (s.mode === 'Rehearse' || mode === 'Rehearse') setEnded({ ...s, mode: 'Rehearse' });
        else leave(doc);
      }}
    >
      {mode === 'Rehearse' ? <RehearseBar client={client} doc={doc} send={send} /> : null}
    </ShowView>
  );
}

/** Rehearse Timings: the slide's time and the total, with Next and Pause as PowerPoint has them. */
function RehearseBar({
  client,
  doc,
  send,
}: {
  client: KilnClient;
  doc: DocId;
  send: (id: string, args?: unknown) => Promise<unknown>;
}) {
  const [position, setPosition] = useState(1);
  const started = useRef(performance.now());
  const slideStart = useRef(performance.now());
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => {
      tick((x) => x + 1);
      void client
        .query(doc, { Deck: 'ShowInfo' })
        .then((r) => {
          const d = deckResult(r);
          if (!d || !('ShowInfo' in d)) return;
          if (d.ShowInfo.status.position !== position) {
            setPosition(d.ShowInfo.status.position);
            slideStart.current = performance.now();
          }
        })
        .catch(() => undefined);
    }, 250);
    return () => clearInterval(t);
  }, [client, doc, position]);
  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
  };
  return (
    <div className="kiln-show__rehearse" role="timer" aria-label={t('Rehearsal')} data-testid="deck-rehearse">
      <span>{t('Recording')}</span>
      <span>
        <span className="kiln-show__sr">{t('This slide')} </span>
        {fmt(performance.now() - slideStart.current)}
      </span>
      <span>
        <span className="kiln-show__sr">{t('Total')} </span>
        {fmt(performance.now() - started.current)}
      </span>
      <button type="button" className="kiln-show__btn" onClick={() => void send('deck.show.next')}>
        {t('Next')}
      </button>
    </div>
  );
}

/** After a rehearsal: keep the new slide timings? */
function KeepTimings({ client, doc }: { client: KilnClient; doc: DocId }) {
  const done = (keep: boolean) => {
    const go = () => leave(doc);
    if (keep) void exec(client, doc, 'deck.show.keep_timings').then(go, go);
    else go();
  };
  return (
    <div className="kiln-present-black">
      <Dialog open onOpenChange={(o) => (o ? undefined : done(false))}>
        <DialogContent
          title={t('Rehearse Timings')}
          size="sm"
          data-testid="deck-keep-timings"
          data-kiln-shortcuts="off"
          footer={
            <>
              <Button onClick={() => done(false)}>{t('No')}</Button>
              <Button variant="primary" onClick={() => done(true)}>
                {t('Yes')}
              </Button>
            </>
          }
        >
          <p>{t('Do you want to keep the new slide timings to use when you show the presentation?')}</p>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Presenter View here; the show in the window the editor opened, kept on the same step. */
function Presenter({
  client,
  doc,
  channel,
  keymap,
  clock,
}: {
  client: KilnClient;
  doc: DocId;
  channel: string;
  keymap: Keymap;
  clock: ShowClock;
}) {
  // The relay lives with the effect (development renders run effects twice).
  const relayRef = useRef<ShowRelay | null>(null);
  const engine = useRef<ShowEngine | null>(null);
  /** The number of the last show command sent to this window's core. */
  const seq = useRef(0);
  const [audience, setAudience] = useState<'waiting' | 'connected' | 'none'>(
    typeof BroadcastChannel === 'undefined' ? 'none' : 'waiting',
  );
  useEffect(() => {
    const relay = new ShowRelay(channel);
    relayRef.current = relay;
    const off = relay.subscribe((m: RelayMessage) => {
      if (m.kind === 'hello') {
        // The core answers in order: the snapshot is the show after every command sent so far.
        const at = seq.current;
        void Promise.all([
          client.query(doc, { Deck: 'Snapshot' }),
          client.query(doc, { Deck: 'ShowInfo' }),
        ]).then(([snap, info]) => {
          const s = deckResult(snap);
          const i = deckResult(info);
          if (!s || !('Bytes' in s) || !i || !('ShowInfo' in i)) return;
          relay.post({
            kind: 'snapshot',
            bytes: Uint8Array.from(s.Bytes),
            name: 'Presentation.pptx',
            slide: i.ShowInfo.status.slide,
            click: i.ShowInfo.status.click,
            screen: i.ShowInfo.status.screen,
            tool: i.ShowInfo.status.tool,
            seq: at,
          });
          setAudience('connected');
        });
      } else if (m.kind === 'command' && m.seq === undefined) {
        // The audience's key or click: done there, numbered here.
        seq.current += 1;
        const n = seq.current;
        void exec(client, doc, m.id, m.args)
          .catch(() => undefined)
          .finally(() => {
            relay.post({ kind: 'command', id: m.id, args: m.args, seq: n, echo: true });
            engine.current?.kick();
          });
      } else if (m.kind === 'end') {
        // The audience ended the show (its End Show, or its last click): end it here too.
        void exec(client, doc, 'deck.show.end')
          .catch(() => undefined)
          .finally(() => engine.current?.kick());
      }
    });
    return () => {
      off();
      relay.close();
      if (relayRef.current === relay) relayRef.current = null;
    };
  }, [channel, client, doc]);
  const send = useCallback(
    async (id: string, args?: unknown) => {
      if (!relays(id)) return exec(client, doc, id, args);
      seq.current += 1;
      const n = seq.current;
      try {
        return await exec(client, doc, id, args);
      } finally {
        relayRef.current?.post({ kind: 'command', id, args, seq: n });
      }
    },
    [client, doc],
  );
  return (
    <PresenterView
      client={client}
      doc={doc}
      keymap={keymap}
      clock={clock}
      send={send}
      audience={audience}
      onEngine={(e) => {
        engine.current = e;
        onEngine(e);
      }}
      onEnd={() => {
        relayRef.current?.post({ kind: 'end' });
        leave(doc);
      }}
    />
  );
}

/** The command that picks each pointer tool but the arrow (a snapshot's tool). */
const TOOLS: Partial<Record<DeckShowStatus['tool'], string>> = {
  Laser: 'deck.show.laser',
  Pen: 'deck.show.pen',
  Highlighter: 'deck.show.highlighter',
  Eraser: 'deck.show.eraser',
};

/**
 * The audience's window: opens the presenter's presentation and follows the presenter. Commands
 * that come while it opens wait and are done after it, those the snapshot already has are
 * skipped.
 */
function Audience({
  client,
  channel,
  keymap,
  clock,
}: {
  client: KilnClient;
  channel: string | null;
  keymap: Keymap;
  clock: ShowClock;
}) {
  const relayRef = useRef<ShowRelay | null>(null);
  const [doc, setDoc] = useState<DocId | null>(null);
  const [over, setOver] = useState(false);
  const engine = useRef<ShowEngine | null>(null);
  const docRef = useRef<DocId | null>(null);
  useEffect(() => {
    if (!channel) return;
    const relay = new ShowRelay(channel);
    relayRef.current = relay;
    /** Opening a snapshot; a snapshot is wanted (at the start, or after a missed command). */
    let opening = false;
    let wanted = true;
    /** The presenter's number of the last command this window's show reflects. */
    let last = 0;
    /** The presenter's commands not yet done here (all of them until a snapshot is open). */
    const queue: Extract<RelayMessage, { kind: 'command' }>[] = [];
    let draining = false;
    const drain = async () => {
      if (draining) return;
      draining = true;
      try {
        while (queue.length > 0 && docRef.current !== null && !wanted) {
          const m = queue.shift();
          if (!m || m.seq === undefined || m.seq <= last) continue;
          if (m.seq !== last + 1) {
            // A command went missing: start again from a fresh snapshot.
            wanted = true;
            relay.post({ kind: 'hello' });
            break;
          }
          last = m.seq;
          if (!m.echo) await exec(client, docRef.current, m.id, m.args).catch(() => undefined);
          engine.current?.kick();
        }
      } finally {
        draining = false;
      }
    };
    const hello = setInterval(() => {
      if (wanted && !opening) relay.post({ kind: 'hello' });
    }, 400);
    relay.post({ kind: 'hello' });
    const off = relay.subscribe((m: RelayMessage) => {
      if (m.kind === 'snapshot' && wanted && !opening) {
        opening = true;
        void (async () => {
          const summary = await client.open(m.bytes, {
            name: m.name,
            path: null,
            format: 'Pptx',
            read_only: true,
          });
          const id = summary.doc;
          await exec(client, id, 'deck.show.start', { from: m.slide, mode: 'Show' });
          for (let k = 0; k < m.click; k++) await exec(client, id, 'deck.show.next');
          if (m.screen === 'Black') await exec(client, id, 'deck.show.black');
          else if (m.screen === 'White') await exec(client, id, 'deck.show.white');
          const tool = TOOLS[m.tool];
          if (tool) await exec(client, id, tool);
          const old = docRef.current;
          docRef.current = id;
          last = m.seq;
          wanted = false;
          opening = false;
          setDoc(id);
          // The old copy goes once the show has moved to the new one.
          if (old !== null)
            setTimeout(
              () => void client.execute(old, { Suite: 'CloseWithoutSaving' }).catch(() => undefined),
              1000,
            );
          await drain();
        })().catch(() => setOver(true));
      } else if (m.kind === 'command' && m.seq !== undefined) {
        queue.push(m);
        void drain();
      } else if (m.kind === 'end') {
        setOver(true);
        window.close();
      }
    });
    return () => {
      clearInterval(hello);
      off();
      relay.close();
      if (relayRef.current === relay) relayRef.current = null;
    };
  }, [channel, client]);
  const send = useCallback(
    async (id: string, args?: unknown) => {
      const d = docRef.current;
      if (d === null) return undefined;
      const r = await exec(client, d, id, args);
      if (relays(id)) relayRef.current?.post({ kind: 'command', id, args });
      return r;
    },
    [client],
  );
  if (over || !channel)
    return (
      <div className="kiln-present-message" data-testid="deck-audience-over">
        {t('The slide show has ended. You can close this window.')}
      </div>
    );
  if (doc === null) return <div className="kiln-present-black" data-testid="deck-audience-waiting" />;
  return (
    <ShowView
      client={client}
      doc={doc}
      keymap={keymap}
      clock={clock}
      send={send}
      onEngine={(e) => {
        engine.current = e;
        onEngine(e);
      }}
      onEnd={() => {
        relayRef.current?.post({ kind: 'end' });
        setOver(true);
        window.close();
      }}
    />
  );
}
