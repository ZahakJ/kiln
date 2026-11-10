/**
 * The stage's own keys (Deck column): what arrows, Home/End, Backspace/Delete,
 * Enter, Tab and Esc do, which depends on whether text is being edited, objects are selected or
 * only the slide is. Everything else (Ctrl+B, Ctrl+D, Shift+Arrows on an object…) goes to the
 * window's keymap. Pure functions of the key and the mode, so they are tested without a browser.
 */
import type { CaretMotion } from '@kiln/protocol';

/** The parts of a keyboard event the stage looks at. */
export interface KeyLike {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

/** What the stage is doing. */
export type StageMode = 'text' | 'object' | 'slide';

/** What a key does on the stage. */
export type KeyAction =
  /** A core command, sent straight to the core in order with the typing. */
  | { kind: 'core'; id: string; args?: Record<string, unknown> }
  /** The input proxy takes it (a character, a composition). */
  | { kind: 'native' };

/** Whether a key types a character (no Ctrl/Cmd, one character or a dead key). */
export function typesText(e: KeyLike): boolean {
  if (e.ctrlKey || e.metaKey) return false;
  return [...e.key].length === 1 || e.key === 'Dead' || e.key === 'Process';
}

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'] as const;

/** The caret motion of a key while editing text. */
export function caretMotion(e: KeyLike): CaretMotion | null {
  const word = e.ctrlKey || e.metaKey;
  switch (e.key) {
    case 'ArrowLeft':
      return word ? 'WordBackward' : 'Backward';
    case 'ArrowRight':
      return word ? 'WordForward' : 'Forward';
    case 'ArrowUp':
      return word ? 'ParagraphBackward' : 'LineUp';
    case 'ArrowDown':
      return word ? 'ParagraphForward' : 'LineDown';
    case 'Home':
      return word ? 'DocumentStart' : 'LineStart';
    case 'End':
      return word ? 'DocumentEnd' : 'LineEnd';
    default:
      return null;
  }
}

/** The stage's action for a key, or `null` to leave it to the window's keymap. */
export function deckKeyAction(e: KeyLike, mode: StageMode): KeyAction | null {
  const mod = e.ctrlKey || e.metaKey;
  if (mode === 'text') {
    if (e.altKey && !mod) return null;
    const motion = caretMotion(e);
    if (motion && !e.altKey)
      return { kind: 'core', id: 'deck.text.move', args: { motion, extend: e.shiftKey } };
    switch (e.key) {
      case 'Backspace':
        return { kind: 'core', id: 'deck.text.delete', args: { forward: false, word: mod } };
      case 'Delete':
        return { kind: 'core', id: 'deck.text.delete', args: { forward: true, word: mod } };
      case 'Enter':
        if (mod) return null;
        return e.shiftKey
          ? { kind: 'core', id: 'deck.text.line_break' }
          : { kind: 'core', id: 'deck.text.new_paragraph' };
      case 'Tab':
        if (mod) return null;
        return { kind: 'core', id: 'deck.text.tab', args: { shift: e.shiftKey } };
      case 'Escape':
        return { kind: 'core', id: 'deck.select.escape' };
      default:
        return typesText(e) ? { kind: 'native' } : null;
    }
  }
  if (mod || e.altKey) return null;
  if (mode === 'object') {
    if ((ARROWS as readonly string[]).includes(e.key)) {
      // Shift+Arrows resize (the keymap); plain arrows nudge by a grid step.
      if (e.shiftKey) return null;
      return {
        kind: 'core',
        id: 'deck.drawing.nudge',
        args: { direction: e.key.slice(5).toLowerCase(), fine: false },
      };
    }
    switch (e.key) {
      case 'Delete':
      case 'Backspace':
        return { kind: 'core', id: 'deck.edit.delete' };
      case 'Tab':
        return { kind: 'core', id: 'deck.select.next', args: { backward: e.shiftKey } };
      case 'Escape':
        return { kind: 'core', id: 'deck.select.escape' };
      case 'Enter':
        return { kind: 'core', id: 'deck.slides.edit_text' };
      default:
        // Typing on a selected shape edits its text.
        return typesText(e) && e.key !== ' ' ? { kind: 'native' } : null;
    }
  }
  // Only the slide: Page Up/Down and the arrows go through the slides, Tab selects a shape.
  switch (e.key) {
    case 'PageDown':
    case 'ArrowDown':
    case 'ArrowRight':
      return { kind: 'core', id: 'deck.slides.next_slide' };
    case 'PageUp':
    case 'ArrowUp':
    case 'ArrowLeft':
      return { kind: 'core', id: 'deck.slides.previous_slide' };
    case 'Tab':
      return { kind: 'core', id: 'deck.select.next', args: { backward: e.shiftKey } };
    case 'Escape':
      return { kind: 'core', id: 'deck.select.escape' };
    default:
      return null;
  }
}

/** The rail's action for a key (Up/Down/Home/End go through the slides, Delete deletes them). */
export function railKeyAction(
  e: KeyLike,
  current: number,
  count: number,
): { id: string; args?: Record<string, unknown> } | null {
  // PowerPoint's thumbnails: Ctrl+Up and Ctrl+Down move the selected slides one place, with
  // Shift to the start or the end.
  if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    const up = e.key === 'ArrowUp';
    const to = e.shiftKey ? (up ? 0 : count) : up ? Math.max(0, current - 1) : Math.min(count, current + 2);
    return { id: 'deck.slides.move', args: { to } };
  }
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  const select = (index: number) => ({
    id: 'deck.slides.select',
    args: { index: Math.max(0, Math.min(count - 1, index)), extend: e.shiftKey, toggle: false },
  });
  switch (e.key) {
    case 'ArrowUp':
    case 'ArrowLeft':
    case 'PageUp':
      return select(current - 1);
    case 'ArrowDown':
    case 'ArrowRight':
    case 'PageDown':
      return select(current + 1);
    case 'Home':
      return select(0);
    case 'End':
      return select(count - 1);
    case 'Delete':
    case 'Backspace':
      return { id: 'deck.slides.delete' };
    case 'Enter':
      return { id: 'deck.slides.new_slide' };
    default:
      return null;
  }
}
