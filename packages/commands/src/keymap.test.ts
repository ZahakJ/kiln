import { describe, expect, it, vi } from 'vitest';
import { Keymap } from './keymap.ts';
import { createDefaultKeymap } from './keymaps/index.ts';
import { MemoryStorage } from './storage.ts';
import { APP_IDS } from './types.ts';

const ctrl = (key: string, extra: Record<string, boolean> = {}) => ({ key, ctrlKey: true, ...extra });

describe('Keymap', () => {
  it('resolves events per app, honouring scopes', () => {
    const km = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
    expect(km.resolve(ctrl('b'), { app: 'write' })?.command).toBe('suite.format.bold');
    expect(km.resolve(ctrl('L', { shiftKey: true }), { app: 'write' })?.command).toBe('suite.format.bullets');
    expect(km.resolve(ctrl('L', { shiftKey: true }), { app: 'grid' })?.command).toBe(
      'grid.data.toggle_filter',
    );
    expect(km.resolve(ctrl('d'), { app: 'deck' })?.command).toBe('deck.slides.duplicate');
    // Ctrl+Enter makes a new slide only in the thumbnail rail.
    expect(km.resolve(ctrl('Enter'), { app: 'deck' })).toBeUndefined();
    expect(km.resolve(ctrl('Enter'), { app: 'deck', scopes: ['rail'] })?.command).toBe(
      'deck.slides.new_slide',
    );
    // F4 toggles $ while editing a cell, repeats otherwise.
    expect(km.resolve({ key: 'F4' }, { app: 'grid' })?.command).toBe('suite.edit.repeat');
    expect(km.resolve({ key: 'F4' }, { app: 'grid', scopes: ['cell-edit'] })?.command).toBe(
      'grid.editing.toggle_absolute',
    );
    // Slide-show keys win over editing keys while presenting.
    expect(km.resolve(ctrl('p'), { app: 'deck', scopes: ['slideshow'] })?.command).toBe('deck.show.pen');
    expect(km.resolve(ctrl('p'), { app: 'deck' })?.command).toBe('suite.file.print');
    expect(km.resolve({ key: '3', code: 'Digit3' }, { app: 'deck', scopes: ['slideshow'] })?.args).toEqual({
      digit: 3,
    });
    expect(km.resolve(ctrl('b'), {})).toBeUndefined();
  });

  it('uses ⌘ on macOS', () => {
    const km = createDefaultKeymap({ platform: 'mac', storage: new MemoryStorage() });
    expect(km.resolve({ key: 'b', metaKey: true }, { app: 'write' })?.command).toBe('suite.format.bold');
    expect(km.resolve({ key: 'b', ctrlKey: true }, { app: 'write' })).toBeUndefined();
    expect(km.label('ui.palette.open', 'write')).toBe('⌥Q');
  });

  it('only fires bare Alt on release', () => {
    const km = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
    expect(km.resolve({ key: 'Alt', altKey: true }, { app: 'write' })).toBeUndefined();
    expect(km.resolve({ key: 'Alt' }, { app: 'write' }, { release: true })?.command).toBe('ui.keytips.show');
    expect(km.resolve({ key: 'F10' }, { app: 'write' })?.command).toBe('ui.keytips.show');
  });

  it('lists shortcuts per command for tooltips and menus', () => {
    const km = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
    expect(km.shortcutsFor('suite.edit.redo', 'write')).toEqual(['Mod+Y', 'Mod+Shift+Z']);
    expect(km.primaryShortcut('ui.keytips.show', 'write')).toBe('F10');
    expect(km.label('suite.format.grow_font', 'write')).toBe('Ctrl+>');
    expect(km.commandsFor('Mod+Shift+L', 'grid')).toEqual(['grid.data.toggle_filter']);
  });

  it('has no conflicting defaults in any app', () => {
    const km = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
    for (const app of APP_IDS) expect(km.conflicts(app), app).toEqual([]);
    const mac = createDefaultKeymap({ platform: 'mac', storage: new MemoryStorage() });
    for (const app of APP_IDS) expect(mac.conflicts(app), `mac ${app}`).toEqual([]);
  });

  it('detects conflicts in custom keymaps', () => {
    const km = new Keymap({
      platform: 'other',
      storage: new MemoryStorage(),
      layers: { common: [{ key: 'Mod+B', command: 'a.one' }], write: [{ key: 'Ctrl+B', command: 'a.two' }] },
    });
    expect(km.conflicts('write')).toEqual([
      { layer: 'write', shortcut: 'Ctrl+B', scope: undefined, commands: ['a.one', 'a.two'] },
    ]);
  });

  it('throws on malformed default bindings', () => {
    expect(
      () =>
        new Keymap({
          storage: new MemoryStorage(),
          layers: { write: [{ key: 'Mod+Nope', command: 'a.b' }] },
        }),
    ).toThrow(/Keymap "write".*a\.b/);
  });

  it('remaps Office-style, persists, and resets', () => {
    const storage = new MemoryStorage();
    const km = createDefaultKeymap({ platform: 'other', storage });
    const listener = vi.fn();
    km.subscribe(listener);
    // Give Ctrl+B to Strikethrough: Bold loses it.
    const losers = km.assign('suite.format.strikethrough', 'Mod+B', 'write');
    expect(losers).toEqual(['suite.format.bold']);
    expect(km.resolve(ctrl('b'), { app: 'write' })?.command).toBe('suite.format.strikethrough');
    expect(km.shortcutsFor('suite.format.bold', 'write')).toEqual([]);
    expect(km.isCustomized('suite.format.bold', 'write')).toBe(true);
    // Other apps keep the default.
    expect(km.resolve(ctrl('b'), { app: 'grid' })?.command).toBe('suite.format.bold');
    expect(listener).toHaveBeenCalled();

    // A new keymap on the same storage sees the remapping.
    const again = createDefaultKeymap({ platform: 'other', storage });
    expect(again.resolve(ctrl('b'), { app: 'write' })?.command).toBe('suite.format.strikethrough');

    again.unassign('suite.format.strikethrough', 'Mod+B', 'write');
    expect(again.resolve(ctrl('b'), { app: 'write' })).toBeUndefined();
    again.resetCommand('suite.format.bold', 'write');
    again.resetCommand('suite.format.strikethrough', 'write');
    expect(again.resolve(ctrl('b'), { app: 'write' })?.command).toBe('suite.format.bold');
    again.assign('suite.format.italic', 'Mod+Alt+I', 'write');
    again.resetAll();
    expect(storage.getItem('kiln.keymap')).toBeNull();
    expect(again.shortcutsFor('suite.format.italic', 'write')).toEqual(['Mod+I']);
  });

  it('ignores corrupt stored overrides', () => {
    const storage = new MemoryStorage();
    storage.setItem(
      'kiln.keymap',
      '{"version":1,"layers":{"write":{"suite.format.bold":["Mod+Nope", 7]},"bogus":{}}}',
    );
    const km = createDefaultKeymap({ platform: 'other', storage });
    expect(km.shortcutsFor('suite.format.bold', 'write')).toEqual([]);
    storage.setItem('kiln.keymap', 'not json');
    expect(
      createDefaultKeymap({ platform: 'other', storage }).shortcutsFor('suite.format.bold', 'write'),
    ).toEqual(['Mod+B']);
    km.importOverrides({ version: 2 });
    expect(km.overrides.layers).toEqual({});
  });
});
