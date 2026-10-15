import {
  type AppId,
  type CommandContext as CommandCtx,
  type CommandMeta,
  type CommandRegistry,
  type CommandState,
  formatShortcut,
  type Keymap,
  type Platform,
} from '@officina/commands';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useSyncExternalStore,
} from 'react';

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
  if (!v) throw new Error('Officina chrome components need a <CommandsProvider> (or <AppFrame>) above them');
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

/** A stable function that runs a command (fire-and-forget, errors are reported by the registry). */
export function useExecute(): (id: string, args?: unknown, ctx?: CommandCtx) => void {
  const { registry } = useCommands();
  return useCallback(
    (id, args, ctx) => {
      void registry.execute(id, args, ctx);
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
