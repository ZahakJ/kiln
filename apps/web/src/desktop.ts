/**
 * The page's side of the desktop shell, in Tauri only: tells the shell what the window
 * shows (its app, document, unsaved changes, the interface language and theme), so the window
 * has the menu bar of its app in the right language, runs the commands picked in that menu bar,
 * closes a document before its window, reports Kiln Home's first paint (`--startup-probe`) and,
 * where the shell does not see files dropped on a window itself (Windows), opens them. On Android
 * it gives the system bars the title bar's colour and handles Back: the topmost sheet,
 * dialog, menu or Backstage closes first, then the document, and on Kiln Home Kiln goes to the
 * background. The web build takes the same steps for the browser's Back (history) and the phone's
 * app bar Back: `goBack` and `useWebBack`.
 */

import type { CommandRegistry } from '@kiln/commands';
import { language, onLanguageChange } from '@kiln/i18n';
import { type DocSummary, type Event, isTauri, type KilnClient, tauriShell } from '@kiln/protocol';
import { useEffect, useRef } from 'react';

type Shell = Awaited<ReturnType<typeof tauriShell>>;

let shell: Promise<Shell> | null = null;
const getShell = (): Promise<Shell> => {
  shell ??= tauriShell();
  return shell;
};

/** What this window reports (the app comes from the route, the document from the frame). */
const state = { app: 'home', doc: null as number | null, dirty: false };
let reportQueued = false;
let watching = false;

function theme(): 'light' | 'dark' {
  const t = document.documentElement.getAttribute('data-theme');
  if (t === 'light') return 'light';
  if (t === 'dark' || t === 'hc') return 'dark';
  return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/** The title bar's colour: the page's `theme-color`, which the theme sets from the tokens. */
function barColor(): string | null {
  return document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')?.content || null;
}

function report(): void {
  if (!isTauri() || reportQueued) return;
  reportQueued = true;
  queueMicrotask(() => {
    reportQueued = false;
    void getShell()
      .then((s) => s.reportWindow({ ...state, language: language(), theme: theme(), bar: barColor() }))
      .catch(() => undefined);
  });
  if (!watching) {
    watching = true;
    onLanguageChange(() => report());
    new MutationObserver(() => report()).observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
  }
}

/** Kiln on Android (the Tauri app in Android's WebView). */
export function isAndroid(): boolean {
  return isTauri() && /Android/i.test(navigator.userAgent);
}

/** What Back closes before anything else: the open sheets, dialogs, menus and popovers. */
const LAYERS =
  '[data-kiln-back-layer], [role="dialog"][data-state="open"], [role="alertdialog"][data-state="open"], [role="menu"][data-state="open"], [data-radix-popper-content-wrapper]';

/** The document frame's part of Back: close the document (asking about unsaved changes). */
let backFrame: (() => void) | null = null;
/** The layer Back last asked to close: one that stays open is passed over the next time. */
let lastEscaped: Element | null = null;
let backInstalled = false;

/**
 * What one press of Back did: closed the topmost layer (a sheet, dialog, menu or Backstage, or
 * ended a slide show), asked the document to close, went to Kiln Home, or found nothing left to
 * close (on Kiln Home).
 */
export type BackStep = 'layer' | 'document' | 'home' | 'none';

/**
 * One press of Back, the same for Android's Back button, the browser's Back and the phone's app
 * bar: the topmost layer closes first (Escape, as with a keyboard), then the document (through
 * `suite.file.close`, which asks about unsaved changes), then Kiln Home shows.
 */
export function backStep(): BackStep {
  const layers = [...document.querySelectorAll(LAYERS)].filter((l) => l !== lastEscaped);
  const top = layers.at(-1);
  if (top || state.app === 'present') {
    // A screen of an editor's own (Deck's slide on a phone, over its slide list) goes back itself.
    if (top?.hasAttribute('data-kiln-back-layer')) {
      lastEscaped = null;
      top.dispatchEvent(new CustomEvent('kiln:back'));
      return 'layer';
    }
    lastEscaped = top ?? null;
    // Escape closes it, as it does with a keyboard (and ends a slide show).
    const target = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
    target.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }),
    );
    return 'layer';
  }
  lastEscaped = null;
  if (backFrame) {
    backFrame();
    return 'document';
  }
  if (state.app !== 'home') {
    window.location.hash = '#/home';
    return 'home';
  }
  return 'none';
}

function back(s: Shell): void {
  if (backStep() === 'none') void s.leaveApp().catch(() => undefined);
}

function installBack(): void {
  if (backInstalled || !isAndroid()) return;
  backInstalled = true;
  void getShell()
    .then((s) => s.onBack(() => back(s)))
    .catch(() => {
      backInstalled = false;
    });
}

/** The history entry the web build keeps over a document's own, so Back stays on the page. */
const GUARD = 'kilnBack';
/** The mark of a document's own history entry (the one under its guard). */
const OWN = 'kilnDocEntry';
/** Where the web's Back goes once the document it closed has gone. */
let afterClose: (() => void) | null = null;

function guarded(): boolean {
  const st: unknown = window.history.state;
  return typeof st === 'object' && st !== null && GUARD in st;
}

function pushGuard(): void {
  const st: unknown = window.history.state;
  window.history.pushState(
    { ...(typeof st === 'object' && st ? st : {}), [GUARD]: true },
    '',
    window.location.href,
  );
}

/** Leaves the document's history entries: back past them, or to Kiln Home when there is nothing before. */
function leaveDocument(): void {
  const href = window.location.href;
  window.history.go(guarded() ? -2 : -1);
  // Opened straight from a link: there is no page of Kiln before it.
  setTimeout(() => {
    if (window.location.href === href) window.location.replace('#/home');
  }, 400);
}

/**
 * The app bar's Back on a phone (and anywhere else the web build offers Back): one `backStep`;
 * when it closed the document, Kiln Home shows once the document has gone.
 */
export function goBack(): void {
  const step = backStep();
  if (step === 'document' && !isTauri()) afterClose = leaveDocument;
}

/**
 * The web build's Back for a document's frame: a history entry over the document's own
 * keeps the browser's Back on the page, and each Back takes one `backStep` (a sheet, dialog or
 * menu closes, then the document, asking about unsaved changes), as Android's Back button does.
 * Not in the desktop or Android apps, which have their own Back.
 */
export function useWebBack(doc: number | null): void {
  useEffect(() => {
    if (isTauri() || doc === null) return;
    // The document's own entry carries a mark: Back returns to it, while a new visit to the same
    // address (a link, the address bar) is an entry of its own without it.
    const mark = `${doc}:${Math.random().toString(36).slice(2)}`;
    const own = () => {
      const st: unknown = window.history.state;
      return typeof st === 'object' && st !== null && (st as Record<string, unknown>)[OWN] === mark;
    };
    const enter = () => {
      const st: unknown = window.history.state;
      window.history.replaceState(
        { ...(typeof st === 'object' && st ? st : {}), [OWN]: mark },
        '',
        window.location.href,
      );
      pushGuard();
    };
    if (!guarded()) enter();
    const guardHref = window.location.href;
    const onPop = () => {
      if (guarded() || window.location.href !== guardHref) return;
      // The same address visited again: the document stays, with its guard over the new entry.
      if (!own()) {
        enter();
        return;
      }
      // Back to the document's own entry: Back was pressed here.
      const step = backStep();
      if (step === 'layer' || step === 'document') pushGuard();
      if (step === 'document') afterClose = leaveDocument;
    };
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, [doc]);
}

/** The window shows `app` (`home`, `write`, `grid`, `deck`, `present`). */
export function useDesktopRoute(app: string): void {
  useEffect(() => {
    // Back's steps (`backStep`) know the route in the browser too.
    state.app = app;
    if (!isTauri()) return;
    state.doc = null;
    state.dirty = false;
    report();
    installBack();
  }, [app]);
}

/**
 * Android: sends a copy of document `doc` through the share sheet (its file as last saved, or a
 * PDF of it). Rejects with a message to show (save first, no app takes it).
 */
export async function shareDocument(doc: number, pdf: boolean): Promise<void> {
  const s = await getShell();
  await s.shareDocument(doc, pdf);
}

/**
 * Shows `route` (a document, or a new one) from a document's frame. In the browser the tab goes
 * there. On the desktop it opens in a window of its own, as in Office: the window keeps the
 * document it shows, so no document with unsaved changes is left without a window.
 */
export function showRoute(route: string): void {
  if (!isTauri()) {
    window.location.hash = route;
    return;
  }
  void getShell()
    .then((s) => s.openRoute(route))
    .catch((error: unknown) => console.error('Kiln: could not open a window', error));
}

/** Kiln Home has painted (the startup probe's end). */
export function desktopHomePainted(): void {
  if (!isTauri()) return;
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      void getShell()
        .then((s) => s.homePainted())
        .catch(() => undefined);
    }),
  );
}

/** Text fields of the chrome: the menu's clipboard, undo and Select All act on their text there. */
const TEXT_FIELD =
  'textarea, input:not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="range"]):not([type="color"]):not([type="file"])';
const CHROME =
  '.kiln-ribbon, .kiln-titlebar, .kiln-statusbar, .kiln-backstage, aside, [role="dialog"], [data-kiln-shortcuts="off"]';

function chromeField(): HTMLInputElement | HTMLTextAreaElement | null {
  const el = document.activeElement;
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return null;
  return el.matches(TEXT_FIELD) && el.closest(CHROME) ? el : null;
}

/** The menu's editing commands as a text field does them. `true` when done. */
async function fieldCommand(command: string): Promise<boolean> {
  const field = chromeField();
  if (!field) return false;
  const selected = () => field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0);
  switch (command) {
    case 'write.caret.select_all':
    case 'grid.nav.select_all':
    case 'suite.edit.select_all':
      field.select();
      return true;
    case 'suite.edit.undo':
      document.execCommand('undo');
      return true;
    case 'suite.edit.redo':
      document.execCommand('redo');
      return true;
    case 'suite.edit.copy':
    case 'suite.edit.cut': {
      const text = selected();
      if (text) await (await getShell()).clipboardText(text).catch(() => '');
      if (command === 'suite.edit.cut' && text && !field.readOnly) document.execCommand('delete');
      return true;
    }
    case 'suite.edit.paste':
    case 'suite.edit.paste_special': {
      if (field.readOnly) return true;
      const s = await getShell();
      const text = await s.clipboardText().catch(() => '');
      if (text) document.execCommand('insertText', false, text);
      return true;
    }
    default:
      // Every other menu command waits while a dialog has the keyboard.
      return !!field.closest('[data-kiln-shortcuts="off"]');
  }
}

/**
 * The document frame's part: reports the document, runs the native menu's commands through the
 * registry, and closes the window with its document (after the usual question about unsaved
 * changes) when its close button is pressed, or the document when it is closed with Ctrl+W.
 */
export function useDesktopFrame(
  registry: CommandRegistry,
  client: KilnClient | null,
  summary: DocSummary | null,
): void {
  const closing = useRef(false);
  const doc = summary?.doc ?? null;
  const dirty = !!summary?.dirty;

  useEffect(() => {
    if (!isTauri()) return;
    if (state.doc === doc && state.dirty === dirty) return;
    state.doc = doc;
    state.dirty = dirty;
    report();
  }, [doc, dirty]);

  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    const stops: (() => void)[] = [];
    void getShell().then(async (s) => {
      const menu = await s.onMenuCommand(({ command, args }) => {
        void fieldCommand(command).then((done) => {
          if (done) return;
          if (document.querySelector('[data-kiln-shortcuts="off"][role="dialog"]')) return;
          const st = registry.getState(command);
          if (st.exists && !st.pending && st.enabled) void registry.execute(command, args ?? undefined);
        });
      });
      const close = await s.onCloseRequested(() => {
        closing.current = true;
        void registry.execute('suite.file.close').catch(() => {
          closing.current = false;
        });
      });
      if (live) stops.push(menu, close);
      else {
        menu();
        close();
      }
    });
    return () => {
      live = false;
      for (const stop of stops) stop();
    };
  }, [registry]);

  // Back (Android's button, the browser's, the phone's app bar) closes the document once nothing
  // else is open; in the browser Kiln Home shows once it has gone (`afterClose`), and a close that
  // was cancelled (Cancel in the question about unsaved changes) forgets it.
  useEffect(() => {
    if (!isAndroid() && isTauri()) return;
    const close = () => {
      afterClose = null;
      void registry
        .execute('suite.file.close')
        .then((r) => {
          // Cancelled (the question's Cancel answers `Unchanged`), or not run at all.
          const outcome = (r.value as { outcome?: unknown } | undefined)?.outcome;
          if (r.status !== 'ran' || outcome === 'Unchanged') afterClose = null;
        })
        .catch(() => {
          afterClose = null;
        });
    };
    backFrame = close;
    return () => {
      if (backFrame === close) backFrame = null;
    };
  }, [registry]);

  // The browser: the document Back closed has gone, so its page goes too.
  useEffect(() => {
    if (isTauri() || !client || doc === null) return;
    return client.subscribe((e: Event) => {
      if (typeof e !== 'object' || !('Closed' in e) || e.Closed.doc !== doc) return;
      const leave = afterClose;
      afterClose = null;
      leave?.();
    });
  }, [client, doc]);

  // The document is closed: its window goes too (the last one shows Kiln Home instead, unless
  // its close button was pressed).
  useEffect(() => {
    if (!isTauri() || !client || doc === null) return;
    return client.subscribe((e: Event) => {
      if (typeof e !== 'object' || !('Closed' in e) || e.Closed.doc !== doc) return;
      const asked = closing.current;
      closing.current = false;
      void getShell()
        .then((s) => s.closeWindow(!asked))
        .then((closed) => {
          if (!closed) window.location.hash = '#/home';
        })
        .catch(() => undefined);
    });
  }, [client, doc]);
}

/** Kiln Home's part: File ▸ Open in the native menu. */
export function useDesktopHome(open: () => void): void {
  const run = useRef(open);
  run.current = open;
  useEffect(() => {
    if (!isTauri()) return;
    let live = true;
    let stop: (() => void) | null = null;
    void getShell().then(async (s) => {
      const off = await s.onMenuCommand(({ command }) => {
        if (command === 'suite.file.open') run.current();
      });
      if (live) stop = off;
      else off();
    });
    return () => {
      live = false;
      stop?.();
    };
  }, []);
}

/** A presentation window closes itself (the audience's window at the end of the show). */
export function closeThisWindow(): void {
  void getShell()
    .then((s) => s.closeWindow(false))
    .catch(() => undefined);
}

/**
 * Files dropped on the window where the shell leaves drops to the page (Windows): documents open
 * through the shell by path when the webview gives one, else from their bytes.
 */
export function installDesktopDrop(open: (file: File) => void): () => void {
  if (!isTauri() || !/Windows/i.test(navigator.userAgent)) return () => undefined;
  const over = (e: DragEvent) => {
    if (e.dataTransfer?.types.includes('Files')) e.preventDefault();
  };
  const drop = (e: DragEvent) => {
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length === 0) return;
    e.preventDefault();
    for (const f of files) open(f);
  };
  window.addEventListener('dragover', over);
  window.addEventListener('drop', drop);
  return () => {
    window.removeEventListener('dragover', over);
    window.removeEventListener('drop', drop);
  };
}
