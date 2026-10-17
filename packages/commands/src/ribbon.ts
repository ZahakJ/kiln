/**
 * Ribbon definitions as data (PLAN.md §1.4): tabs, groups, group labels, dialog launchers, galleries,
 * split buttons, drop-downs and contextual tabs. Every item names a command id from the catalog, so
 * the ribbon is a view of the command registry: an item whose command is hidden in this build (a
 * higher tier, or a command the core does not implement yet in a release build) is not shown, never
 * greyed out. `@kiln/ui-kit` renders these specs; apps may add `custom` items there.
 */
import type { AppId, Tier } from './types.ts';

/**
 * Size in the Classic ribbon: `large` (32 px icon above the label, full group height), `medium`
 * (icon + label in one row), `small` (icon only, label in the tooltip). The Simplified ribbon
 * shows `large` items as `medium` and keeps the rest.
 */
export type RibbonItemSize = 'large' | 'medium' | 'small';

/** An entry of a ribbon drop-down, split-button menu or gallery footer. */
export type RibbonMenuEntry =
  | {
      kind?: 'command';
      command: string;
      args?: unknown;
      /** Overrides the command title ("Keep Text Only" for `suite.edit.paste_text_only`). */
      label?: string;
      icon?: string;
      description?: string;
    }
  | { kind: 'separator' }
  | { kind: 'heading'; label: string }
  | { kind: 'submenu'; label: string; icon?: string; items: readonly RibbonMenuEntry[] };

/** Data for a gallery tile: a label and a typographic preview (no markup in data). */
export interface RibbonGalleryEntry {
  id: string;
  /** Accessible name and tooltip ("Heading 1"). */
  label: string;
  /** Preview text ("AaBbCc"); defaults to the label. */
  sample?: string;
  /** Presentation hints for the preview, mapped to CSS by ui-kit. */
  look?: {
    font?: 'ui' | 'display' | 'serif' | 'mono';
    size?: number;
    weight?: number;
    italic?: boolean;
    underline?: boolean;
    caps?: boolean;
    /** A token-coloured accent (the app's text colour). */
    accent?: boolean;
    muted?: boolean;
    /** A swatch shown instead of text (themes, colours): CSS colours, light then dark. */
    swatch?: readonly string[];
  };
}

/** An option of an editable list (Font, Font Size, Number Format). */
export interface RibbonComboOption {
  value: string;
  label?: string;
  group?: string;
  /** Show the option in this font (font lists). */
  fontFamily?: string;
  /** Right-aligned hint ("(Headings)"). */
  hint?: string;
}

interface ItemBase {
  /** Stable id (defaults to the command id); used for keys, Key Tips and overflow measurement. */
  id?: string;
  /** Overrides the command title. */
  label?: string;
  /** Overrides the command icon (glyph or Lucide name). */
  icon?: string;
  /** Office Key Tip letters within the tab (Bold = 1). Defaults to the command's `keyTip`. */
  keyTip?: string;
  size?: RibbonItemSize;
  /** Keep in the Simplified ribbon even when space is short (overflowed last). */
  priority?: number;
  /** Hide in the Simplified ribbon (Office drops some rarely used items there). */
  hideInSimplified?: boolean;
  /** Feature tier; P1/P2 items are hidden unless their tier is enabled. */
  tier?: Tier;
}

export interface RibbonButtonItem extends ItemBase {
  kind: 'button';
  command: string;
  args?: unknown;
}

/**
 * Two-state command (Bold, Ruler). Pressed when the command's state is checked, or, for one choice
 * of a value command (a view of `suite.view.mode`), when the command's value equals `checkedValue`.
 */
export interface RibbonToggleItem extends ItemBase {
  kind: 'toggle';
  command: string;
  args?: unknown;
  checkedValue?: unknown;
}

/** Main part runs `command`; the arrow opens `menu` (Paste ▾, Bullets ▾). */
export interface RibbonSplitItem extends ItemBase {
  kind: 'split';
  command: string;
  args?: unknown;
  menu: readonly RibbonMenuEntry[];
}

/** Button that only opens a menu (Margins ▾, Orientation ▾). `command` supplies label/icon/state. */
export interface RibbonDropdownItem extends ItemBase {
  kind: 'dropdown';
  command?: string;
  menu: readonly RibbonMenuEntry[];
}

/** In-ribbon gallery (Styles, Shapes, Transitions). Selecting runs `command` with `{ value: id }`. */
export interface RibbonGalleryItem extends ItemBase {
  kind: 'gallery';
  command: string;
  items: readonly RibbonGalleryEntry[];
  /** Items per in-ribbon row. */
  columns?: number;
  itemWidth?: number;
  itemHeight?: number;
  /** Live preview: runs with `{ value: id }` on hover and `{ value: null }` when it ends. */
  previewCommand?: string;
  /** Extra menu under the expanded gallery ("Create a Style", "Clear Formatting"). */
  footer?: readonly RibbonMenuEntry[];
}

/** Editable list (Font, Font Size, Number Format). The current value comes from the command's state. */
export interface RibbonComboboxItem extends ItemBase {
  kind: 'combobox';
  command: string;
  options: readonly RibbonComboOption[];
  width?: number;
  /** Shown dimmed while the core reports no value ("Font", "Size"). */
  placeholder?: string;
  allowCustomValue?: boolean;
  /** Accepted typed input: `number` (font sizes), else any text. */
  accepts?: 'number' | 'text';
}

/** Colour split button (Font Color, Highlight, Fill). Runs `command` with `{ color }`. */
export interface RibbonColorItem extends ItemBase {
  kind: 'color';
  command: string;
  allowAuto?: boolean;
  allowNone?: boolean;
  /** Initial colour of the main part (Office: red for font colour, yellow for highlight). */
  defaultColor?: string;
}

/** Spin box (Spacing Before, Shape Height). Runs `command` with `{ value: number }`. */
export interface RibbonNumberInputItem extends ItemBase {
  kind: 'numberinput';
  command: string;
  unit?: 'pt' | 'px' | 'cm' | 'mm' | 'in' | '%' | '°' | '';
  min?: number;
  max?: number;
  step?: number;
  width?: number;
}

/** Thin separator between clusters. */
export interface RibbonSpacerItem {
  kind: 'spacer';
  id?: string;
}

export type RibbonDataLeaf =
  | RibbonButtonItem
  | RibbonToggleItem
  | RibbonSplitItem
  | RibbonDropdownItem
  | RibbonGalleryItem
  | RibbonComboboxItem
  | RibbonColorItem
  | RibbonNumberInputItem
  | RibbonSpacerItem;

/** A horizontal cluster that takes one row of a Classic group (Office's Font group rows). */
export interface RibbonDataRow<Leaf = RibbonDataLeaf> {
  kind: 'row';
  id?: string;
  items: readonly Leaf[];
}

export interface RibbonDataGroup<Item = RibbonDataLeaf | RibbonDataRow> {
  id: string;
  label: string;
  /** Dialog launcher ↘ in the group corner (Font…, Paragraph…, Page Setup…). */
  launcher?: { command: string; label?: string };
  items: readonly Item[];
  /** Rows of a Classic group: 3 (default) or 2 (Font, Paragraph). */
  rows?: 2 | 3;
  /** Icon shown when the group collapses to one button for lack of space. */
  icon?: string;
  /** Higher priority groups collapse last. Default 0. */
  priority?: number;
  tier?: Tier;
}

export interface RibbonDataTab<Group = RibbonDataGroup> {
  id: string;
  label: string;
  /** Office Key Tip letters (Home = H). */
  keyTip?: string;
  groups: readonly Group[];
  /**
   * Contextual tab set this tab belongs to (Table Design + Layout form "Table Tools"). It is shown
   * while the core reports that context (the selection is in a table); `context` names it.
   */
  contextual?: { set: string; label: string; context: string };
  tier?: Tier;
}

export interface RibbonData {
  app: AppId;
  tabs: readonly RibbonDataTab[];
}

// ── Builders: app ribbons read like PLAN.md §1.4 ────────────────────────────────────────────────

type Opts<T> = Partial<Omit<T, 'kind'>>;

export const rtab = (
  id: string,
  label: string,
  groups: readonly RibbonDataGroup[],
  opts: Opts<RibbonDataTab> = {},
): RibbonDataTab => ({ id, label, groups, ...opts });

export const rgroup = (
  id: string,
  label: string,
  items: readonly (RibbonDataLeaf | RibbonDataRow)[],
  opts: Opts<RibbonDataGroup> = {},
): RibbonDataGroup => ({ id, label, items, ...opts });

export const rbutton = (command: string, opts: Opts<RibbonButtonItem> = {}): RibbonButtonItem => ({
  kind: 'button',
  command,
  ...opts,
});

export const rtoggle = (command: string, opts: Opts<RibbonToggleItem> = {}): RibbonToggleItem => ({
  kind: 'toggle',
  command,
  ...opts,
});

export const rsplit = (
  command: string,
  menu: readonly RibbonMenuEntry[],
  opts: Opts<RibbonSplitItem> = {},
): RibbonSplitItem => ({ kind: 'split', command, menu, ...opts });

export const rdropdown = (
  command: string | undefined,
  menu: readonly RibbonMenuEntry[],
  opts: Opts<RibbonDropdownItem> = {},
): RibbonDropdownItem => ({ kind: 'dropdown', ...(command ? { command } : {}), menu, ...opts });

export const rgallery = (
  command: string,
  items: readonly RibbonGalleryEntry[],
  opts: Opts<RibbonGalleryItem> = {},
): RibbonGalleryItem => ({ kind: 'gallery', command, items, ...opts });

export const rcombo = (
  command: string,
  options: readonly RibbonComboOption[],
  opts: Opts<RibbonComboboxItem> = {},
): RibbonComboboxItem => ({ kind: 'combobox', command, options, ...opts });

export const rcolor = (command: string, opts: Opts<RibbonColorItem> = {}): RibbonColorItem => ({
  kind: 'color',
  command,
  ...opts,
});

export const rnumber = (command: string, opts: Opts<RibbonNumberInputItem> = {}): RibbonNumberInputItem => ({
  kind: 'numberinput',
  command,
  ...opts,
});

export const rspacer = (id?: string): RibbonSpacerItem => ({ kind: 'spacer', ...(id ? { id } : {}) });

export const rrow = (...items: RibbonDataLeaf[]): RibbonDataRow => ({ kind: 'row', items });

export const mitem = (
  command: string,
  opts: Omit<Extract<RibbonMenuEntry, { command: string }>, 'command' | 'kind'> = {},
): RibbonMenuEntry => ({ command, ...opts });
export const msep = (): RibbonMenuEntry => ({ kind: 'separator' });
export const mhead = (label: string): RibbonMenuEntry => ({ kind: 'heading', label });
export const msub = (label: string, items: readonly RibbonMenuEntry[], icon?: string): RibbonMenuEntry => ({
  kind: 'submenu',
  label,
  items,
  ...(icon ? { icon } : {}),
});

// ── Walking specs (tests, Key Tips, the e2e) ────────────────────────────────────────────────────

/** A command reference found in a ribbon spec, with where it was found. */
export interface RibbonCommandRef {
  command: string;
  args?: unknown;
  tab: string;
  group: string;
  /** `item`, `menu`, `footer` or `launcher`. */
  via: 'item' | 'menu' | 'footer' | 'launcher';
}

function menuRefs(
  entries: readonly RibbonMenuEntry[],
  at: Omit<RibbonCommandRef, 'command' | 'via'>,
  via: 'menu' | 'footer',
): RibbonCommandRef[] {
  return entries.flatMap((e): RibbonCommandRef[] => {
    if ('command' in e) return [{ command: e.command, args: e.args, ...at, via }];
    if (e.kind === 'submenu') return menuRefs(e.items, at, via);
    return [];
  });
}

/** Every command a ribbon refers to (items, menus, gallery footers, dialog launchers). */
export function ribbonCommandRefs(spec: RibbonData): RibbonCommandRef[] {
  const out: RibbonCommandRef[] = [];
  for (const t of spec.tabs) {
    for (const g of t.groups) {
      const at = { tab: t.id, group: g.id };
      if (g.launcher) out.push({ command: g.launcher.command, ...at, via: 'launcher' });
      const leaves = g.items.flatMap((it) => (it.kind === 'row' ? [...it.items] : [it]));
      for (const it of leaves) {
        if (it.kind === 'spacer') continue;
        if ('command' in it && it.command)
          out.push({ command: it.command, args: 'args' in it ? it.args : undefined, ...at, via: 'item' });
        if (it.kind === 'split' || it.kind === 'dropdown') out.push(...menuRefs(it.menu, at, 'menu'));
        if (it.kind === 'gallery' && it.footer) out.push(...menuRefs(it.footer, at, 'footer'));
      }
    }
  }
  return out;
}
