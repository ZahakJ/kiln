/**
 * The desktop application menu: File, Edit, View, Insert, Format, Slide Show, Window and
 * Help for each app of the release, made from the command catalog and the keymaps, so a menu item
 * has the command's title and the shortcut the editors bind to it. `scripts/desktop-menu.ts` writes
 * it, with every label in English, German and Arabic, to `apps/desktop/src-tauri/menu.json`, which
 * the Tauri shell turns into its native menus at startup; picking an item runs the command in the
 * window's frame. Items the shell handles itself (New Window, Full Screen, About…) are `native`.
 */
import { COMMAND_CATALOG } from './catalog/index.ts';
import type { KeyBinding } from './keymap.ts';
import { DEFAULT_KEYMAP_LAYERS } from './keymaps/index.ts';
import { SHIFTED_SYMBOLS, tryParseShortcut } from './keys.ts';
import type { AppId } from './types.ts';

/** Text the command system holds as data (`msg` without importing i18n into this module). */
const msg = <const S extends string>(s: S): S => s;

/** Things only the shell can do. */
export type DesktopNative =
  | 'new-window'
  | 'close-window'
  | 'minimize'
  | 'fullscreen'
  | 'quit'
  | 'about'
  | 'manual'
  | 'report'
  | 'new-write'
  | 'new-grid'
  | 'new-deck';

/** One entry of a menu as authored here. */
export type DesktopEntry =
  | string
  | { command: string; args?: unknown }
  | { native: DesktopNative; label: string; accelerator?: string }
  | '-';

/** The menus a window shows: Kiln Home or one of the editors. */
export type DesktopMenuContext = 'home' | 'write' | 'grid' | 'deck';

export const DESKTOP_MENU_CONTEXTS: readonly DesktopMenuContext[] = ['home', 'write', 'grid', 'deck'];

/** Labels of the shell's own items, menus, dialog filters and windows. */
export const DESKTOP_STRINGS = {
  file: msg('File'),
  edit: msg('Edit'),
  view: msg('View'),
  insert: msg('Insert'),
  format: msg('Format'),
  slideShow: msg('Slide Show'),
  review: msg('Review'),
  window: msg('Window'),
  help: msg('Help'),
  newWindow: msg('New Window'),
  closeWindow: msg('Close Window'),
  minimize: msg('Minimize'),
  fullScreen: msg('Full Screen'),
  quit: msg('Quit Kiln'),
  about: msg('About Kiln'),
  hide: msg('Hide Kiln'),
  hideOthers: msg('Hide Others'),
  showAll: msg('Show All'),
  services: msg('Services'),
  manual: msg('Kiln Manual'),
  report: msg('Report a Problem'),
  blankDocument: msg('Blank document'),
  blankWorkbook: msg('Blank workbook'),
  blankPresentation: msg('Blank presentation'),
  // Open and Save As dialogs.
  allDocuments: msg('All documents Kiln opens'),
  wordDocuments: msg('Word documents'),
  excelWorkbooks: msg('Excel workbooks'),
  powerpointPresentations: msg('PowerPoint presentations'),
  opendocumentFiles: msg('OpenDocument files'),
  kilnDocuments: msg('Kiln documents'),
  textFiles: msg('Text and Markdown'),
  pictures: msg('Pictures'),
  pdf: msg('PDF'),
  // Save As formats (`FileFormat`).
  Docx: msg('Word Document'),
  Dotx: msg('Word Template'),
  Docm: msg('Word Macro-Enabled Document'),
  Dotm: msg('Word Macro-Enabled Template'),
  Odt: msg('OpenDocument Text'),
  KWrite: msg('Kiln Write Document'),
  Xlsx: msg('Excel Workbook'),
  Xltx: msg('Excel Template'),
  Xlsm: msg('Excel Macro-Enabled Workbook'),
  Csv: msg('CSV (comma-separated values)'),
  Ods: msg('OpenDocument Spreadsheet'),
  KGrid: msg('Kiln Grid Workbook'),
  Pptx: msg('PowerPoint Presentation'),
  Potx: msg('PowerPoint Template'),
  Pptm: msg('PowerPoint Macro-Enabled Presentation'),
  Ppsx: msg('PowerPoint Show'),
  Odp: msg('OpenDocument Presentation'),
  KDeck: msg('Kiln Deck Presentation'),
  Txt: msg('Plain Text'),
  Md: msg('Markdown'),
  Pdf: msg('PDF'),
  // A file given to Kiln that it cannot open.
  couldNotOpenTitle: msg('Could not open the file'),
  couldNotOpen: msg('Kiln could not open {name}.'),
  notADocument: msg('Kiln opens Word, Excel, PowerPoint, OpenDocument, CSV and Kiln documents.'),
  openNotFound: msg('The file is not there any more. It may have been moved, renamed or deleted.'),
  openNoPermission: msg('Kiln is not allowed to read this file. Check who may open it, then try again.'),
  openDamaged: msg('The file is damaged, or it is not the kind of document its name says.'),
  openUnsupported: msg('The file uses a format or a protection Kiln cannot open.'),
  openUnreadable: msg('Kiln could not read the file. Try again, or copy it to another folder first.'),
  // The Android app.
  androidNoWrite: msg('Kiln may not change this file. Use Save As to keep your changes in a copy.'),
  androidPrintFailed: msg(
    'Kiln could not hand the document to the printer. Try again, or export it as a PDF.',
  ),
  androidNoApp: msg('No app on this device can open it.'),
  androidOneScreen: msg(
    'Presenter View needs a second screen. Start the slide show from the beginning to present on this device.',
  ),
  androidSendCopy: msg('Send a copy of {name}'),
  androidSaveFirst: msg('Save the document first, then send it.'),
  androidLinks: msg('Kiln on Android opens links to web pages and e-mail addresses.'),
  // Window titles.
  presenterView: msg('Presenter View'),
  home: msg('Kiln Home'),
} as const;

export type DesktopStringKey = keyof typeof DESKTOP_STRINGS;

const S = DESKTOP_STRINGS;

const nat = (native: DesktopNative, label: string, accelerator?: string): DesktopEntry =>
  accelerator ? { native, label, accelerator } : { native, label };

function fileMenu(app: DesktopMenuContext): DesktopEntry[] {
  if (app === 'home')
    return [
      nat('new-write', S.blankDocument),
      nat('new-grid', S.blankWorkbook),
      nat('new-deck', S.blankPresentation),
      '-',
      'suite.file.open',
      '-',
      nat('close-window', S.closeWindow),
      nat('quit', S.quit),
    ];
  return [
    'suite.file.new',
    'suite.file.open',
    '-',
    'suite.file.save',
    'suite.file.save_as',
    '-',
    'suite.file.print',
    '-',
    'suite.file.close',
    nat('quit', S.quit),
  ];
}

function editMenu(app: Exclude<DesktopMenuContext, 'home'>): DesktopEntry[] {
  const selectAll = {
    write: 'write.caret.select_all',
    grid: 'grid.nav.select_all',
    deck: 'suite.edit.select_all',
  }[app];
  return [
    'suite.edit.undo',
    'suite.edit.redo',
    '-',
    'suite.edit.cut',
    'suite.edit.copy',
    'suite.edit.paste',
    'suite.edit.paste_special',
    '-',
    selectAll,
    '-',
    'suite.edit.find',
    'suite.edit.replace',
    ...(app === 'deck' ? [] : ['suite.edit.go_to']),
  ];
}

const VIEW_COMMON: DesktopEntry[] = [
  'ui.palette.open',
  'ui.ribbon.collapse',
  '-',
  nat('fullscreen', S.fullScreen, 'F11'),
];

const INSERT: Record<'write' | 'grid' | 'deck', DesktopEntry[]> = {
  write: [
    'write.insert.page_break',
    'write.insert.column_break',
    '-',
    'suite.insert.link',
    'suite.review.new_comment',
    '-',
    'suite.insert.date',
    'suite.insert.time',
  ],
  grid: ['suite.insert.link', 'grid.review.new_note', '-', 'grid.styles.format_as_table'],
  deck: [
    'deck.slides.new_slide',
    'deck.slides.duplicate',
    '-',
    'suite.insert.link',
    'suite.review.new_comment',
  ],
};

const FORMAT: Record<'write' | 'grid' | 'deck', DesktopEntry[]> = {
  write: [
    'suite.format.bold',
    'suite.format.italic',
    'suite.format.underline',
    '-',
    'suite.format.align_left',
    'suite.format.align_center',
    'suite.format.align_right',
    'suite.format.justify',
    '-',
    'suite.format.bullets',
    'suite.format.indent',
    'suite.format.outdent',
    '-',
    'suite.format.copy_formatting',
    'suite.format.paste_formatting',
    'write.paragraph.clear_character_formatting',
  ],
  grid: [
    'suite.format.bold',
    'suite.format.italic',
    'suite.format.underline',
    '-',
    'grid.cells.format_cells',
    'grid.number.number',
    'grid.number.currency',
    'grid.number.percent',
  ],
  deck: [
    'suite.format.bold',
    'suite.format.italic',
    'suite.format.underline',
    '-',
    'suite.format.align_left',
    'suite.format.align_center',
    'suite.format.align_right',
    'suite.format.justify',
    '-',
    'suite.format.bullets',
    '-',
    'suite.format.copy_formatting',
    'suite.format.paste_formatting',
  ],
};

const WINDOW: DesktopEntry[] = [nat('new-window', S.newWindow), nat('minimize', S.minimize)];

const HELP: DesktopEntry[] = [
  'ui.shortcuts.show',
  '-',
  nat('manual', S.manual),
  nat('report', S.report),
  '-',
  nat('about', S.about),
];

/** The menus of one context, as authored (titles are English source messages). */
export function desktopMenuSpec(app: DesktopMenuContext): { title: string; entries: DesktopEntry[] }[] {
  if (app === 'home')
    return [
      { title: S.file, entries: fileMenu('home') },
      { title: S.view, entries: [nat('fullscreen', S.fullScreen, 'F11')] },
      { title: S.window, entries: WINDOW },
      { title: S.help, entries: HELP.slice(2) },
    ];
  const menus: { title: string; entries: DesktopEntry[] }[] = [
    { title: S.file, entries: fileMenu(app) },
    { title: S.edit, entries: editMenu(app) },
    { title: S.view, entries: VIEW_COMMON },
    { title: S.insert, entries: INSERT[app] },
    { title: S.format, entries: FORMAT[app] },
  ];
  if (app === 'deck')
    menus.push({
      title: S.slideShow,
      entries: ['deck.show.from_beginning', 'deck.show.from_current', '-', 'deck.show.presenter_view'],
    });
  menus.push({ title: S.review, entries: ['suite.review.spelling'] });
  menus.push({ title: S.window, entries: WINDOW }, { title: S.help, entries: HELP });
  return menus;
}

/** Muda's names for the keys of a canonical chord key (`keys.ts`). */
const CODE_NAMES: Readonly<Record<string, string>> = {
  enter: 'Enter',
  space: 'Space',
  tab: 'Tab',
  backspace: 'Backspace',
  delete: 'Delete',
  insert: 'Insert',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
  arrowup: 'Up',
  arrowdown: 'Down',
  arrowleft: 'Left',
  arrowright: 'Right',
  '/': 'Slash',
  '\\': 'Backslash',
  ';': 'Semicolon',
  "'": 'Quote',
  '=': 'Equal',
  '-': 'Minus',
  ',': 'Comma',
  '.': 'Period',
  '[': 'BracketLeft',
  ']': 'BracketRight',
  '`': 'Backquote',
};

/**
 * A keymap shortcut as a menu accelerator (`Mod+Shift+S` → `CmdOrCtrl+Shift+S`), or `null` for one
 * a menu should not take: a bare modifier or key the editors need as typed (Esc, Alt for Key Tips,
 * plain letters), and shifted symbols, whose key differs between keyboard layouts.
 */
export function menuAccelerator(shortcut: string): string | null {
  const chord = tryParseShortcut(shortcut);
  if (!chord) return null;
  const mods = /\bmod\b/i.test(shortcut);
  const fkey = /^f([1-9]|1[0-9]|2[0-4])$/.test(chord.key);
  if (!fkey && !chord.ctrl && !chord.meta) return null;
  let code: string | undefined;
  if (fkey) code = chord.key.toUpperCase();
  else if (/^[a-z]$/.test(chord.key)) code = chord.key.toUpperCase();
  else if (/^[0-9]$/.test(chord.key)) code = chord.key;
  else code = CODE_NAMES[chord.key];
  if (!code) return null;
  // A shifted symbol (Ctrl+Shift+> is Ctrl+Shift+. on one layout and Ctrl+> on another).
  const last = shortcut.trim().split('+').pop() ?? '';
  if (last.length === 1 && last in SHIFTED_SYMBOLS) return null;
  const parts: string[] = [];
  if (mods) parts.push('CmdOrCtrl');
  else if (chord.ctrl) parts.push('Ctrl');
  if (chord.meta && !mods) parts.push('Super');
  if (chord.alt) parts.push('Alt');
  if (chord.shift) parts.push('Shift');
  parts.push(code);
  return parts.join('+');
}

/** The binding a menu shows for a command in an app: the app's own first, then the common one. */
export function menuBinding(app: AppId, command: string): KeyBinding | null {
  const layers = [DEFAULT_KEYMAP_LAYERS[app] ?? [], DEFAULT_KEYMAP_LAYERS.common ?? []];
  for (const layer of layers) {
    for (const b of layer) {
      if (b.command !== command || b.scope || b.onRelease) continue;
      if (menuAccelerator(b.key)) return b;
    }
  }
  return null;
}

/** A message in each interface language. */
export type Localized = Readonly<Record<string, string>>;

export interface DesktopMenuItemJson {
  /** `cmd:<command id>` (runs in the window's frame) or `native:<action>`. */
  id: string;
  label: Localized;
  accelerator: string | null;
  args?: unknown;
}

export type DesktopMenuEntryJson = DesktopMenuItemJson | { separator: true };

export interface DesktopMenuJson {
  '//': string;
  version: 1;
  locales: readonly string[];
  /** The shell's own messages, by key of {@link DESKTOP_STRINGS}. */
  strings: Readonly<Record<string, Localized>>;
  menus: Readonly<Record<DesktopMenuContext, readonly { title: Localized; items: DesktopMenuEntryJson[] }[]>>;
}

/** Messages of every interface language: locale → key → message. */
export type LocaleTables = Readonly<Record<string, Readonly<Record<string, string>>>>;

/**
 * The menus with their labels in every language of `locales` (`en` is the source: a key it
 * lacks is an error, a translation another locale lacks falls back to English).
 */
export function buildDesktopMenu(locales: LocaleTables): DesktopMenuJson {
  const names = Object.keys(locales);
  const en = locales.en;
  if (!en) throw new Error('the English locale is missing');
  const localize = (key: string): Localized => {
    const source = en[key];
    if (source === undefined) throw new Error(`no English message for "${key}"`);
    return Object.fromEntries(names.map((l) => [l, locales[l]?.[key] ?? source]));
  };
  const declared = new Map(COMMAND_CATALOG.map((d) => [d.id, d]));
  const item = (app: DesktopMenuContext, entry: DesktopEntry): DesktopMenuEntryJson => {
    if (entry === '-') return { separator: true };
    if (typeof entry === 'object' && 'native' in entry)
      return {
        id: `native:${entry.native}`,
        label: localize(entry.label),
        accelerator: entry.accelerator ?? null,
      };
    const command = typeof entry === 'string' ? entry : entry.command;
    const decl = declared.get(command);
    if (!decl) throw new Error(`the desktop menu names "${command}", which the catalog does not declare`);
    if (decl.tier && decl.tier !== 'P0')
      throw new Error(`"${command}" is ${decl.tier}: release builds hide it`);
    const binding = app === 'home' ? null : menuBinding(app, command);
    const args = typeof entry === 'object' && 'args' in entry ? entry.args : binding?.args;
    return {
      id: `cmd:${command}`,
      label: localize(`cmd.${command}`),
      accelerator: binding ? menuAccelerator(binding.key) : null,
      ...(args === undefined ? {} : { args }),
    };
  };
  const menus = Object.fromEntries(
    DESKTOP_MENU_CONTEXTS.map((app) => [
      app,
      desktopMenuSpec(app).map((m) => ({
        title: localize(m.title),
        items: m.entries.map((e) => item(app, e)),
      })),
    ]),
  ) as unknown as DesktopMenuJson['menus'];
  const strings = Object.fromEntries(Object.entries(DESKTOP_STRINGS).map(([k, v]) => [k, localize(v)]));
  return {
    '//': 'generated by `pnpm --filter @kiln/commands desktop:menu` from the command catalog, the keymaps and the locales; do not edit',
    version: 1,
    locales: names,
    strings,
    menus,
  };
}
