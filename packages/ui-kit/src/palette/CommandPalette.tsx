import { type CommandMatch, type RegisteredCommand, searchCommands } from '@kiln/commands';
import * as RD from '@radix-ui/react-dialog';
import { Command } from 'cmdk';
import { Search } from 'lucide-react';
import { useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { useCommands, useKeymapRevision, useRegistryVersion } from '../chrome/CommandContext.tsx';
import { CommandIcon } from '../chrome/commandIcons.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Kbd } from '../primitives/Kbd.tsx';
import './palette.css';

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  placeholder?: string;
  /** Max results while typing. Default 60. */
  limit?: number;
}

function useRecent(): readonly string[] {
  const { registry } = useCommands();
  const recent = registry.recent;
  return useSyncExternalStore(
    (cb) => recent?.subscribe(cb) ?? (() => {}),
    () => recent?.list() ?? EMPTY,
    () => recent?.list() ?? EMPTY,
  );
}
const EMPTY: readonly string[] = [];

/**
 * ★ Command palette (Ctrl+Shift+P, Alt+Q — Office's "Tell me"): fuzzy search over every command's
 * title, Office name and synonyms; Recent commands first; shows shortcuts; runs the command and
 * returns focus to the document.
 */
export function CommandPalette({ open, onOpenChange, placeholder, limit = 60 }: CommandPaletteProps) {
  const { registry, keymap, app } = useCommands();
  const [query, setQuery] = useState('');
  const version = useRegistryVersion();
  useKeymapRevision();
  const recentIds = useRecent();
  // Opened by shortcut or the search box: give focus back to where it was (usually the document).
  const returnFocus = useRef<HTMLElement | null>(null);
  if (open && !returnFocus.current && typeof document !== 'undefined') {
    returnFocus.current = document.activeElement as HTMLElement | null;
  }

  const commands = useMemo(
    () =>
      registry
        .list({ visibleOnly: true })
        .filter((c) => c.palette !== false)
        // Available commands first; not-yet-implemented ones (development builds only) after them.
        .sort((a, b) => Number(a.pending) - Number(b.pending) || a.title.localeCompare(b.title)),
    [registry, version, open],
  );

  const results: CommandMatch<RegisteredCommand>[] = useMemo(
    () =>
      query.trim() ? searchCommands(commands, query, { limit, weight: (c) => (c.pending ? 0.8 : 1) }) : [],
    [commands, query, limit],
  );
  const recent = useMemo(
    () => recentIds.map((id) => commands.find((c) => c.id === id)).filter((c): c is RegisteredCommand => !!c),
    [recentIds, commands],
  );

  const run = (id: string) => {
    onOpenChange(false);
    setQuery('');
    // Let the dialog close and focus return to the document before the command runs.
    setTimeout(() => void registry.execute(id), 0);
  };

  const renderItem = (c: RegisteredCommand, hint?: string, group = 'all') => {
    const state = registry.getState(c.id);
    const shortcut = keymap?.primaryShortcut(c.id, app) ?? c.shortcut;
    return (
      <Command.Item
        key={`${group}:${c.id}`}
        value={`${group}:${c.id}`}
        disabled={!state.enabled}
        onSelect={() => run(c.id)}
        className="kiln-palette__item"
        data-command={c.id}
      >
        <span className="kiln-palette__icon" aria-hidden="true">
          <CommandIcon name={c.icon} size={18} />
        </span>
        <span className="kiln-palette__text">
          <span className="kiln-palette__title">{c.title}</span>
          {hint ? (
            <span className="kiln-palette__hint">{hint}</span>
          ) : c.category ? (
            <span className="kiln-palette__hint">{c.category}</span>
          ) : null}
        </span>
        {state.pending ? <span className="kiln-palette__badge">Not in this build</span> : null}
        {state.checked ? <span className="kiln-palette__badge kiln-palette__badge--on">On</span> : null}
        {shortcut ? <Kbd shortcut={shortcut} className="kiln-palette__kbd" /> : null}
      </Command.Item>
    );
  };

  const hintFor = (m: CommandMatch<RegisteredCommand>) => {
    if (m.matchedOn === 'officeName') return `Office: ${m.matchedText}`;
    if (m.matchedOn === 'synonym') return `Matches “${m.matchedText}”`;
    return m.command.officeName && m.command.officeName !== m.command.title
      ? `Office: ${m.command.officeName}`
      : m.command.category;
  };

  return (
    <RD.Root
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) setQuery('');
      }}
    >
      <RD.Portal>
        <RD.Overlay className="kiln-palette-overlay" />
        <RD.Content
          className="kiln-palette"
          aria-describedby={undefined}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            const el = returnFocus.current;
            returnFocus.current = null;
            if (el?.isConnected) el.focus();
          }}
        >
          <RD.Title className="kiln-visually-hidden">Search commands</RD.Title>
          <Command label="Search commands" shouldFilter={false} loop>
            <div className="kiln-palette__search">
              <Icon icon={Search} size={18} />
              <Command.Input
                className="kiln-palette__input"
                value={query}
                onValueChange={setQuery}
                placeholder={placeholder ?? 'Search for a command, or type what you want to do'}
              />
              <Kbd shortcut="Esc" variant="inline" className="kiln-palette__esc" />
            </div>
            <Command.List className="kiln-palette__list">
              <Command.Empty className="kiln-palette__empty">No commands match “{query}”.</Command.Empty>
              {query.trim() ? (
                <Command.Group heading="Best matches">
                  {results.map((m) => renderItem(m.command, hintFor(m)))}
                </Command.Group>
              ) : (
                <>
                  {recent.length ? (
                    <Command.Group heading="Recent">
                      {recent.map((c) => renderItem(c, undefined, 'recent'))}
                    </Command.Group>
                  ) : null}
                  <Command.Group heading="All commands">{commands.map((c) => renderItem(c))}</Command.Group>
                </>
              )}
            </Command.List>
            <div className="kiln-palette__footer" aria-hidden="true">
              <span>
                <Kbd shortcut="ArrowUp" /> <Kbd shortcut="ArrowDown" /> to move
              </span>
              <span>
                <Kbd shortcut="Enter" /> to run
              </span>
              <span className="kiln-palette__footer-tip">
                Office names work too: try “Merge &amp; Center”
              </span>
            </div>
          </Command>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  );
}
