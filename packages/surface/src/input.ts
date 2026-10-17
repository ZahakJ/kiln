/**
 * Keyboard and text input → commands. Presentation only (PLAN.md §3.7): which key means what is
 * the keymap; what a command does is decided by the core.
 */

import type { CaretMotion, Command } from '@kiln/protocol';

/** The fields of a keyboard event the keymap reads. */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** Whether the platform's primary modifier is Command (macOS, iOS). */
export function isApplePlatform(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform ?? '');
}

function motionFor(key: string, word: boolean): CaretMotion | null {
  switch (key) {
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
    case 'PageUp':
      return 'PageUp';
    case 'PageDown':
      return 'PageDown';
    default:
      return null;
  }
}

/**
 * The command for a key press that does not produce text (caret motion, Tab, undo, select all),
 * or `null` to let `beforeinput` handle it. Word's keys: Ctrl+arrows move by words and
 * paragraphs, Shift extends.
 */
export function commandForKey(e: KeyLike, apple = isApplePlatform()): Command | null {
  const primary = apple ? e.metaKey : e.ctrlKey;
  if (primary && !e.altKey) {
    const k = e.key.toLowerCase();
    if (k === 'z' && !e.shiftKey) return { Suite: 'Undo' };
    if (k === 'y' || (k === 'z' && e.shiftKey)) return { Suite: 'Redo' };
    if (k === 'a' && !e.shiftKey) return { Write: 'SelectAll' };
  }
  // Alt with an arrow is the application's on Windows and Linux (Word: Alt+Shift+Up moves the
  // paragraph); on macOS Option is the word modifier.
  if (!apple && e.altKey) return null;
  // Word motion: Ctrl on Windows/Linux, Option on macOS (Cmd+arrows are line/document ends there).
  const word = apple ? e.altKey : e.ctrlKey;
  if (apple && e.metaKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
    return {
      Write: { MoveCaret: { motion: e.key === 'ArrowLeft' ? 'LineStart' : 'LineEnd', extend: e.shiftKey } },
    };
  }
  if (apple && e.metaKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    return {
      Write: {
        MoveCaret: { motion: e.key === 'ArrowUp' ? 'DocumentStart' : 'DocumentEnd', extend: e.shiftKey },
      },
    };
  }
  const motion = motionFor(e.key, word);
  if (motion) return { Write: { MoveCaret: { motion, extend: e.shiftKey } } };
  if (e.key === 'Tab' && !primary && !e.altKey) return { Write: { InsertText: { text: '\t' } } };
  return null;
}

/**
 * The command for a `beforeinput` event outside an IME composition, or `null` for input types the
 * surface ignores (formatting shortcuts arrive as commands from the ribbon instead).
 */
export function commandForInput(inputType: string, data: string | null): Command | null {
  switch (inputType) {
    case 'insertText':
    case 'insertReplacementText':
    case 'insertFromDrop':
    case 'insertFromYank':
      return data ? { Write: { InsertText: { text: data } } } : null;
    case 'insertParagraph':
    case 'insertLineBreak':
      return { Write: { InsertText: { text: '\n' } } };
    case 'deleteContentBackward':
    case 'deleteByCut':
      return { Write: 'DeleteBackward' };
    case 'deleteContentForward':
      return { Write: 'DeleteForward' };
    case 'deleteWordBackward':
    case 'deleteSoftLineBackward':
      return { Write: 'DeleteWordBackward' };
    case 'deleteWordForward':
    case 'deleteSoftLineForward':
      return { Write: 'DeleteWordForward' };
    case 'historyUndo':
      return { Suite: 'Undo' };
    case 'historyRedo':
      return { Suite: 'Redo' };
    default:
      return null;
  }
}

/** Number of characters (Unicode scalar values) in a string, as the core counts offsets. */
export function charCount(text: string): number {
  let n = 0;
  for (const _ of text) n += 1;
  return n;
}

/** What a click selects, from the click count (1: caret, 2: word, 3+: paragraph). */
export function unitForClicks(detail: number): 'Char' | 'Word' | 'Paragraph' {
  if (detail >= 3) return 'Paragraph';
  if (detail === 2) return 'Word';
  return 'Char';
}

/**
 * Autoscroll speed (document pixels per frame) for a pointer dragged `distance` CSS pixels past
 * an edge of the surface (negative inside): grows with the distance, capped.
 */
export function autoscrollSpeed(distance: number): number {
  if (distance <= 0) return 0;
  return Math.min(60, 4 + distance * 0.6);
}
