/**
 * The sheet tabs (Grid mockup): + to add a sheet, one tab per visible sheet with its colour dot,
 * the active one in the brand's upside-down arch; double-click renames in place; drag reorders;
 * the context menu inserts, deletes, renames, duplicates, moves, colours, hides and unhides.
 * Every change is a core command.
 */
import type { GridState } from '@kiln/protocol';
import {
  ColorPicker,
  Icon,
  IconButton,
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
  Submenu,
} from '@kiln/ui-kit';
import { Plus } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

function RenameInput({ initial, onDone }: { initial: string; onDone: (name: string | null) => void }) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <input
      ref={ref}
      className="kiln-sheettab__rename"
      aria-label="Sheet name"
      value={value}
      maxLength={31}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onDone(value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === 'Enter') onDone(value);
        if (e.key === 'Escape') onDone(null);
      }}
    />
  );
}

export function SheetTabs({ model }: { model: GridEditorModel }) {
  const state: GridState | null = useGrid(model, (s) => s.grid);
  const dialog = useGrid(model, (s) => s.dialog);
  const [renaming, setRenaming] = useState<number | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [menuFor, setMenuFor] = useState<number | null>(null);
  // Format ▸ Rename Sheet and the context menu open the in-place editor.
  useEffect(() => {
    if (dialog?.id === 'renameSheet' && state) {
      setRenaming(dialog.arg ?? state.active_sheet);
      model.set({ dialog: null });
    }
  }, [dialog, state, model]);
  if (!state) return <div className="kiln-sheettabs" />;
  const sheets = state.sheets.map((s, i) => ({ ...s, index: i })).filter((s) => !s.hidden);
  const hidden = state.sheets.map((s, i) => ({ ...s, index: i })).filter((s) => s.hidden);
  const finishRename = (index: number, name: string | null) => {
    setRenaming(null);
    if (name?.trim() && name !== state.sheets[index]?.name)
      void model.run('grid.cells.rename_sheet', { index, name: name.trim() });
    model.surface?.focus();
  };
  return (
    <div className="kiln-sheettabs">
      <IconButton
        label="New Sheet"
        icon={<Icon icon={Plus} size={14} />}
        size="sm"
        variant="ghost"
        shortcut="Shift+F11"
        className="kiln-sheettabs__add"
        onClick={() => void model.run('grid.cells.new_sheet')}
      />
      <div className="kiln-sheettabs__list" role="tablist" aria-label="Sheets">
        {sheets.map((s) => {
          const active = s.index === state.active_sheet;
          return (
            <Menu
              key={`${s.index}:${s.name}`}
              open={menuFor === s.index}
              onOpenChange={(o) => setMenuFor(o ? s.index : null)}
            >
              <MenuTrigger asChild>
                <div
                  role="tab"
                  tabIndex={active ? 0 : -1}
                  aria-selected={active}
                  className={`kiln-sheettab${active ? ' kiln-sheettab--active' : ''}${dragFrom === s.index ? ' kiln-sheettab--dragging' : ''}`}
                  draggable={renaming === null}
                  data-sheet={s.name}
                  onPointerDown={(e) => {
                    // The menu opens on right-click only.
                    if (e.button === 0) {
                      e.preventDefault();
                      if (!active) void model.run('grid.sheet.activate', { index: s.index });
                      model.surface?.focus();
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
                      e.preventDefault();
                      void model.run(
                        e.key === 'ArrowRight' ? 'grid.cells.next_sheet' : 'grid.cells.previous_sheet',
                      );
                    }
                  }}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setMenuFor(s.index);
                  }}
                  onDoubleClick={() => setRenaming(s.index)}
                  onDragStart={(e) => {
                    setDragFrom(s.index);
                    e.dataTransfer.effectAllowed = 'move';
                    e.dataTransfer.setData('text/plain', s.name);
                  }}
                  onDragEnd={() => setDragFrom(null)}
                  onDragOver={(e) => {
                    if (dragFrom !== null) e.preventDefault();
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    if (dragFrom !== null && dragFrom !== s.index)
                      void model.run('grid.sheet.move', { from: dragFrom, to: s.index });
                    setDragFrom(null);
                  }}
                >
                  {s.color ? (
                    <span
                      className="kiln-sheettab__dot"
                      style={{ background: `#${s.color}` }}
                      aria-hidden="true"
                    />
                  ) : null}
                  {renaming === s.index ? (
                    <RenameInput initial={s.name} onDone={(n) => finishRename(s.index, n)} />
                  ) : (
                    <span className="kiln-sheettab__name">{s.name}</span>
                  )}
                </div>
              </MenuTrigger>
              <MenuContent align="start" side="top">
                <MenuItem onSelect={() => void model.run('grid.cells.new_sheet')}>Insert Sheet</MenuItem>
                <MenuItem onSelect={() => void model.run('grid.sheet.delete', { index: s.index })} danger>
                  Delete
                </MenuItem>
                <MenuItem onSelect={() => setRenaming(s.index)}>Rename</MenuItem>
                <MenuItem onSelect={() => void model.run('grid.sheet.duplicate', { index: s.index })}>
                  Duplicate
                </MenuItem>
                <MenuItem
                  disabled={s.index === 0}
                  onSelect={() => void model.run('grid.sheet.move', { from: s.index, to: s.index - 1 })}
                >
                  Move Left
                </MenuItem>
                <MenuItem
                  disabled={s.index === state.sheets.length - 1}
                  onSelect={() => void model.run('grid.sheet.move', { from: s.index, to: s.index + 1 })}
                >
                  Move Right
                </MenuItem>
                <Submenu label="Tab Color">
                  <ColorPicker
                    allowNone
                    noneLabel="No Color"
                    onChange={(c) => {
                      setMenuFor(null);
                      void model.run('grid.sheet.tab_color', {
                        index: s.index,
                        color: c.type === 'none' || c.type === 'auto' ? null : c.hex,
                      });
                    }}
                  />
                </Submenu>
                <MenuSeparator />
                <MenuItem onSelect={() => void model.run('grid.sheet.hide', { index: s.index })}>
                  Hide
                </MenuItem>
                {hidden.length > 0 ? (
                  <Submenu label="Unhide">
                    {hidden.map((h) => (
                      <MenuItem
                        key={h.index}
                        onSelect={() => void model.run('grid.sheet.unhide', { index: h.index })}
                      >
                        {h.name}
                      </MenuItem>
                    ))}
                  </Submenu>
                ) : null}
              </MenuContent>
            </Menu>
          );
        })}
      </div>
    </div>
  );
}
