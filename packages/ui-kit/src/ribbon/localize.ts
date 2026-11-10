/**
 * A ribbon spec in the interface language: tab, group, item, menu, gallery and list labels
 * are messages kept as English data (`@kiln/commands` ribbon definitions, `msg` in chart tabs);
 * labels the core supplies (a presentation's layout names) are not messages and stay as they are.
 * Command titles come translated from the registry.
 */
import { msg, SOURCE_MESSAGES, t, tx } from '@kiln/i18n';
import type { MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import type { RibbonGroup, RibbonItem, RibbonLeafItem, RibbonSpec, RibbonTab } from './types.ts';

/**
 * A combo box's placeholder fits a narrow box, so it may have its own shorter message
 * (`ribbon.placeholder.Size`: the font size box), not the word the group labels use.
 */
const PLACEHOLDERS: readonly string[] = [msg('ribbon.placeholder.Size')];

function placeholder(text: string): string {
  const key = `ribbon.placeholder.${text}`;
  return PLACEHOLDERS.includes(key) && key in SOURCE_MESSAGES ? t(key) : tx(text);
}

function entry(e: MenuEntry): MenuEntry {
  switch (e.kind) {
    case 'separator':
    case 'custom':
      return e;
    case 'heading':
      return { ...e, label: tx(e.label) };
    case 'submenu':
      return { ...e, label: tx(e.label), items: e.items.map(entry) };
    default:
      return {
        ...e,
        ...(e.label ? { label: tx(e.label) } : {}),
        ...(e.description ? { description: tx(e.description) } : {}),
      };
  }
}

function leaf(item: RibbonLeafItem): RibbonLeafItem {
  if (item.kind === 'spacer') return item;
  const label = 'label' in item && item.label ? { label: tx(item.label) } : {};
  switch (item.kind) {
    case 'split':
    case 'dropdown':
      return { ...item, ...label, menu: item.menu.map(entry) };
    case 'gallery':
      return {
        ...item,
        ...label,
        items: item.items.map((g) => ({ ...g, label: tx(g.label) })),
        ...(item.footer ? { footer: item.footer.map(entry) } : {}),
      };
    case 'combobox':
      return {
        ...item,
        ...label,
        ...(item.placeholder ? { placeholder: placeholder(item.placeholder) } : {}),
        options: item.options.map((o) => ({
          ...o,
          ...(o.label ? { label: tx(o.label) } : {}),
          ...(o.group ? { group: tx(o.group) } : {}),
          ...(typeof o.hint === 'string' ? { hint: tx(o.hint) } : {}),
        })),
      };
    default:
      return { ...item, ...label } as RibbonLeafItem;
  }
}

function item(it: RibbonItem): RibbonItem {
  return it.kind === 'row' ? { ...it, items: it.items.map(leaf) } : leaf(it);
}

function group(g: RibbonGroup): RibbonGroup {
  return {
    ...g,
    label: tx(g.label),
    ...(g.launcher?.label ? { launcher: { ...g.launcher, label: tx(g.launcher.label) } } : {}),
    items: g.items.map(item),
  };
}

function tab(tb: RibbonTab): RibbonTab {
  return {
    ...tb,
    label: tx(tb.label),
    ...(tb.contextual ? { contextual: { ...tb.contextual, label: tx(tb.contextual.label) } } : {}),
    groups: tb.groups.map(group),
  };
}

/** The spec with every label in the interface language (the same spec in English). */
export function localizeRibbon(spec: RibbonSpec, language: string): RibbonSpec {
  if (language === 'en') return spec;
  return { ...spec, tabs: spec.tabs.map(tab) };
}
