/**
 * The sheet tabs (Grid mockup): + to add a sheet, one tab per visible sheet with its colour dot,
 * the active one in the brand's upside-down arch; double-click renames in place; drag reorders;
 * the context menu is Excel's: Insert…, Delete, Rename, Move or Copy…, View Code, Protect Sheet…,
 * Tab Color, Hide, Unhide… and Select All Sheets.
 * Every change is a core command.
 */
import { readingStep, t } from '@kiln/i18n';
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
import { BarChart3, Plus } from 'lucide-react';
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
      data-kiln-shortcuts="off"
      aria-label={t('Sheet name')}
      value={value}
      maxLength={31}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onDone(value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        // The keys go back to the sheet in this keydown: its Enter must not reach the sheet's
        // input proxy as a new line (an entry left open in the active cell).
        if (e.key === 'Enter' || e.key === 'Escape') e.preventDefault();
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
        label={t('New Sheet')}
        icon={<Icon icon={Plus} size={14} />}
        size="sm"
        variant="ghost"
        shortcut="Shift+F11"
        className="kiln-sheettabs__add"
        onClick={() => void model.run('grid.cells.new_sheet')}
      />
      <div className="kiln-sheettabs__list" role="tablist" aria-label={t('Sheets')}>
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
                    const step = readingStep(e.key, e.currentTarget);
                    if (step) {
                      e.preventDefault();
                      void model.run(step > 0 ? 'grid.cells.next_sheet' : 'grid.cells.previous_sheet');
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
                    <span className="kiln-sheettab__name">
                      {s.chart ? (
                        <Icon
                          icon={BarChart3}
                          size={12}
                          className="kiln-sheettab__chart"
                          label={t('Chart sheet')}
                        />
                      ) : null}
                      {s.name}
                    </span>
                  )}
                </div>
              </MenuTrigger>
              <MenuContent align="start" side="top" aria-label={t('Sheet')}>
                <MenuItem onSelect={() => model.openDialog('insertSheet')}>{t('Insert…')}</MenuItem>
                <MenuItem onSelect={() => void model.run('grid.sheet.delete', { index: s.index })} danger>
                  {t('Delete')}
                </MenuItem>
                <MenuItem onSelect={() => setRenaming(s.index)}>{t('Rename')}</MenuItem>
                <MenuItem onSelect={() => model.openDialog('moveCopySheet', undefined, s.index)}>
                  {t('Move or Copy…')}
                </MenuItem>
                <MenuItem disabled description={t('Macros are kept, not edited')}>
                  {t('View Code')}
                </MenuItem>
                <MenuItem onSelect={() => void model.run('grid.review.protect_sheet')}>
                  {t('Protect Sheet…')}
                </MenuItem>
                <Submenu label={t('Tab Color')}>
                  <ColorPicker
                    allowNone
                    noneLabel={t('No Color')}
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
                <MenuItem
                  disabled={sheets.length < 2}
                  onSelect={() => void model.run('grid.sheet.hide', { index: s.index })}
                >
                  {t('Hide')}
                </MenuItem>
                <MenuItem disabled={hidden.length === 0} onSelect={() => model.openDialog('unhideSheet')}>
                  {t('Unhide…')}
                </MenuItem>
                <MenuSeparator />
                <MenuItem disabled description={t('Not available yet')}>
                  {t('Select All Sheets')}
                </MenuItem>
              </MenuContent>
            </Menu>
          );
        })}
      </div>
    </div>
  );
}
