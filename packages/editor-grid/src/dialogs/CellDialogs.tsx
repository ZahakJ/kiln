/**
 * Grid's small dialogs: Go To (F5, Ctrl+G), Paste Special (Ctrl+Alt+V), Insert and Delete Cells,
 * Row Height and Column Width, Tab Color and Insert Function (Shift+F3, fx). Each one ends in one
 * core command; the lists they show (defined names, functions) are the core's answers.
 */

import { msg, t, tx, txPairs } from '@kiln/i18n';
import type { CellShift, GridFunctionInfo, GridPaste, GridPasteOp } from '@kiln/protocol';
import { Button, Dialog, DialogContent, NumberInput } from '@kiln/ui-kit';
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from 'react';
import { featuresOf, recentFunctions } from '../features/model.ts';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { ColorButton } from './ColorButton.tsx';

/** A modal dialog of the Grid editor, closed through the model. */
export function GridDialogFrame({
  model,
  title,
  onOk,
  okLabel = 'OK',
  okDisabled,
  size = 'sm',
  testId,
  children,
}: {
  model: GridEditorModel;
  title: string;
  onOk: () => void | Promise<void>;
  okLabel?: string;
  okDisabled?: boolean;
  size?: 'sm' | 'md' | 'lg';
  testId?: string;
  children: ReactNode;
}) {
  const close = () => model.closeDialog();
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={title}
        size={size}
        data-testid={testId}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={(e) => {
          // The keys go back to the sheet, not to the ribbon button that opened the dialog.
          e.preventDefault();
          model.surface?.focus();
        }}
        footer={
          <>
            <Button onClick={close}>{t('Cancel')}</Button>
            <Button
              variant="primary"
              disabled={okDisabled}
              onClick={async () => {
                await onOk();
                close();
              }}
            >
              {okLabel}
            </Button>
          </>
        }
      >
        <form
          className="kiln-griddlg"
          onSubmit={async (e) => {
            e.preventDefault();
            if (okDisabled) return;
            await onOk();
            close();
          }}
        >
          {children}
          {/* Enter in a field submits. */}
          <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function Radios<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="kiln-griddlg__radios">
      <legend>{label}</legend>
      {options.map(([v, text]) => (
        <label key={v} className="kiln-griddlg__check">
          <input type="radio" name={name} checked={value === v} onChange={() => onChange(v)} />
          {tx(text)}
        </label>
      ))}
    </fieldset>
  );
}

export function GoToDialog({ model }: { model: GridEditorModel }) {
  const names = useGrid(model, (s) => s.grid?.names ?? []);
  const [reference, setReference] = useState('');
  const id = useId();
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => field.current?.focus(), []);
  return (
    <GridDialogFrame
      model={model}
      title={t('Go To')}
      testId="grid-goto"
      okDisabled={!reference.trim()}
      onOk={async () => {
        await model.run('grid.nav.go_to', { reference: reference.trim() });
      }}
    >
      <div className="kiln-griddlg__label">{t('Go to')}</div>
      <div className="kiln-griddlg__list" role="listbox" aria-label={t('Defined names')}>
        {names.map((n) => (
          <div
            key={n}
            role="option"
            aria-selected={n === reference}
            tabIndex={-1}
            className={
              n === reference ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
            }
            onClick={() => setReference(n)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') setReference(n);
            }}
            onDoubleClick={async () => {
              await model.run('grid.nav.go_to', { reference: n });
              model.closeDialog();
            }}
          >
            {n}
          </div>
        ))}
      </div>
      <label className="kiln-griddlg__label" htmlFor={id}>
        {t('Reference')}
      </label>
      <input
        ref={field}
        id={id}
        className="kiln-griddlg__input"
        value={reference}
        onChange={(e) => setReference(e.target.value)}
      />
    </GridDialogFrame>
  );
}

const PASTE: readonly (readonly [GridPaste, string])[] = [
  ['All', msg('All')],
  ['Formulas', msg('Formulas')],
  ['Values', msg('Values')],
  ['Formats', msg('Formats')],
  ['Comments', msg('Comments and notes')],
  ['Validation', msg('Validation')],
  ['AllExceptBorders', msg('All except borders')],
  ['ColumnWidths', msg('Column widths')],
  ['FormulasAndNumberFormats', msg('Formulas and number formats')],
  ['ValuesAndNumberFormats', msg('Values and number formats')],
];

const OPS: readonly (readonly [GridPasteOp, string])[] = [
  ['None', msg('None')],
  ['Add', msg('Add')],
  ['Subtract', msg('Subtract')],
  ['Multiply', msg('Multiply')],
  ['Divide', msg('Divide')],
];

export function PasteSpecialDialog({ model }: { model: GridEditorModel }) {
  const [what, setWhat] = useState<GridPaste>('All');
  const [op, setOp] = useState<GridPasteOp>('None');
  const [skip, setSkip] = useState(false);
  const [transpose, setTranspose] = useState(false);
  return (
    <GridDialogFrame
      model={model}
      title={t('Paste Special')}
      size="md"
      testId="grid-paste-special"
      onOk={async () => {
        await model.run('grid.clipboard.paste_special', { what, op, skip_blanks: skip, transpose });
      }}
    >
      <div className="kiln-griddlg__cols">
        <Radios label={t('Paste')} value={what} options={txPairs(PASTE)} onChange={setWhat} />
        <Radios label={t('Operation')} value={op} options={txPairs(OPS)} onChange={setOp} />
      </div>
      <div className="kiln-griddlg__row">
        <label className="kiln-griddlg__check">
          <input type="checkbox" checked={skip} onChange={(e) => setSkip(e.target.checked)} />
          {t('Skip blanks')}
        </label>
        <label className="kiln-griddlg__check">
          <input type="checkbox" checked={transpose} onChange={(e) => setTranspose(e.target.checked)} />
          {t('Transpose')}
        </label>
        <Button
          onClick={async () => {
            model.closeDialog();
            await model.run('grid.clipboard.paste_link');
          }}
        >
          {t('Paste Link')}
        </Button>
      </div>
    </GridDialogFrame>
  );
}

export function InsertDeleteDialog({ model, mode }: { model: GridEditorModel; mode: 'insert' | 'delete' }) {
  const insert = mode === 'insert';
  const [shift, setShift] = useState<CellShift>(insert ? 'Vertical' : 'Horizontal');
  const options: readonly (readonly [CellShift, string])[] = insert
    ? [
        ['Horizontal', t('Shift cells right')],
        ['Vertical', t('Shift cells down')],
        ['Rows', t('Entire row')],
        ['Columns', t('Entire column')],
      ]
    : [
        ['Horizontal', t('Shift cells left')],
        ['Vertical', t('Shift cells up')],
        ['Rows', t('Entire row')],
        ['Columns', t('Entire column')],
      ];
  return (
    <GridDialogFrame
      model={model}
      title={insert ? t('Insert') : t('Delete')}
      testId={insert ? 'grid-insert-cells' : 'grid-delete-cells'}
      onOk={async () => {
        await model.run(insert ? 'grid.cells.insert_cells' : 'grid.cells.delete_cells', { shift });
      }}
    >
      <Radios
        label={insert ? t('Insert') : t('Delete')}
        value={shift}
        options={options}
        onChange={setShift}
      />
    </GridDialogFrame>
  );
}

export function SizeDialog({ model, what }: { model: GridEditorModel; what: 'row' | 'column' }) {
  const row = what === 'row';
  const [value, setValue] = useState(row ? 15 : 8.43);
  return (
    <GridDialogFrame
      model={model}
      title={row ? t('Row Height') : t('Column Width')}
      testId={row ? 'grid-row-height' : 'grid-column-width'}
      onOk={async () => {
        await model.run(
          row ? 'grid.cells.row_height' : 'grid.cells.column_width',
          row ? { points: value } : { chars: value },
        );
      }}
    >
      <span className="kiln-griddlg__field">
        <span className="kiln-griddlg__label">{row ? t('Row height:') : t('Column width:')}</span>
        <NumberInput
          aria-label={row ? t('Row height') : t('Column width')}
          value={value}
          min={0}
          max={row ? 409 : 255}
          step={row ? 0.75 : 1}
          precision={2}
          unit=""
          onValueChange={setValue}
          width={100}
        />
      </span>
    </GridDialogFrame>
  );
}

export function TabColorDialog({ model, sheet }: { model: GridEditorModel; sheet: number }) {
  const [color, setColor] = useState<string | null | undefined>(undefined);
  return (
    <GridDialogFrame
      model={model}
      title={t('Tab Color')}
      okDisabled={color === undefined}
      onOk={async () => {
        await model.run('grid.sheet.tab_color', { index: sheet, color: color ?? null });
      }}
    >
      <ColorButton
        label={t('Tab color')}
        allowNone
        value={color ? color.replace('#', '') : null}
        onChange={(c) => setColor(c.type === 'none' || c.type === 'auto' ? null : c.hex)}
      />
    </GridDialogFrame>
  );
}

const FUNCTION_CATEGORIES = [
  msg('Most Recently Used'),
  msg('All'),
  msg('Financial'),
  msg('Date & Time'),
  msg('Math & Trig'),
  msg('Statistical'),
  msg('Lookup & Reference'),
  msg('Database'),
  msg('Text'),
  msg('Logical'),
  msg('Information'),
  msg('Engineering'),
  msg('Dynamic Array'),
  msg('Web'),
  msg('Compatibility'),
] as const;

export function InsertFunctionDialog({
  model,
  category: initial,
}: {
  model: GridEditorModel;
  category?: string;
}) {
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState<string>(initial ?? 'All');
  const [list, setList] = useState<GridFunctionInfo[]>([]);
  const [picked, setPicked] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true;
    const t = setTimeout(async () => {
      const recent = category === 'Most Recently Used';
      const cat = category === 'All' || recent ? null : category;
      const r = await model.query({ Functions: { prefix: search.trim().toUpperCase(), category: cat } });
      if (!live) return;
      const all = r && 'Functions' in r ? r.Functions : [];
      // Most Recently Used lists the ten functions inserted last, newest first.
      const fns = recent ? recentFunctions().flatMap((n) => all.filter((f) => f.name === n)) : all;
      setList(fns);
      setPicked((p) => (p && fns.some((f) => f.name === p) ? p : (fns[0]?.name ?? null)));
    }, 60);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [model, search, category]);
  const info = useMemo(() => list.find((f) => f.name === picked) ?? null, [list, picked]);
  // OK opens Function Arguments for the function (Excel).
  const insert = async (name: string) => {
    model.closeDialog();
    featuresOf(model).openDialog('functionArgs', name);
  };
  const id = useId();
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => field.current?.focus(), []);
  return (
    <GridDialogFrame
      model={model}
      title={t('Insert Function')}
      size="md"
      testId="grid-insert-function"
      okDisabled={!picked}
      onOk={async () => {
        if (picked) await insert(picked);
      }}
    >
      <label className="kiln-griddlg__label" htmlFor={id}>
        {t('Search for a function')}
      </label>
      <input
        ref={field}
        id={id}
        className="kiln-griddlg__input"
        value={search}
        placeholder={t('Type a function name')}
        onChange={(e) => setSearch(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') {
            e.preventDefault();
            const i = list.findIndex((f) => f.name === picked);
            const next = list[Math.min(list.length - 1, i + 1)];
            if (next) setPicked(next.name);
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            const i = list.findIndex((f) => f.name === picked);
            const prev = list[Math.max(0, i - 1)];
            if (prev) setPicked(prev.name);
          }
        }}
      />
      <span className="kiln-griddlg__field">
        <label className="kiln-griddlg__label" htmlFor={`${id}-cat`}>
          {t('Or select a category')}
        </label>
        <select
          id={`${id}-cat`}
          className="kiln-griddlg__input"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        >
          {FUNCTION_CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {tx(c)}
            </option>
          ))}
        </select>
      </span>
      <div
        ref={listRef}
        className="kiln-griddlg__list kiln-griddlg__functions"
        role="listbox"
        aria-label={t('Functions')}
      >
        {list.map((f) => (
          <div
            key={f.name}
            role="option"
            aria-selected={f.name === picked}
            tabIndex={-1}
            className={
              f.name === picked ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
            }
            onClick={() => setPicked(f.name)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') setPicked(f.name);
            }}
            onDoubleClick={async () => {
              await insert(f.name);
              model.closeDialog();
            }}
          >
            {f.name}
          </div>
        ))}
      </div>
      <div className="kiln-griddlg__doc" aria-live="polite">
        {info ? (
          <>
            <div className="kiln-griddlg__syntax">{info.syntax}</div>
            <div>{info.summary}</div>
          </>
        ) : (
          <div>{t('No function matches.')}</div>
        )}
      </div>
    </GridDialogFrame>
  );
}
