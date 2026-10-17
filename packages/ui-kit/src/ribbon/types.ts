import type { AppId, CommandContext, Tier } from '@kiln/commands';
import type { ReactNode } from 'react';
import type { MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import type { ComboboxOption } from '../primitives/Combobox.tsx';
import type { GalleryItem } from '../primitives/Gallery.tsx';

export type { MenuEntry };

/**
 * Size in the Classic ribbon: `large` (32 px icon above the label, full group height), `medium`
 * (icon + label in one row), `small` (icon only, label in the tooltip). The Simplified ribbon
 * shows `large` items as `medium` and keeps the rest.
 */
export type RibbonItemSize = 'large' | 'medium' | 'small';

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
  /** Feature tier; P1/P2 items are hidden in release builds unless their tier is enabled. */
  tier?: Tier;
}

export interface RibbonButtonItem extends ItemBase {
  kind: 'button';
  command: string;
  args?: unknown;
}

/**
 * Two-state command (Bold, Ruler); pressed state comes from the command's `isChecked`, or, with
 * `checkedValue`, from the command's value (one view of `suite.view.mode`).
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
  menu: readonly MenuEntry[];
}

/** Button that only opens a menu (Margins ▾, Orientation ▾). `command` supplies label/icon/state. */
export interface RibbonDropdownItem extends ItemBase {
  kind: 'dropdown';
  command?: string;
  menu: readonly MenuEntry[];
}

/** In-ribbon gallery (Styles, Shapes, Transitions). Selecting runs `command` with `{ value: id }`. */
export interface RibbonGalleryItem extends ItemBase {
  kind: 'gallery';
  command: string;
  items: readonly GalleryItem[];
  /** Items per in-ribbon row. */
  columns?: number;
  itemWidth?: number;
  itemHeight?: number;
  /** Live preview: runs with `{ value: id }` on hover and `{ value: null }` when it ends. */
  previewCommand?: string;
  /** Extra menu under the expanded gallery ("Create a Style", "Clear Formatting"). */
  footer?: readonly MenuEntry[];
}

/** Editable list (Font, Font Size, Number Format). Value from the command's `value`. */
export interface RibbonComboboxItem extends ItemBase {
  kind: 'combobox';
  command: string;
  options: readonly ComboboxOption[];
  width?: number;
  /** Shown dimmed while the core reports no value ("Font", "Size"). */
  placeholder?: string;
  allowCustomValue?: boolean;
  /** Validate typed text; return null to reject. */
  parse?: (text: string) => string | null;
}

/** Colour split button (Font Color, Highlight, Fill). Runs `command` with `{ color: ColorChoice }`. */
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

/** Anything else (an app-specific control); still navigable when it renders focusable elements. */
export interface RibbonCustomItem extends ItemBase {
  kind: 'custom';
  id: string;
  label: string;
  render: (ctx: { layout: 'classic' | 'simplified'; context: CommandContext }) => ReactNode;
}

/** A horizontal cluster that takes one row of a Classic group (Office's Font group rows). */
export interface RibbonRowItem {
  kind: 'row';
  id?: string;
  items: readonly RibbonLeafItem[];
}

export type RibbonLeafItem =
  | RibbonButtonItem
  | RibbonToggleItem
  | RibbonSplitItem
  | RibbonDropdownItem
  | RibbonGalleryItem
  | RibbonComboboxItem
  | RibbonColorItem
  | RibbonNumberInputItem
  | RibbonSpacerItem
  | RibbonCustomItem;

export type RibbonItem = RibbonLeafItem | RibbonRowItem;

export interface RibbonGroup {
  id: string;
  label: string;
  /** Dialog launcher ↘ in the group corner (Font…, Paragraph…, Page Setup…). */
  launcher?: { command: string; label?: string };
  items: readonly RibbonItem[];
  /** Rows of a Classic group: 3 (default) or 2 (Font, Paragraph). */
  rows?: 2 | 3;
  /** Icon shown when the group collapses to one button for lack of space. */
  icon?: string;
  /** Higher priority groups collapse last. Default 0. */
  priority?: number;
  tier?: Tier;
}

export interface RibbonTab {
  id: string;
  label: string;
  /** Office Key Tip letters (Home = H). */
  keyTip?: string;
  groups: readonly RibbonGroup[];
  /** Contextual tab set this tab belongs to (Table Design + Layout form "Table Tools"). */
  contextual?: { set: string; label: string };
  /** Shown only while true (contextual tabs: a table is selected). Re-evaluated on registry changes. */
  when?: (ctx: CommandContext) => boolean;
  tier?: Tier;
}

export interface RibbonSpec {
  app: AppId;
  tabs: readonly RibbonTab[];
}

export type RibbonLayout = 'classic' | 'simplified';
