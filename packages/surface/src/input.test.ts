import { describe, expect, it } from 'vitest';
import { autoscrollSpeed, charCount, commandForInput, commandForKey, unitForClicks } from './input';

const key = (
  k: string,
  mods: Partial<{ ctrl: boolean; shift: boolean; alt: boolean; meta: boolean }> = {},
) => ({
  key: k,
  ctrlKey: !!mods.ctrl,
  shiftKey: !!mods.shift,
  altKey: !!mods.alt,
  metaKey: !!mods.meta,
});

describe('keymap (Word)', () => {
  it('moves the caret, by words and paragraphs with Ctrl, extending with Shift', () => {
    expect(commandForKey(key('ArrowLeft'), false)).toEqual({
      Write: { MoveCaret: { motion: 'Backward', extend: false } },
    });
    expect(commandForKey(key('ArrowRight', { ctrl: true, shift: true }), false)).toEqual({
      Write: { MoveCaret: { motion: 'WordForward', extend: true } },
    });
    expect(commandForKey(key('ArrowUp', { ctrl: true }), false)).toEqual({
      Write: { MoveCaret: { motion: 'ParagraphBackward', extend: false } },
    });
    expect(commandForKey(key('End', { ctrl: true }), false)).toEqual({
      Write: { MoveCaret: { motion: 'DocumentEnd', extend: false } },
    });
    expect(commandForKey(key('PageDown', { shift: true }), false)).toEqual({
      Write: { MoveCaret: { motion: 'PageDown', extend: true } },
    });
  });

  it('maps undo, redo, select all and Tab; leaves text keys to beforeinput', () => {
    expect(commandForKey(key('z', { ctrl: true }), false)).toEqual({ Suite: 'Undo' });
    expect(commandForKey(key('y', { ctrl: true }), false)).toEqual({ Suite: 'Redo' });
    expect(commandForKey(key('Z', { ctrl: true, shift: true }), false)).toEqual({ Suite: 'Redo' });
    expect(commandForKey(key('a', { ctrl: true }), false)).toEqual({ Write: 'SelectAll' });
    expect(commandForKey(key('Tab'), false)).toEqual({ Write: { InsertText: { text: '\t' } } });
    expect(commandForKey(key('a'), false)).toBeNull();
    expect(commandForKey(key('Enter'), false)).toBeNull();
    expect(commandForKey(key('Backspace'), false)).toBeNull();
  });

  it('uses Command and Option on Apple platforms', () => {
    expect(commandForKey(key('z', { meta: true }), true)).toEqual({ Suite: 'Undo' });
    expect(commandForKey(key('z', { ctrl: true }), true)).toBeNull();
    expect(commandForKey(key('ArrowLeft', { alt: true }), true)).toEqual({
      Write: { MoveCaret: { motion: 'WordBackward', extend: false } },
    });
    expect(commandForKey(key('ArrowRight', { meta: true, shift: true }), true)).toEqual({
      Write: { MoveCaret: { motion: 'LineEnd', extend: true } },
    });
    expect(commandForKey(key('ArrowDown', { meta: true }), true)).toEqual({
      Write: { MoveCaret: { motion: 'DocumentEnd', extend: false } },
    });
  });
});

describe('beforeinput', () => {
  it('maps text, breaks and deletions', () => {
    expect(commandForInput('insertText', 'é')).toEqual({ Write: { InsertText: { text: 'é' } } });
    expect(commandForInput('insertText', null)).toBeNull();
    expect(commandForInput('insertParagraph', null)).toEqual({ Write: { InsertText: { text: '\n' } } });
    expect(commandForInput('deleteContentBackward', null)).toEqual({ Write: 'DeleteBackward' });
    expect(commandForInput('deleteContentForward', null)).toEqual({ Write: 'DeleteForward' });
    expect(commandForInput('deleteWordBackward', null)).toEqual({ Write: 'DeleteWordBackward' });
    expect(commandForInput('deleteWordForward', null)).toEqual({ Write: 'DeleteWordForward' });
    expect(commandForInput('historyUndo', null)).toEqual({ Suite: 'Undo' });
    expect(commandForInput('formatBold', null)).toBeNull();
  });

  it('counts characters as the core does and maps clicks and autoscroll', () => {
    expect(charCount('日本語')).toBe(3);
    expect(charCount('👋🏽')).toBe(2);
    expect(unitForClicks(1)).toBe('Char');
    expect(unitForClicks(2)).toBe('Word');
    expect(unitForClicks(4)).toBe('Paragraph');
    expect(autoscrollSpeed(-3)).toBe(0);
    expect(autoscrollSpeed(10)).toBe(10);
    expect(autoscrollSpeed(1000)).toBe(60);
  });
});
