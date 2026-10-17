/**
 * Terse builders for ribbon specs, so app ribbons read like PLAN.md §1.4:
 *
 *   tab('home', 'Home', [
 *     group('clipboard', 'Clipboard', [split('suite.edit.paste', pasteMenu, { size: 'large' }), button('suite.edit.cut'), …]),
 *   ], { keyTip: 'H' })
 */
import type { AppId } from '@kiln/commands';
import type {
  MenuEntry,
  RibbonButtonItem,
  RibbonColorItem,
  RibbonComboboxItem,
  RibbonCustomItem,
  RibbonDropdownItem,
  RibbonGalleryItem,
  RibbonGroup,
  RibbonItem,
  RibbonLeafItem,
  RibbonNumberInputItem,
  RibbonRowItem,
  RibbonSpacerItem,
  RibbonSpec,
  RibbonSplitItem,
  RibbonTab,
  RibbonToggleItem,
} from './types.ts';

type Opts<T> = Partial<Omit<T, 'kind'>>;

export const ribbon = (app: AppId, tabs: readonly RibbonTab[]): RibbonSpec => ({ app, tabs });

export const tab = (
  id: string,
  label: string,
  groups: readonly RibbonGroup[],
  opts: Opts<RibbonTab> = {},
): RibbonTab => ({
  id,
  label,
  groups,
  ...opts,
});

export const group = (
  id: string,
  label: string,
  items: readonly RibbonItem[],
  opts: Opts<RibbonGroup> = {},
): RibbonGroup => ({ id, label, items, ...opts });

export const button = (command: string, opts: Opts<RibbonButtonItem> = {}): RibbonButtonItem => ({
  kind: 'button',
  command,
  ...opts,
});

export const toggle = (command: string, opts: Opts<RibbonToggleItem> = {}): RibbonToggleItem => ({
  kind: 'toggle',
  command,
  ...opts,
});

export const split = (
  command: string,
  menu: readonly MenuEntry[],
  opts: Opts<RibbonSplitItem> = {},
): RibbonSplitItem => ({ kind: 'split', command, menu, ...opts });

export const dropdown = (
  command: string | undefined,
  menu: readonly MenuEntry[],
  opts: Opts<RibbonDropdownItem> = {},
): RibbonDropdownItem => ({ kind: 'dropdown', ...(command ? { command } : {}), menu, ...opts });

export const gallery = (
  command: string,
  items: RibbonGalleryItem['items'],
  opts: Opts<RibbonGalleryItem> = {},
): RibbonGalleryItem => ({ kind: 'gallery', command, items, ...opts });

export const combobox = (
  command: string,
  options: RibbonComboboxItem['options'],
  opts: Opts<RibbonComboboxItem> = {},
): RibbonComboboxItem => ({ kind: 'combobox', command, options, ...opts });

export const color = (command: string, opts: Opts<RibbonColorItem> = {}): RibbonColorItem => ({
  kind: 'color',
  command,
  ...opts,
});

export const numberInput = (
  command: string,
  opts: Opts<RibbonNumberInputItem> = {},
): RibbonNumberInputItem => ({
  kind: 'numberinput',
  command,
  ...opts,
});

export const spacer = (id?: string): RibbonSpacerItem => ({ kind: 'spacer', ...(id ? { id } : {}) });

export const custom = (
  id: string,
  label: string,
  render: RibbonCustomItem['render'],
  opts: Opts<RibbonCustomItem> = {},
): RibbonCustomItem => ({
  kind: 'custom',
  id,
  label,
  render,
  ...opts,
});

export const row = (...items: RibbonLeafItem[]): RibbonRowItem => ({ kind: 'row', items });

// Menu entries.
export const item = (
  command: string,
  opts: Omit<Extract<MenuEntry, { command: string }>, 'command' | 'kind'> = {},
): MenuEntry => ({
  command,
  ...opts,
});
export const separator = (): MenuEntry => ({ kind: 'separator' });
export const heading = (label: string): MenuEntry => ({ kind: 'heading', label });
export const submenu = (label: string, items: readonly MenuEntry[], icon?: string): MenuEntry => ({
  kind: 'submenu',
  label,
  items,
  ...(icon ? { icon } : {}),
});

/** Stable id of an item (explicit id, else its command, else its kind + index). */
export function itemId(it: RibbonItem, index: number): string {
  if ('id' in it && it.id) return it.id;
  if ('command' in it && it.command) return it.command;
  return `${it.kind}-${index}`;
}

/** Every leaf item of a group, rows flattened. */
export function leafItems(g: RibbonGroup): RibbonLeafItem[] {
  return g.items.flatMap((it) => (it.kind === 'row' ? [...it.items] : [it]));
}
