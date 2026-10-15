import { type CommandRegistry, normalizeKeyTip } from '@officina/commands';
import { hasCommandIcon } from '../chrome/commandIcons.tsx';
import { isCommandEntry, type MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import { itemId, leafItems } from './builders.ts';
import type { RibbonSpec } from './types.ts';

export interface RibbonSpecIssue {
  /** `Home ▸ Font ▸ format.bold` */
  path: string;
  message: string;
}

export interface ValidateRibbonSpecOptions {
  /** Icon resolver; default: Officina glyphs + curated Lucide icons + registered extras. */
  hasIcon?: (name: string) => boolean;
}

/**
 * Checks a ribbon spec against the registry, for tests (`expect(validateRibbonSpec(spec, reg)).toEqual([])`):
 * every command id (items, menus, launchers) exists in the registry; every icon (explicit or from the
 * command) resolves; tab, group and item ids are unique; Key Tips are well formed and unique per level.
 */
export function validateRibbonSpec(
  spec: RibbonSpec,
  registry: CommandRegistry,
  options: ValidateRibbonSpecOptions = {},
): RibbonSpecIssue[] {
  const hasIcon = options.hasIcon ?? hasCommandIcon;
  const issues: RibbonSpecIssue[] = [];
  const add = (path: string, message: string) => issues.push({ path, message });

  const checkCommand = (
    path: string,
    command: string | undefined,
    icon: string | undefined,
    needsIcon: boolean,
  ) => {
    if (command !== undefined && !registry.has(command)) add(path, `unknown command "${command}"`);
    const name = icon ?? (command ? registry.get(command)?.icon : undefined);
    if (name !== undefined && !hasIcon(name)) add(path, `unknown icon "${name}"`);
    if (needsIcon && name === undefined) add(path, 'no icon (set one on the item or the command)');
  };

  const checkMenu = (path: string, entries: readonly MenuEntry[]) => {
    for (const e of entries) {
      if (isCommandEntry(e)) checkCommand(`${path} ▸ ${e.command}`, e.command, e.icon, false);
      else if (e.kind === 'submenu') {
        if (e.icon && !hasIcon(e.icon)) add(`${path} ▸ ${e.label}`, `unknown icon "${e.icon}"`);
        checkMenu(`${path} ▸ ${e.label}`, e.items);
      }
    }
  };

  const tabIds = new Set<string>();
  const tabTips = new Map<string, string>();
  for (const t of spec.tabs) {
    if (tabIds.has(t.id)) add(t.label, `duplicate tab id "${t.id}"`);
    tabIds.add(t.id);
    if (t.keyTip !== undefined) {
      const tip = normalizeKeyTip(t.keyTip);
      if (!tip) add(t.label, `invalid Key Tip "${t.keyTip}"`);
      else if (!t.when && tabTips.has(tip)) add(t.label, `Key Tip "${tip}" also used by ${tabTips.get(tip)}`);
      else if (!t.when) tabTips.set(tip, t.label);
    }
    const groupIds = new Set<string>();
    const itemIds = new Set<string>();
    const itemTips = new Map<string, string>();
    for (const g of t.groups) {
      const gPath = `${t.label} ▸ ${g.label}`;
      if (groupIds.has(g.id)) add(gPath, `duplicate group id "${g.id}"`);
      groupIds.add(g.id);
      if (g.icon && !hasIcon(g.icon)) add(gPath, `unknown icon "${g.icon}"`);
      if (g.launcher) checkCommand(`${gPath} ▸ launcher`, g.launcher.command, undefined, false);
      leafItems(g).forEach((it, i) => {
        if (it.kind === 'spacer') return;
        const id = itemId(it, i);
        const path = `${gPath} ▸ ${id}`;
        if (itemIds.has(id)) add(path, `duplicate item id "${id}" in tab "${t.label}"`);
        itemIds.add(id);
        if (it.kind === 'custom') return;
        const command = 'command' in it ? it.command : undefined;
        const needsIcon =
          it.kind === 'button' || it.kind === 'toggle' || it.kind === 'split' || it.kind === 'color';
        checkCommand(path, command, it.icon, needsIcon);
        if ('menu' in it) checkMenu(path, it.menu);
        if (it.kind === 'gallery' && it.footer) checkMenu(path, it.footer);
        if (it.kind === 'gallery' && it.previewCommand)
          checkCommand(`${path} ▸ preview`, it.previewCommand, undefined, false);
        if (it.keyTip !== undefined) {
          const tip = normalizeKeyTip(it.keyTip);
          if (!tip) add(path, `invalid Key Tip "${it.keyTip}"`);
          else if (itemTips.has(tip)) add(path, `Key Tip "${tip}" also used by ${itemTips.get(tip)}`);
          else itemTips.set(tip, id);
        }
      });
    }
  }
  return issues;
}

/** Throws with every issue listed (for app start-up in development). */
export function assertValidRibbonSpec(
  spec: RibbonSpec,
  registry: CommandRegistry,
  options?: ValidateRibbonSpecOptions,
): void {
  const issues = validateRibbonSpec(spec, registry, options);
  if (issues.length) {
    throw new Error(
      `Invalid ribbon spec for ${spec.app}:\n${issues.map((i) => `  ${i.path}: ${i.message}`).join('\n')}`,
    );
  }
}
