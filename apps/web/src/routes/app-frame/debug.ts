/**
 * `window.__kiln` in development builds: the window's registry, keymap and command log, and the
 * command id each key press resolved to. The e2e tests (`e2e/specs/shortcuts.spec.ts`,
 * `chrome.spec.ts`) read it to check that a shortcut reaches its catalog id and that core commands
 * reach the session.
 */
import type { AppId, CommandLog, CommandLogEntry, CommandRegistry, Keymap } from '@kiln/commands';
import type { KilnClient } from '@kiln/protocol';

export interface KilnDebugHandle {
  app: AppId;
  registry: CommandRegistry;
  keymap: Keymap;
  client: KilnClient | null;
  /** Commands sent to the core, oldest first. */
  log(): readonly CommandLogEntry[];
  /** The command id (and args) each key press resolved to, oldest first. */
  resolved: { key: string; command: string; args?: unknown }[];
  /** Every command the registry ran (ribbon, Key Tips, shortcuts, palette) and how it ended. */
  executed: { id: string; status: string }[];
}

declare global {
  interface Window {
    __kiln?: KilnDebugHandle;
  }
}

export function installDebugHandle(options: {
  app: AppId;
  registry: CommandRegistry;
  keymap: Keymap;
  log: CommandLog;
  client: KilnClient | null;
}): () => void {
  if (!import.meta.env.DEV) return () => {};
  const handle: KilnDebugHandle = {
    app: options.app,
    registry: options.registry,
    keymap: options.keymap,
    client: options.client,
    log: () => options.log.entries(),
    resolved: [],
    executed: [],
  };
  const stopExecuted = options.registry.subscribe((e) => {
    if (e.type === 'executed') handle.executed.push({ id: e.id, status: e.result.status });
  });
  const onKey = (e: KeyboardEvent) => {
    const binding = options.keymap.resolve(e, options.registry.context);
    if (binding) handle.resolved.push({ key: e.key, command: binding.command, args: binding.args });
  };
  const onKeyUp = (e: KeyboardEvent) => {
    if (e.key !== 'Alt') return;
    const binding = options.keymap.resolve(e, options.registry.context, { release: true });
    if (binding) handle.resolved.push({ key: 'Alt', command: binding.command, args: binding.args });
  };
  window.addEventListener('keydown', onKey, true);
  window.addEventListener('keyup', onKeyUp, true);
  window.__kiln = handle;
  return () => {
    stopExecuted();
    window.removeEventListener('keydown', onKey, true);
    window.removeEventListener('keyup', onKeyUp, true);
    if (window.__kiln === handle) window.__kiln = undefined;
  };
}
