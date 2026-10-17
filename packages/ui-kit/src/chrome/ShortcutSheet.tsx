import { formatChord, parseShortcut } from '@kiln/commands';
import { useMemo, useState } from 'react';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import { useCommands, useKeymapRevision } from './CommandContext.tsx';

export interface ShortcutSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const SCOPE_NAMES: Record<string, string> = {
  slideshow: 'During a slide show',
  rail: 'In the thumbnail rail',
  object: 'With a shape selected',
  'cell-edit': 'While editing a cell',
};

/** ★ Ctrl+/ — the shortcuts of the current app, grouped like the ribbon, filterable. */
export function ShortcutSheet({ open, onOpenChange }: ShortcutSheetProps) {
  const { registry, keymap, app, platform } = useCommands();
  const rev = useKeymapRevision();
  const [filter, setFilter] = useState('');
  const groups = useMemo(() => {
    if (!keymap || !open) return [];
    const byGroup = new Map<string, Map<string, { title: string; keys: string[] }>>();
    for (const b of keymap.bindings(app)) {
      if (b.onRelease) continue;
      const cmd = registry.get(b.command);
      if (!cmd) continue;
      const state = registry.getState(b.command, {
        ...registry.context,
        app,
        scopes: b.scope ? [b.scope] : [],
      });
      if (!state.visible) continue;
      const group = b.scope ? (SCOPE_NAMES[b.scope] ?? b.scope) : (cmd.category ?? 'General');
      const rows = byGroup.get(group) ?? new Map();
      const row = rows.get(b.command) ?? { title: cmd.title, keys: [] };
      const label = formatChord(parseShortcut(b.key, platform), platform);
      if (!row.keys.includes(label)) row.keys.push(label);
      rows.set(b.command, row);
      byGroup.set(group, rows);
    }
    const q = filter.trim().toLowerCase();
    return [...byGroup.entries()]
      .map(([name, rows]) => ({
        name,
        rows: [...rows.values()].filter(
          (r) => !q || r.title.toLowerCase().includes(q) || r.keys.some((k) => k.toLowerCase().includes(q)),
        ),
      }))
      .filter((g) => g.rows.length > 0);
  }, [keymap, registry, app, platform, filter, rev, open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Keyboard shortcuts" size="xl" className="kiln-shortcut-sheet">
        <input
          className="kiln-options__input kiln-shortcut-sheet__filter"
          type="search"
          placeholder="Filter shortcuts"
          aria-label="Filter shortcuts"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="kiln-shortcut-sheet__grid">
          {groups.map((g) => (
            <section key={g.name} className="kiln-shortcut-sheet__group">
              <h3>{g.name}</h3>
              <dl>
                {g.rows.map((r) => (
                  <div key={r.title} className="kiln-shortcut-sheet__row">
                    <dt>{r.title}</dt>
                    <dd>
                      {r.keys.map((k) => (
                        <kbd key={k} className="kiln-options-keyboard__key">
                          {k}
                        </kbd>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
