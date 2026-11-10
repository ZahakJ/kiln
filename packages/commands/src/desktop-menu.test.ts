/**
 * The desktop shell's native menus: apps/desktop/src-tauri/menu.json is the text generated
 * from the catalog, the keymaps and the locales; every item is a released command or one of the
 * shell's own actions, its accelerator is the shortcut the app binds and no two items of a menu bar
 * share one; every label is translated.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { COMMAND_CATALOG } from './catalog/index.ts';
import {
  buildDesktopMenu,
  DESKTOP_MENU_CONTEXTS,
  type DesktopMenuItemJson,
  menuAccelerator,
  menuBinding,
} from './desktop-menu.ts';
import { desktopMenuText, readLocales } from './desktop-menu-file.ts';

const menu = buildDesktopMenu(readLocales());
const items = (app: (typeof DESKTOP_MENU_CONTEXTS)[number]) =>
  menu.menus[app].flatMap((m) => m.items).filter((i): i is DesktopMenuItemJson => 'id' in i);

describe('desktop menu', () => {
  it('menu.json is the generated text', () => {
    const file = readFileSync(new URL('../../../apps/desktop/src-tauri/menu.json', import.meta.url), 'utf8');
    expect(file).toBe(desktopMenuText());
  });

  it('turns keymap shortcuts into menu accelerators', () => {
    expect(menuAccelerator('Mod+Shift+S')).toBe('CmdOrCtrl+Shift+S');
    expect(menuAccelerator('F12')).toBe('F12');
    expect(menuAccelerator('Shift+F5')).toBe('Shift+F5');
    expect(menuAccelerator('Mod+/')).toBe('CmdOrCtrl+Slash');
    expect(menuAccelerator('Mod+;')).toBe('CmdOrCtrl+Semicolon');
    expect(menuAccelerator('Mod+Alt+V')).toBe('CmdOrCtrl+Alt+V');
    // Keys the editors need as typed, and shifted symbols.
    expect(menuAccelerator('Esc')).toBeNull();
    expect(menuAccelerator('Alt')).toBeNull();
    expect(menuAccelerator('B')).toBeNull();
    expect(menuAccelerator('Alt+Shift+ArrowUp')).toBeNull();
    expect(menuAccelerator('Mod+Shift+>')).toBeNull();
  });

  it('shows the binding of the app before the common one', () => {
    expect(menuBinding('write', 'write.caret.select_all')?.key).toBe('Mod+A');
    expect(menuBinding('deck', 'deck.show.from_beginning')?.key).toBe('F5');
    expect(menuBinding('grid', 'suite.file.save')?.key).toBe('Mod+S');
  });

  it('names released commands and the shell actions only, each accelerator once per menu bar', () => {
    const declared = new Map(COMMAND_CATALOG.map((d) => [d.id, d]));
    const natives = new Set([
      'new-window',
      'close-window',
      'minimize',
      'fullscreen',
      'quit',
      'about',
      'manual',
      'report',
      'new-write',
      'new-grid',
      'new-deck',
    ]);
    for (const app of DESKTOP_MENU_CONTEXTS) {
      const seen = new Map<string, string>();
      for (const item of items(app)) {
        const [kind, name = ''] = item.id.split(':');
        if (kind === 'cmd') expect(declared.get(name)?.tier ?? 'P0', item.id).toBe('P0');
        else expect(natives.has(name), item.id).toBe(true);
        if (item.accelerator) {
          expect(item.accelerator).toMatch(
            /^((CmdOrCtrl|Ctrl|Super|Alt|Shift)\+)*([A-Z0-9]|F\d{1,2}|Enter|Space|Up|Down|Left|Right|PageUp|PageDown|Home|End|Slash|Semicolon|Equal|Minus|Comma|Period|BracketLeft|BracketRight|Backquote|Quote|Backslash)$/,
          );
          expect(seen.get(item.accelerator), `${app}: ${item.accelerator}`).toBeUndefined();
          seen.set(item.accelerator, item.id);
        }
      }
    }
  });

  it('has every label in every interface language', () => {
    expect(menu.locales).toEqual(['en', 'ar', 'de']);
    for (const app of DESKTOP_MENU_CONTEXTS) {
      for (const m of menu.menus[app]) {
        expect(Object.keys(m.title).sort()).toEqual(['ar', 'de', 'en']);
        for (const item of m.items) if ('label' in item) expect(item.label.ar).not.toBe('');
      }
    }
    expect(menu.menus.write[0]?.title).toEqual({ en: 'File', ar: 'ملف', de: 'Datei' });
    expect(menu.strings.quit).toEqual({ en: 'Quit Kiln', ar: 'إنهاء Kiln', de: 'Kiln beenden' });
  });
});
