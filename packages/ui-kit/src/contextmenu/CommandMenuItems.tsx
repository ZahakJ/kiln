import type { ReactNode } from 'react';
import {
  useCommandMeta,
  useCommandShortcut,
  useCommandState,
  useExecute,
} from '../chrome/CommandContext.tsx';
import { CommandIcon } from '../chrome/commandIcons.tsx';
import { MenuCheckboxItem, MenuItem, MenuLabel, MenuSeparator, Submenu } from '../primitives/Menu.tsx';

/** An entry of a command-driven menu (context menus, ribbon drop-downs and split buttons). */
export type MenuEntry =
  | {
      kind?: 'command';
      command: string;
      args?: unknown;
      /** Overrides the command title ("Keep Text Only" for `edit.pasteTextOnly`). */
      label?: string;
      icon?: string;
      description?: string;
    }
  | { kind: 'separator' }
  | { kind: 'heading'; label: string }
  | { kind: 'submenu'; label: string; icon?: string; items: readonly MenuEntry[] }
  /** Arbitrary content for app-specific needs (e.g. a recent-files list). */
  | { kind: 'custom'; id: string; render: () => ReactNode };

export function isCommandEntry(e: MenuEntry): e is Extract<MenuEntry, { command: string }> {
  return 'command' in e;
}

function CommandEntry({ entry }: { entry: Extract<MenuEntry, { command: string }> }) {
  const state = useCommandState(entry.command);
  const meta = useCommandMeta(entry.command);
  const shortcut = useCommandShortcut(entry.command);
  const execute = useExecute();
  if (!state.visible) return null;
  const label = entry.label ?? meta?.title ?? entry.command;
  const icon = <CommandIcon name={entry.icon ?? meta?.icon} size={16} />;
  if (state.checked !== undefined) {
    return (
      <MenuCheckboxItem
        checked={state.checked}
        disabled={!state.enabled}
        shortcut={shortcut}
        description={entry.description}
        onSelect={() => execute(entry.command, entry.args)}
      >
        {label}
      </MenuCheckboxItem>
    );
  }
  return (
    <MenuItem
      icon={icon}
      shortcut={shortcut}
      disabled={!state.enabled}
      description={entry.description}
      data-command={entry.command}
      onSelect={() => execute(entry.command, entry.args)}
    >
      {label}
    </MenuItem>
  );
}

/** Renders menu entries bound to the command registry: titles, icons, shortcuts, enabled/checked state. */
export function CommandMenuItems({ entries }: { entries: readonly MenuEntry[] }) {
  return (
    <>
      {entries.map((e, i) => {
        if (isCommandEntry(e)) return <CommandEntry key={`${e.command}-${i}`} entry={e} />;
        switch (e.kind) {
          case 'separator':
            return <MenuSeparator key={`sep-${i}`} />;
          case 'heading':
            return <MenuLabel key={`h-${e.label}`}>{e.label}</MenuLabel>;
          case 'submenu':
            return (
              <Submenu key={`sub-${e.label}`} label={e.label} icon={<CommandIcon name={e.icon} size={16} />}>
                <CommandMenuItems entries={e.items} />
              </Submenu>
            );
          case 'custom':
            return <span key={e.id}>{e.render()}</span>;
          default:
            return null;
        }
      })}
    </>
  );
}
