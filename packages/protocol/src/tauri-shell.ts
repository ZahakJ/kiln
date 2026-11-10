/**
 * The desktop shell's own events and commands, next to the core's IPC commands of
 * `TauriClient`: the native menu's commands (`kiln://menu`), a window asked to close
 * (`kiln://close-requested`), and what a window reports or asks (`kiln_window_context`,
 * `kiln_close_window`, `kiln_home_painted`, `kiln_open_paths`, `kiln_open_route`,
 * `kiln_clipboard_text`), and on Android the Back button (the app plugin's
 * `back-button` event), sending a copy through the share sheet (`kiln_share`) and leaving for the
 * launcher (`kiln_leave`).
 *
 * Loaded on demand (see `tauriShell`) so the web bundle does not carry the Tauri API.
 */

import { onBackButtonPress } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** A command picked in the window's native menu. */
export interface MenuCommandEvent {
  command: string;
  args: unknown;
}

/** What a window shows, for its menu bar, its title bar and the close button. */
export interface WindowContext {
  /** `home`, `write`, `grid`, `deck` or `present`. */
  app: string;
  doc: number | null;
  dirty: boolean;
  /** The interface language (`en`, `de`, `ar`). */
  language: string;
  /** `light` or `dark`. */
  theme: 'light' | 'dark';
  /** The title bar's colour (`#RRGGBB`, the page's `theme-color`): Android's system bars take it. */
  bar?: string | null;
}

/** Listens to the window's menu; returns the function that stops listening. */
export function onMenuCommand(handler: (e: MenuCommandEvent) => void): Promise<() => void> {
  return listen<MenuCommandEvent>('kiln://menu', (e) => handler(e.payload));
}

/** Listens to the window's close button (and Quit) while it shows a document. */
export function onCloseRequested(handler: () => void): Promise<() => void> {
  return listen('kiln://close-requested', () => handler());
}

export function reportWindow(ctx: WindowContext): Promise<void> {
  return invoke('kiln_window_context', { ...ctx, bar: ctx.bar ?? null });
}

/** Closes this window; with `unlessLast`, the last window stays (and `false` comes back). */
export function closeWindow(unlessLast: boolean): Promise<boolean> {
  return invoke('kiln_close_window', { unlessLast });
}

export function homePainted(): Promise<void> {
  return invoke('kiln_home_painted');
}

/** Shows a route (a document, a new document) in a window of its own. */
export function openRoute(route: string): Promise<void> {
  return invoke('kiln_open_route', { route });
}

/** Opens documents by their paths, each in a window of its own. */
export function openPaths(paths: string[]): Promise<void> {
  return invoke('kiln_open_paths', { paths });
}

/** The clipboard's plain text; with `text`, puts it there first. */
export function clipboardText(text?: string): Promise<string> {
  return invoke('kiln_clipboard_text', { text: text ?? null });
}

/**
 * Android's Back button and gesture: while a handler listens, Back goes to the page instead of
 * leaving the app. Returns the function that stops listening.
 */
export async function onBack(handler: () => void): Promise<() => void> {
  const listener = await onBackButtonPress(() => handler());
  return () => void listener.unregister();
}

/** Android: Kiln goes to the background (Back on Kiln Home). */
export function leaveApp(): Promise<void> {
  return invoke('kiln_leave');
}

/**
 * Android: sends a copy of a document through the share sheet, its file as last saved or, with
 * `pdf`, a PDF of it. Rejects with a message for the person (save first, no app takes it).
 */
export function shareDocument(doc: number, pdf: boolean): Promise<void> {
  return invoke('kiln_share', { doc, pdf });
}
