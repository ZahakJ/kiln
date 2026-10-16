/** Kiln Deck shortcuts: the "Kiln Deck (presentations)" column and the slide-show table of docs/SHORTCUTS.md. */
import type { KeyBinding } from '../keymap.ts';
import { bind, SLIDE_SHOW_ROW as S } from './bind.ts';

/** A shape or other object is selected on the slide (not editing its text). */
const OBJECT = { scope: 'object' } as const;
/** Focus is in the thumbnail rail. */
const RAIL = { scope: 'rail' } as const;
/** A slide show is running. */
const SHOW = { scope: 'slideshow' } as const;

const OBJ_ROW = 'Shift+Arrows / Ctrl+Arrows (object selected)';
const DIRS = [
  ['ArrowUp', 'up'],
  ['ArrowDown', 'down'],
  ['ArrowLeft', 'left'],
  ['ArrowRight', 'right'],
] as const;

export const deckKeymap: readonly KeyBinding[] = [
  bind('Mod+Shift+C', 'format.copyFormatting', 'Ctrl+Shift+C / Ctrl+Shift+V'),
  bind('Mod+Shift+V', 'format.pasteFormatting', 'Ctrl+Shift+C / Ctrl+Shift+V'),
  bind('Mod+=', 'format.subscript', 'Ctrl+= / Ctrl+Shift+='),
  bind('Mod+Shift+=', 'format.superscript', 'Ctrl+= / Ctrl+Shift+='),
  bind('Mod+Shift+>', 'format.growFont', 'Ctrl+Shift+> / <, Ctrl+] / ['),
  bind('Mod+Shift+<', 'format.shrinkFont', 'Ctrl+Shift+> / <, Ctrl+] / ['),
  bind('Mod+]', 'format.growFont', 'Ctrl+Shift+> / <, Ctrl+] / [', { args: { step: 1 } }),
  bind('Mod+[', 'format.shrinkFont', 'Ctrl+Shift+> / <, Ctrl+] / [', { args: { step: 1 } }),
  bind('Mod+Shift+L', 'format.bullets', 'Ctrl+Shift+L'),
  bind('Mod+E', 'format.alignCenter', 'Ctrl+E / L / R / J'),
  bind('Mod+L', 'format.alignLeft', 'Ctrl+E / L / R / J'),
  bind('Mod+R', 'format.alignRight', 'Ctrl+E / L / R / J'),
  bind('Mod+J', 'format.justify', 'Ctrl+E / L / R / J'),
  bind('Mod+M', 'deck.newSlide', 'Ctrl+M / Ctrl+Shift+M'),
  bind('Mod+Enter', 'deck.newSlide', 'Ctrl+Enter', RAIL),
  bind('Mod+D', 'deck.duplicate', 'Ctrl+D'),
  bind('Shift+F3', 'format.changeCase', 'Shift+F3'),
  bind('Alt+Shift+ArrowLeft', 'deck.promote', 'Alt+Shift+Arrows'),
  bind('Alt+Shift+ArrowRight', 'deck.demote', 'Alt+Shift+Arrows'),
  bind('F2', 'deck.editText', 'F2'),
  bind('F4', 'edit.repeat', 'F4'),
  bind('F5', 'deck.show.fromBeginning', 'F5 / Shift+F5'),
  bind('Shift+F5', 'deck.show.fromCurrent', 'F5 / Shift+F5'),
  bind('Mod+PageUp', 'deck.previousSlide', 'Ctrl+Page Up / Page Down'),
  bind('Mod+PageDown', 'deck.nextSlide', 'Ctrl+Page Up / Page Down'),
  bind('Mod+Shift+G', 'deck.group', 'Ctrl+Shift+G / Ctrl+Shift+H'),
  bind('Mod+Shift+H', 'deck.ungroup', 'Ctrl+Shift+G / Ctrl+Shift+H'),
  ...DIRS.flatMap(([key, dir]) => [
    bind(`Shift+${key}`, 'deck.resize', OBJ_ROW, { ...OBJECT, args: { direction: dir } }),
    bind(`Mod+${key}`, 'deck.nudge', OBJ_ROW, { ...OBJECT, args: { direction: dir } }),
  ]),

  // During a slide show.
  ...['N', 'Space', 'ArrowRight', 'PageDown'].map((k) =>
    bind(k, 'deck.show.next', `${S}N, Space, Right, Page Down`, SHOW),
  ),
  ...['P', 'Backspace', 'ArrowLeft', 'PageUp'].map((k) =>
    bind(k, 'deck.show.previous', `${S}P, Backspace, Left, Page Up`, SHOW),
  ),
  bind('Home', 'deck.show.first', `${S}Home / End`, SHOW),
  bind('End', 'deck.show.last', `${S}Home / End`, SHOW),
  ...Array.from({ length: 10 }, (_, d) =>
    bind(String(d), 'deck.show.digit', `${S}number, then Enter`, { ...SHOW, args: { digit: d } }),
  ),
  bind('Enter', 'deck.show.goToTyped', `${S}number, then Enter`, SHOW),
  bind('B', 'deck.show.black', `${S}B / W`, SHOW),
  bind('W', 'deck.show.white', `${S}B / W`, SHOW),
  bind('Mod+L', 'deck.show.laser', `${S}Ctrl+L / Ctrl+P / Ctrl+I`, SHOW),
  bind('Mod+P', 'deck.show.pen', `${S}Ctrl+L / Ctrl+P / Ctrl+I`, SHOW),
  bind('Mod+I', 'deck.show.highlighter', `${S}Ctrl+L / Ctrl+P / Ctrl+I`, SHOW),
  bind('Esc', 'deck.show.end', `${S}Esc`, SHOW),
];
