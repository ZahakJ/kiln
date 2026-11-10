import {
  type AppId,
  type CommandContext as CommandCtx,
  type CommandMeta,
  type CommandRegistry,
  type CommandState,
  formatShortcut,
  type Keymap,
  type Platform,
} from '@kiln/commands';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useSyncExternalStore,
} from 'react';
import { armReturnToDocument } from '../primitives/returnFocus.ts';

export interface CommandsContextValue {
  registry: CommandRegistry;
  keymap?: Keymap;
  app: AppId;
  platform: Platform;
}

const Ctx = createContext<CommandsContextValue | null>(null);

export interface CommandsProviderProps {
  registry: CommandRegistry;
  keymap?: Keymap;
  app: AppId;
  /** Default: the keymap's platform, else detected. */
  platform?: Platform;
  children?: ReactNode;
}

/** Makes a command registry (and keymap) available to every chrome component below it. */
export function CommandsProvider({ registry, keymap, app, platform, children }: CommandsProviderProps) {
  const value = useMemo<CommandsContextValue>(
    () => ({ registry, keymap, app, platform: platform ?? keymap?.platform ?? detect() }),
    [registry, keymap, app, platform],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

function detect(): Platform {
  if (typeof navigator === 'undefined') return 'other';
  return /mac|iphone|ipad/i.test(navigator.platform ?? '') ? 'mac' : 'other';
}

export function useCommands(): CommandsContextValue {
  const v = useContext(Ctx);
  if (!v) throw new Error('Kiln chrome components need a <CommandsProvider> (or <AppFrame>) above them');
  return v;
}

export function useOptionalCommands(): CommandsContextValue | null {
  return useContext(Ctx);
}

const sameState = (a: CommandState, b: CommandState) =>
  a.exists === b.exists &&
  a.pending === b.pending &&
  a.visible === b.visible &&
  a.enabled === b.enabled &&
  a.checked === b.checked &&
  Object.is(a.value, b.value);

/** Live state of a command (enabled, checked, value…), re-rendering only when it changes. */
export function useCommandState(id: string | undefined): CommandState {
  const { registry } = useCommands();
  const cache = useRef<CommandState | null>(null);
  const subscribe = useCallback(
    (cb: () => void) => (id ? registry.subscribeTo(id, cb) : () => {}),
    [registry, id],
  );
  const get = useCallback(() => {
    const next = registry.getState(id ?? '');
    if (cache.current && cache.current.id === next.id && sameState(cache.current, next)) return cache.current;
    cache.current = next;
    return next;
  }, [registry, id]);
  return useSyncExternalStore(subscribe, get, get);
}

/** Metadata of a command (title, icon, Office name…). */
export function useCommandMeta(id: string | undefined): (CommandMeta & { pending: boolean }) | undefined {
  const { registry } = useCommands();
  const subscribe = useCallback(
    (cb: () => void) => (id ? registry.subscribeTo(id, cb) : () => {}),
    [registry, id],
  );
  const get = useCallback(() => (id ? registry.get(id) : undefined), [registry, id]);
  const cmd = useSyncExternalStore(subscribe, get, get);
  return useMemo(() => (cmd && id ? registry.meta(id) : undefined), [cmd, id, registry]);
}

/** Re-renders when the keymap changes (user remapping); returns a revision counter. */
export function useKeymapRevision(): number {
  const { keymap } = useCommands();
  const rev = useRef(0);
  const subscribe = useCallback(
    (cb: () => void) =>
      keymap
        ? keymap.subscribe(() => {
            rev.current++;
            cb();
          })
        : () => {},
    [keymap],
  );
  return useSyncExternalStore(
    subscribe,
    () => rev.current,
    () => rev.current,
  );
}

/** The platform-neutral primary shortcut of a command in the current app (`Mod+B`). */
export function useCommandShortcut(id: string | undefined): string | undefined {
  const { keymap, app, registry } = useCommands();
  useKeymapRevision();
  if (!id) return undefined;
  return keymap?.primaryShortcut(id, app) ?? registry.get(id)?.shortcut;
}

/** Display label for a shortcut on this platform. */
export function useShortcutLabel(shortcut: string | undefined): string | undefined {
  const { platform } = useCommands();
  return shortcut ? formatShortcut(shortcut, platform) : undefined;
}

/**
 * Where commands run from that hand the keys back to the document afterwards (Office): the
 * ribbon and its menus, popovers, galleries and pickers (the table grid, colours, borders,
 * shapes), the status bar.
 */
const CHROME =
  '.kiln-ribbon, .kiln-menu, .kiln-popover, .kiln-gallery, .kiln-statusbar, .kiln-minitoolbar, [data-kiln-return-focus]';

/** Transient chrome that closes after a command (its focus return comes first). */
const TRANSIENT = '[role="menu"], [role="listbox"], .kiln-popover';

/** A box that takes typing (the ribbon's number boxes, the font name, a search box). */
const FIELD = 'input, textarea, select, [contenteditable="true"], [role="combobox"], [role="spinbutton"]';

/** A dialog the command opened: the keys stay there. */
const MODAL = '[role="dialog"]:not(.kiln-popover), [role="alertdialog"]';

/**
 * After a ribbon, menu, picker or status bar command, Office puts the keys back in the document:
 * the next keystroke types there. Once the command has run and its menu or popover has closed,
 * if the focus is still on the chrome (or nowhere) and no dialog is open,
 * `kiln:return-to-document` is dispatched on `window`; the editors focus their document on it.
 */
export function returnToDocument(
  origin: Element | null = typeof document === 'undefined' ? null : document.activeElement,
): void {
  if (typeof window === 'undefined' || typeof document === 'undefined') return;
  // An open menu, list or popover (not the ribbon's own inline galleries, which are lists too).
  const popup = (el: Element | null) => !!el && !el.closest('.kiln-ribbon');
  const anyOpen = () => [...document.querySelectorAll(TRANSIENT)].some(popup);
  // Focus in a menu, list or popover the user opened since: the keys stay there. So do the keys
  // of a box the user has moved to since the command ran (Enter in one ribbon number box, then a
  // click in the next one): what they type there must not land in the document.
  const onChrome = () => {
    const a = document.activeElement as HTMLElement | null;
    if (!a || a === document.body) return true;
    if (a !== origin && a.matches(FIELD)) return false;
    return !!a.closest(CHROME) && !popup(a.closest(TRANSIENT));
  };
  let tries = 0;
  let sent = 0;
  const check = () => {
    // The page may be gone by the time a timer fires (tests tear the DOM down).
    if (typeof document === 'undefined' || typeof window === 'undefined') return;
    tries += 1;
    // Wait for the menu or popover to close and its focus return (to its trigger) to run. One
    // still open after that is one the user opened since the command: leave it the keys.
    if (anyOpen()) {
      if (tries < 14) setTimeout(check, 40);
      return;
    }
    if (document.querySelector(MODAL) || !onChrome()) return;
    window.dispatchEvent(new CustomEvent('kiln:return-to-document'));
    sent += 1;
    // Focus can come back to the trigger once more as the popover unmounts.
    if (sent < 3) {
      setTimeout(() => {
        if (typeof document !== 'undefined' && !document.querySelector(MODAL) && onChrome()) check();
      }, 120);
    }
  };
  setTimeout(check, 30);
}

/** A stable function that runs a command (fire-and-forget, errors are reported by the registry). */
export function useExecute(): (id: string, args?: unknown, ctx?: CommandCtx) => void {
  const { registry } = useCommands();
  return useCallback(
    (id, args, ctx) => {
      const active = typeof document !== 'undefined' ? (document.activeElement as HTMLElement | null) : null;
      const fromChrome = !!active && (active === document.body || !!active.closest?.(CHROME));
      if (fromChrome) armReturnToDocument();
      void registry.execute(id, args, ctx).then(
        () => {
          if (fromChrome) returnToDocument(active);
        },
        () => undefined,
      );
    },
    [registry],
  );
}

/** A counter that changes whenever commands are (un)registered or their state may have changed. */
export function useRegistryVersion(): number {
  const { registry } = useCommands();
  const version = useRef(0);
  const subscribe = useCallback(
    (cb: () => void) =>
      registry.subscribe((e) => {
        if (e.type !== 'executed') {
          version.current++;
          cb();
        }
      }),
    [registry],
  );
  return useSyncExternalStore(
    subscribe,
    () => version.current,
    () => version.current,
  );
}
