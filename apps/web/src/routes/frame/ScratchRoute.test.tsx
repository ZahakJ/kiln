import { describe, expect, it } from 'vitest';
import { commandForInput, commandForKey } from './ScratchRoute';

const key = (
  k: string,
  mods: Partial<{ ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }> = {},
) => commandForKey({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });

describe('scratch keymap', () => {
  it('maps typing, editing and Office shortcuts to commands', () => {
    expect(key('a')).toBeNull(); // text arrives as beforeinput
    expect(key('Enter')).toEqual({ Write: { InsertText: { text: '\n' } } });
    expect(key('Backspace')).toEqual({ Write: 'DeleteBackward' });
    expect(key('z', { ctrlKey: true })).toEqual({ Suite: 'Undo' });
    expect(key('y', { ctrlKey: true })).toEqual({ Suite: 'Redo' });
    expect(key('Z', { metaKey: true, shiftKey: true })).toEqual({ Suite: 'Redo' });
    expect(key('ArrowLeft', { ctrlKey: true, shiftKey: true })).toEqual({
      Write: { MoveCaret: { motion: 'WordBackward', extend: true } },
    });
    expect(key('End', { ctrlKey: true })).toEqual({
      Write: { MoveCaret: { motion: 'DocumentEnd', extend: false } },
    });
    expect(key('Shift')).toBeNull();
    expect(key('c', { ctrlKey: true })).toBeNull();
    expect(key('F5')).toBeNull();
  });

  it('maps input events to text commands', () => {
    expect(commandForInput('insertText', 'é👋')).toEqual({ Write: { InsertText: { text: 'é👋' } } });
    expect(commandForInput('insertText', null)).toBeNull();
    expect(commandForInput('insertParagraph', null)).toEqual({ Write: { InsertText: { text: '\n' } } });
    expect(commandForInput('deleteContentBackward', null)).toEqual({ Write: 'DeleteBackward' });
    expect(commandForInput('formatBold', null)).toBeNull();
  });
});
