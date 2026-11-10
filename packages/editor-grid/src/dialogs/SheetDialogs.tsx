/**
 * The sheet tab menu's dialogs, as Excel's: Insert (a worksheet or a chart sheet), Move or Copy
 * (the sheet goes before the chosen one, or to the end, optionally as a copy) and Unhide (one of
 * the hidden sheets). Every change is a core command.
 */

import { t } from '@kiln/i18n';
import { Icon } from '@kiln/ui-kit';
import { BarChart3, Sheet } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { GridDialogFrame } from './CellDialogs.tsx';

/** A list of choices in a dialog (Excel's list boxes): click to choose, double-click for OK. */
function ChoiceList({
  label,
  items,
  value,
  onChange,
  onPick,
}: {
  label: string;
  items: readonly { id: number; text: string; icon?: ReactNode }[];
  value: number | null;
  onChange: (id: number) => void;
  onPick: (id: number) => void;
}) {
  return (
    <div
      className="kiln-griddlg__list"
      role="listbox"
      aria-label={label}
      tabIndex={0}
      aria-activedescendant={value === null ? undefined : `kiln-sheetdlg-${value}`}
      onKeyDown={(e) => {
        const at = items.findIndex((i) => i.id === value);
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          const next = items[Math.max(0, Math.min(items.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)))];
          if (next) onChange(next.id);
        }
      }}
    >
      {items.map((i) => (
        // biome-ignore lint/a11y/useKeyWithClickEvents: the list box handles the keys
        <div
          key={i.id}
          id={`kiln-sheetdlg-${i.id}`}
          role="option"
          tabIndex={-1}
          aria-selected={i.id === value}
          className={
            i.id === value ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
          }
          onClick={() => onChange(i.id)}
          onDoubleClick={() => onPick(i.id)}
        >
          {i.icon}
          {i.text}
        </div>
      ))}
    </div>
  );
}

/** A sheet name not taken yet: `Chart1`, `Chart2`… */
function freeName(base: string, taken: readonly string[]): string {
  const lower = new Set(taken.map((t) => t.toLowerCase()));
  for (let n = 1; ; n++) if (!lower.has(`${base}${n}`.toLowerCase())) return `${base}${n}`;
}

/** Right-click a sheet tab ▸ Insert…: a worksheet, or a chart sheet charting the selection. */
export function InsertSheetDialog({ model }: { model: GridEditorModel }) {
  const sheets = useGrid(model, (s) => s.grid?.sheets ?? []);
  const [kind, setKind] = useState(0);
  const run = async (k: number) => {
    if (k === 0) {
      await model.run('grid.cells.new_sheet');
      return;
    }
    // Excel's Insert ▸ Chart: a chart sheet of the selection, before the current sheet.
    if (await model.chart({ ChartInsert: { kind: null } }))
      await model.chart({
        ChartMove: {
          to: {
            NewSheet: {
              name: freeName(
                'Chart',
                sheets.map((s) => s.name),
              ),
            },
          },
        },
      });
  };
  return (
    <GridDialogFrame model={model} title={t('Insert')} testId="grid-insert-sheet" onOk={() => run(kind)}>
      <ChoiceList
        label={t('Insert')}
        value={kind}
        onChange={setKind}
        onPick={async (k) => {
          await run(k);
          model.closeDialog();
        }}
        items={[
          { id: 0, text: 'Worksheet', icon: <Icon icon={Sheet} size={16} /> },
          { id: 1, text: 'Chart', icon: <Icon icon={BarChart3} size={16} /> },
        ]}
      />
    </GridDialogFrame>
  );
}

/** The end of the before-sheet list: Excel's "(move to end)". */
const END = -1;

/** Right-click a sheet tab ▸ Move or Copy…: before which sheet, and whether as a copy. */
export function MoveCopySheetDialog({ model, sheet }: { model: GridEditorModel; sheet: number }) {
  const sheets = useGrid(model, (s) => s.grid?.sheets ?? []);
  const [before, setBefore] = useState<number>(sheets.length > sheet + 1 ? sheet + 1 : END);
  const [copy, setCopy] = useState(false);
  const run = async (b: number) => {
    const n = sheets.length;
    if (copy) {
      // The copy is made after the sheet, then moved where it was asked for.
      await model.run('grid.sheet.duplicate', { index: sheet });
      const from = sheet + 1;
      const to = b === END ? n : b;
      if (to !== from) await model.run('grid.sheet.move', { from, to });
      return;
    }
    const to = b === END ? n - 1 : b > sheet ? b - 1 : b;
    if (to !== sheet) await model.run('grid.sheet.move', { from: sheet, to });
  };
  const items = [...sheets.map((s, i) => ({ id: i, text: s.name })), { id: END, text: '(move to end)' }];
  return (
    <GridDialogFrame
      model={model}
      title={t('Move or Copy')}
      testId="grid-move-copy-sheet"
      onOk={() => run(before)}
    >
      <div className="kiln-griddlg__label">
        {t('Move selected sheets')}
        <br />
        {t('To book: (this workbook)')}
      </div>
      <div className="kiln-griddlg__label">{t('Before sheet:')}</div>
      <ChoiceList
        label={t('Before sheet')}
        value={before}
        onChange={setBefore}
        onPick={async (b) => {
          await run(b);
          model.closeDialog();
        }}
        items={items}
      />
      <label className="kiln-griddlg__check">
        <input type="checkbox" checked={copy} onChange={(e) => setCopy(e.target.checked)} />
        {t('Create a copy')}
      </label>
    </GridDialogFrame>
  );
}

/** Right-click a sheet tab ▸ Unhide…: the hidden sheets, one to show. */
export function UnhideSheetDialog({ model }: { model: GridEditorModel }) {
  const sheets = useGrid(model, (s) => s.grid?.sheets ?? []);
  const hidden = sheets.map((s, i) => ({ id: i, text: s.name, hidden: s.hidden })).filter((s) => s.hidden);
  const [pick, setPick] = useState<number | null>(hidden[0]?.id ?? null);
  const run = async (i: number | null) => {
    if (i !== null) await model.run('grid.sheet.unhide', { index: i });
  };
  return (
    <GridDialogFrame
      model={model}
      title={t('Unhide')}
      testId="grid-unhide-sheet"
      okDisabled={pick === null}
      onOk={() => run(pick)}
    >
      <div className="kiln-griddlg__label">{t('Unhide sheet:')}</div>
      <ChoiceList
        label={t('Unhide sheet')}
        value={pick}
        onChange={setPick}
        onPick={async (i) => {
          await run(i);
          model.closeDialog();
        }}
        items={hidden}
      />
    </GridDialogFrame>
  );
}
