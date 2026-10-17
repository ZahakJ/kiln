/**
 * Grid's small dialogs: Go To (F5, Ctrl+G), Paste Special (Ctrl+Alt+V), Insert and Delete Cells,
 * Row Height and Column Width, Tab Color and Insert Function (Shift+F3, fx). Each one ends in one
 * core command; the lists they show (defined names, functions) are the core's answers.
 */
import type { CellShift, GridFunctionInfo, GridPaste, GridPasteOp } from '@kiln/protocol';
import { Button, Dialog, DialogContent, NumberInput } from '@kiln/ui-kit';
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from 'react';
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
        footer={
          <>
            <Button onClick={close}>Cancel</Button>
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

function Radios<T extends string>({
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
          {text}
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
      title="Go To"
      testId="grid-goto"
      okDisabled={!reference.trim()}
      onOk={async () => {
        await model.run('grid.nav.go_to', { reference: reference.trim() });
      }}
    >
      <div className="kiln-griddlg__label">Go to</div>
      <div className="kiln-griddlg__list" role="listbox" aria-label="Defined names">
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
        Reference
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
  ['All', 'All'],
  ['Formulas', 'Formulas'],
  ['Values', 'Values'],
  ['Formats', 'Formats'],
  ['AllExceptBorders', 'All except borders'],
  ['ValuesAndNumberFormats', 'Values and number formats'],
];

const OPS: readonly (readonly [GridPasteOp, string])[] = [
  ['None', 'None'],
  ['Add', 'Add'],
  ['Subtract', 'Subtract'],
  ['Multiply', 'Multiply'],
  ['Divide', 'Divide'],
];

export function PasteSpecialDialog({ model }: { model: GridEditorModel }) {
  const [what, setWhat] = useState<GridPaste>('All');
  const [op, setOp] = useState<GridPasteOp>('None');
  const [skip, setSkip] = useState(false);
  const [transpose, setTranspose] = useState(false);
  return (
    <GridDialogFrame
      model={model}
      title="Paste Special"
      size="md"
      testId="grid-paste-special"
      onOk={async () => {
        await model.run('grid.clipboard.paste_special', { what, op, skip_blanks: skip, transpose });
      }}
    >
      <div className="kiln-griddlg__cols">
        <Radios label="Paste" value={what} options={PASTE} onChange={setWhat} />
        <Radios label="Operation" value={op} options={OPS} onChange={setOp} />
      </div>
      <div className="kiln-griddlg__row">
        <label className="kiln-griddlg__check">
          <input type="checkbox" checked={skip} onChange={(e) => setSkip(e.target.checked)} />
          Skip blanks
        </label>
        <label className="kiln-griddlg__check">
          <input type="checkbox" checked={transpose} onChange={(e) => setTranspose(e.target.checked)} />
          Transpose
        </label>
      </div>
    </GridDialogFrame>
  );
}

export function InsertDeleteDialog({ model, mode }: { model: GridEditorModel; mode: 'insert' | 'delete' }) {
  const insert = mode === 'insert';
  const [shift, setShift] = useState<CellShift>(insert ? 'Vertical' : 'Horizontal');
  const options: readonly (readonly [CellShift, string])[] = insert
    ? [
        ['Horizontal', 'Shift cells right'],
        ['Vertical', 'Shift cells down'],
        ['Rows', 'Entire row'],
        ['Columns', 'Entire column'],
      ]
    : [
        ['Horizontal', 'Shift cells left'],
        ['Vertical', 'Shift cells up'],
        ['Rows', 'Entire row'],
        ['Columns', 'Entire column'],
      ];
  return (
    <GridDialogFrame
      model={model}
      title={insert ? 'Insert' : 'Delete'}
      testId={insert ? 'grid-insert-cells' : 'grid-delete-cells'}
      onOk={async () => {
        await model.run(insert ? 'grid.cells.insert_cells' : 'grid.cells.delete_cells', { shift });
      }}
    >
      <Radios label={insert ? 'Insert' : 'Delete'} value={shift} options={options} onChange={setShift} />
    </GridDialogFrame>
  );
}

export function SizeDialog({ model, what }: { model: GridEditorModel; what: 'row' | 'column' }) {
  const row = what === 'row';
  const [value, setValue] = useState(row ? 15 : 8.43);
  return (
    <GridDialogFrame
      model={model}
      title={row ? 'Row Height' : 'Column Width'}
      testId={row ? 'grid-row-height' : 'grid-column-width'}
      onOk={async () => {
        await model.run(
          row ? 'grid.cells.row_height' : 'grid.cells.column_width',
          row ? { points: value } : { chars: value },
        );
      }}
    >
      <span className="kiln-griddlg__field">
        <span className="kiln-griddlg__label">{row ? 'Row height:' : 'Column width:'}</span>
        <NumberInput
          aria-label={row ? 'Row height' : 'Column width'}
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
      title="Tab Color"
      okDisabled={color === undefined}
      onOk={async () => {
        await model.run('grid.sheet.tab_color', { index: sheet, color: color ?? null });
      }}
    >
      <ColorButton
        label="Tab color"
        allowNone
        value={color ? color.replace('#', '') : null}
        onChange={(c) => setColor(c.type === 'none' || c.type === 'auto' ? null : c.hex)}
      />
    </GridDialogFrame>
  );
}

const FUNCTION_CATEGORIES = [
  'Most Recently Used',
  'All',
  'Financial',
  'Date & Time',
  'Math & Trig',
  'Statistical',
  'Lookup & Reference',
  'Database',
  'Text',
  'Logical',
  'Information',
  'Engineering',
  'Dynamic Array',
  'Web',
  'Compatibility',
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
      const cat = category === 'All' || category === 'Most Recently Used' ? null : category;
      const r = await model.query({ Functions: { prefix: search.trim().toUpperCase(), category: cat } });
      if (!live) return;
      const fns = r && 'Functions' in r ? r.Functions : [];
      setList(fns);
      setPicked((p) => (p && fns.some((f) => f.name === p) ? p : (fns[0]?.name ?? null)));
    }, 60);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [model, search, category]);
  const info = useMemo(() => list.find((f) => f.name === picked) ?? null, [list, picked]);
  const insert = async (name: string) => {
    const g = model.state.grid;
    const ed = g?.editor;
    if (ed?.text.startsWith('=')) {
      const text = `${ed.text.slice(0, ed.start)}${name}(${ed.text.slice(ed.end)}`;
      const at = ed.start + name.length + 1;
      await model.core('grid.editing.set_text', { text, start: at, end: at, formula_bar: ed.formula_bar });
    } else {
      await model.core('grid.editing.begin', { text: `=${name}(` });
    }
  };
  const id = useId();
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => field.current?.focus(), []);
  return (
    <GridDialogFrame
      model={model}
      title="Insert Function"
      size="md"
      testId="grid-insert-function"
      okDisabled={!picked}
      onOk={async () => {
        if (picked) await insert(picked);
      }}
    >
      <label className="kiln-griddlg__label" htmlFor={id}>
        Search for a function
      </label>
      <input
        ref={field}
        id={id}
        className="kiln-griddlg__input"
        value={search}
        placeholder="Type a function name"
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
          Or select a category
        </label>
        <select
          id={`${id}-cat`}
          className="kiln-griddlg__input"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        >
          {FUNCTION_CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </span>
      <div
        ref={listRef}
        className="kiln-griddlg__list kiln-griddlg__functions"
        role="listbox"
        aria-label="Functions"
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
          <div>No function matches.</div>
        )}
      </div>
    </GridDialogFrame>
  );
}
