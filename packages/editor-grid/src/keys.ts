/**
 * The grid's own keys (PLAN.md §1.7, Grid column): what arrows, Enter, Tab, Esc, Home, Page Up/Down,
 * Delete and Backspace do, which depends on the mode the core reports (Ready, Enter, Edit, Point)
 * and on the suggestion list. Everything else (Ctrl+B, Ctrl+Arrows, F4, Ctrl+D…) goes to the
 * window's keymap. Pure functions of the key and the mode, so they are tested without a browser.
 */
import type { GridDirection, GridMode } from '@kiln/protocol';

/** The parts of a keyboard event the grid looks at. */
export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

/** What a key does in the grid. */
export type KeyAction =
  /** A core command (sent straight to the core, in order with the typing). */
  | { kind: 'core'; id: string; args?: Record<string, unknown> }
  /** The textarea handles it (typing, caret movement in Edit mode). */
  | { kind: 'native' }
  /** A line break typed into the entry. */
  | { kind: 'newline' };

const ARROWS: Readonly<Record<string, GridDirection>> = {
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
};

/** Whether a key types a character (no Ctrl/Cmd, one character or a dead key). */
export function typesText(e: KeyLike): boolean {
  if (e.ctrlKey || e.metaKey) return false;
  return [...e.key].length === 1 || e.key === 'Dead' || e.key === 'Process';
}

/** The grid's action for a key, or `null` to leave it to the window's keymap. */
export function gridKeyAction(
  e: KeyLike,
  mode: GridMode,
  editor: { completions: number; pickList: boolean } | null,
): KeyAction | null {
  const mod = e.ctrlKey || e.metaKey;
  const dir = ARROWS[e.key];
  if (mode === 'Ready') {
    if (mod || e.altKey) {
      if (e.altKey && !mod && (e.key === 'PageUp' || e.key === 'PageDown'))
        return {
          kind: 'core',
          id: 'grid.nav.page',
          args: { dir: e.key === 'PageUp' ? 'Left' : 'Right', extend: e.shiftKey },
        };
      return null;
    }
    if (dir) return { kind: 'core', id: 'grid.nav.move', args: { dir, extend: e.shiftKey } };
    // Shift+Space selects the row (the window's keymap), as in Excel; a plain space starts an entry.
    if (e.key === ' ' && e.shiftKey) return null;
    switch (e.key) {
      case 'Enter':
        return { kind: 'core', id: 'grid.nav.advance', args: { backward: e.shiftKey, across: false } };
      case 'Tab':
        return { kind: 'core', id: 'grid.nav.advance', args: { backward: e.shiftKey, across: true } };
      case 'Home':
        return { kind: 'core', id: 'grid.nav.row_start', args: { extend: e.shiftKey } };
      case 'PageDown':
        return { kind: 'core', id: 'grid.nav.page', args: { dir: 'Down', extend: e.shiftKey } };
      case 'PageUp':
        return { kind: 'core', id: 'grid.nav.page', args: { dir: 'Up', extend: e.shiftKey } };
      case 'Delete':
        return { kind: 'core', id: 'grid.editing.clear_contents' };
      case 'Backspace':
        // Excel clears the cell and starts an empty entry.
        return { kind: 'core', id: 'grid.editing.begin', args: { text: '' } };
      default:
        return typesText(e) ? { kind: 'native' } : null;
    }
  }
  // Editing (Enter, Edit or Point mode).
  const list = editor && (editor.completions > 0 || editor.pickList);
  if (list && !mod && !e.altKey) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp')
      return { kind: 'core', id: 'grid.editing.move_completion', args: { down: e.key === 'ArrowDown' } };
    if (e.key === 'Tab' && !e.shiftKey) return { kind: 'core', id: 'grid.editing.accept_completion' };
  }
  if (e.key === 'Enter') {
    if (e.altKey && !mod) return { kind: 'newline' };
    if (mod && e.shiftKey) return { kind: 'core', id: 'grid.editing.array_formula' };
    if (mod) return { kind: 'core', id: 'grid.editing.fill_selection' };
    if (editor?.pickList) return { kind: 'core', id: 'grid.editing.accept_completion' };
    return { kind: 'core', id: 'grid.editing.commit', args: { advance: e.shiftKey ? 'Up' : 'Down' } };
  }
  if (e.key === 'Tab' && !mod && !e.altKey)
    return { kind: 'core', id: 'grid.editing.commit', args: { advance: e.shiftKey ? 'Left' : 'Right' } };
  if (e.key === 'Escape') return { kind: 'core', id: 'grid.editing.cancel_edit' };
  if (e.key === 'F2' && !mod) return { kind: 'core', id: 'grid.editing.edit_cell' };
  if (e.key === 'F4' && !mod) return { kind: 'core', id: 'grid.editing.toggle_absolute' };
  if (dir && !e.altKey && !mod && mode !== 'Edit')
    return { kind: 'core', id: 'grid.editing.point_move', args: { dir, extend: e.shiftKey } };
  // The entry's own clipboard and select-all (the textarea's, not the sheet's).
  if (mod && !e.altKey && !e.shiftKey && ['a', 'c', 'x', 'v'].includes(e.key.toLowerCase()))
    return { kind: 'native' };
  if (mod || e.altKey) return null;
  return { kind: 'native' };
}
