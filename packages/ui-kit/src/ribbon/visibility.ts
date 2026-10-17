/**
 * What of a ribbon spec this build actually shows. Items whose command is hidden (tier above the
 * build tier, or still only declared in a release build, PLAN.md §1.4) disappear; a row, group or
 * tab left with nothing to show disappears with them, so a release build never ships empty group
 * slabs or tabs that open onto nothing.
 */
import { type CommandRegistry, isTierEnabled } from '@kiln/commands';
import type { MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import type { RibbonGroup, RibbonItem, RibbonLeafItem, RibbonSpec, RibbonTab } from './types.ts';

type Registry = Pick<CommandRegistry, 'getState' | 'tiers'>;

function commandVisible(registry: Registry, id: string): boolean {
  return registry.getState(id).visible;
}

/** Whether a menu shows at least one entry (headings and separators alone do not count). */
export function menuHasContent(registry: Registry, entries: readonly MenuEntry[]): boolean {
  return entries.some((e) => {
    if ('command' in e) return commandVisible(registry, e.command);
    if (e.kind === 'submenu') return menuHasContent(registry, e.items);
    return e.kind === 'custom';
  });
}

/** Whether a leaf control renders in this build (spacers never count as content). */
export function isLeafVisible(registry: Registry, it: RibbonLeafItem): boolean {
  if (it.kind === 'spacer') return false;
  if (it.tier && !isTierEnabled(it.tier, registry.tiers)) return false;
  if (it.kind === 'custom') return true;
  // A dropdown only opens its menu (its command lends the label and icon): it shows while any
  // entry of the menu does (Select ▾ with only Select All implemented yet).
  if (it.kind === 'dropdown') {
    return (!!it.command && commandVisible(registry, it.command)) || menuHasContent(registry, it.menu);
  }
  return commandVisible(registry, it.command);
}

/** Drops leading, trailing and doubled spacers left behind once hidden items are removed. */
function tidySpacers(items: RibbonItem[]): RibbonItem[] {
  const out: RibbonItem[] = [];
  for (const it of items) {
    if (it.kind === 'spacer' && (out.length === 0 || out[out.length - 1]?.kind === 'spacer')) continue;
    out.push(it);
  }
  while (out.length && out[out.length - 1]?.kind === 'spacer') out.pop();
  return out;
}

/** The group with only its visible items, or null when nothing in it would show. */
export function visibleGroup(registry: Registry, g: RibbonGroup): RibbonGroup | null {
  if (g.tier && !isTierEnabled(g.tier, registry.tiers)) return null;
  const items: RibbonItem[] = [];
  let changed = false;
  for (const it of g.items) {
    if (it.kind === 'row') {
      const leaves = it.items.filter((l) => isLeafVisible(registry, l));
      if (leaves.length === 0) {
        changed = true;
        continue;
      }
      if (leaves.length !== it.items.length) changed = true;
      items.push(leaves.length === it.items.length ? it : { ...it, items: leaves });
    } else if (it.kind === 'spacer' || isLeafVisible(registry, it)) {
      items.push(it);
    } else {
      changed = true;
    }
  }
  const tidy = tidySpacers(items);
  if (!tidy.some((it) => it.kind !== 'spacer')) return null;
  if (tidy.length !== items.length) changed = true;
  const launcherHidden = !!g.launcher && !commandVisible(registry, g.launcher.command);
  if (!changed && !launcherHidden) return g;
  const { launcher, ...rest } = g;
  return { ...rest, ...(launcher && !launcherHidden ? { launcher } : {}), items: promoteLone(tidy) };
}

const PROMOTABLE = new Set(['button', 'toggle', 'split', 'dropdown']);

/**
 * A group thinned down to one small or medium command would leave it floating in the slab's top
 * corner; it takes the full group height instead, as Office lays out one-command groups.
 */
function promoteLone(items: RibbonItem[]): RibbonItem[] {
  const [only] = items;
  if (items.length !== 1 || !only || !PROMOTABLE.has(only.kind)) return items;
  const leaf = only as Exclude<RibbonLeafItem, { kind: 'spacer' }>;
  return leaf.size === 'large' ? items : [{ ...leaf, size: 'large' } as RibbonLeafItem];
}

/** The tab with only its visible groups, or null when the tab would open onto an empty slab. */
export function visibleTab(registry: Registry, t: RibbonTab): RibbonTab | null {
  if (t.tier && !isTierEnabled(t.tier, registry.tiers)) return null;
  const groups = t.groups.map((g) => visibleGroup(registry, g)).filter((g): g is RibbonGroup => g !== null);
  if (groups.length === 0) return null;
  const same = groups.length === t.groups.length && groups.every((g, i) => g === t.groups[i]);
  return same ? t : { ...t, groups };
}

/** The spec as this build shows it (tab `when` conditions are left to the caller). */
export function visibleSpec(registry: Registry, spec: RibbonSpec): RibbonSpec {
  return { ...spec, tabs: spec.tabs.map((t) => visibleTab(registry, t)).filter((t): t is RibbonTab => !!t) };
}
