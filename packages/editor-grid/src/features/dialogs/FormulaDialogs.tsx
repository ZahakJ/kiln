/**
 * Formulas dialogs: the Name Manager (New, Edit, Delete, filter, scope, value, comment), Define
 * Name and Edit Name, Create Names from Selection, Paste Name (F3, Use in Formula), Evaluate
 * Formula (Excel's step-by-step reduction of the active cell's formula) and Error Checking.
 */
import { t } from '@kiln/i18n';
import type { GridErrorInfo, GridFormulaTrace, GridNameInfo } from '@kiln/protocol';
import { Button } from '@kiln/ui-kit';
import { useCallback, useEffect, useState } from 'react';
import { useGrid } from '../../hooks.ts';
import type { GridFeatures } from '../model.ts';
import { selectionRanges } from '../model.ts';
import { Check, Choice, FeatureFrame, rangeA1, TextField } from '../ui.tsx';

function useNames(features: GridFeatures): [GridNameInfo[] | null, () => void] {
  const [names, setNames] = useState<GridNameInfo[] | null>(null);
  const load = useCallback(() => {
    void features.ask('Names').then((r) => {
      if (r && 'Names' in r) setNames(r.Names);
    });
  }, [features]);
  useEffect(load, [load]);
  return [names, load];
}

function sheetOptions(features: GridFeatures): (readonly [string, string])[] {
  const sheets = features.model.state.grid?.sheets ?? [];
  return [['wb', t('Workbook')] as const, ...sheets.map((s, i) => [String(i), s.name] as const)];
}

/** The edit form of a name (Define Name, Name Manager ▸ New / Edit). */
function NameForm({
  features,
  initial,
  onDone,
  title,
}: {
  features: GridFeatures;
  initial: GridNameInfo | null;
  onDone: () => void;
  title: string;
}) {
  const sel = selectionRanges(features.model).at(-1);
  const sheet =
    features.model.state.grid?.sheets[features.model.state.grid?.active_sheet ?? 0]?.name ?? 'Sheet1';
  const quoted = /^[A-Za-z_][A-Za-z0-9_.]*$/.test(sheet) ? sheet : `'${sheet.replace(/'/g, "''")}'`;
  const [name, setName] = useState(initial?.name ?? '');
  const [scope, setScope] = useState(
    initial?.scope === null || initial?.scope === undefined ? 'wb' : String(initial.scope),
  );
  const [comment, setComment] = useState(initial?.comment ?? '');
  const [refers, setRefers] = useState(initial?.refers_to ?? (sel ? `=${quoted}!${rangeA1(sel)}` : '='));
  const proxy = Object.create(features) as GridFeatures;
  proxy.closeDialog = onDone;
  return (
    <FeatureFrame
      features={proxy}
      title={title}
      size="md"
      testId="grid-define-name"
      okDisabled={!name.trim()}
      onOk={() =>
        features.tryExec({
          DefineName: {
            name: name.trim(),
            refers_to: refers,
            scope: scope === 'wb' ? null : Number(scope),
            comment,
            previous: initial ? [initial.name, initial.scope] : null,
          },
        })
      }
    >
      <TextField label={t('Name')} value={name} onChange={setName} autoFocus wide testId="grid-name-name" />
      <Choice
        label={t('Scope')}
        value={scope}
        options={sheetOptions(features)}
        width={200}
        onChange={setScope}
      />
      <TextField label={t('Comment')} value={comment} onChange={setComment} wide />
      <TextField label={t('Refers to')} value={refers} onChange={setRefers} wide testId="grid-name-refers" />
    </FeatureFrame>
  );
}

export function DefineNameDialog({ features }: { features: GridFeatures }) {
  return (
    <NameForm
      features={features}
      initial={null}
      title={t('New Name')}
      onDone={() => features.closeDialog()}
    />
  );
}

export function NameManagerDialog({ features }: { features: GridFeatures }) {
  const [names, reload] = useNames(features);
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [editing, setEditing] = useState<GridNameInfo | 'new' | null>(null);
  const sheets = features.model.state.grid?.sheets ?? [];
  if (editing)
    return (
      <NameForm
        features={features}
        initial={editing === 'new' ? null : editing}
        title={editing === 'new' ? t('New Name') : t('Edit Name')}
        onDone={() => {
          setEditing(null);
          reload();
        }}
      />
    );
  const list = (names ?? []).filter((n) => !filter || n.name.toLowerCase().includes(filter.toLowerCase()));
  const current = list.find((n) => `${n.name}|${n.scope}` === selected) ?? null;
  return (
    <FeatureFrame features={features} title={t('Name Manager')} size="lg" testId="grid-name-manager">
      <div className="kiln-gfx__toolbar">
        <Button size="sm" onClick={() => setEditing('new')}>
          {t('New…')}
        </Button>
        <Button size="sm" disabled={!current || current.table} onClick={() => current && setEditing(current)}>
          {t('Edit…')}
        </Button>
        <Button
          size="sm"
          disabled={!current || current.table}
          onClick={async () => {
            if (!current) return;
            await features.exec({ DeleteName: { name: current.name, scope: current.scope } });
            setSelected(null);
            reload();
          }}
        >
          {t('Delete')}
        </Button>
        <span className="kiln-gfx__grow" />
        <input
          className="kiln-griddlg__input"
          aria-label={t('Filter names')}
          placeholder={t('Filter')}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="kiln-gfx__table kiln-gfx__names">
        <div className="kiln-gfx__thead">
          <span>{t('Name')}</span>
          <span>{t('Value')}</span>
          <span>{t('Refers To')}</span>
          <span>{t('Scope')}</span>
          <span>{t('Comment')}</span>
        </div>
        {names === null ? <p className="kiln-gfx__hint">{t('Loading…')}</p> : null}
        {names?.length === 0 ? <p className="kiln-gfx__hint">{t('The workbook has no names yet.')}</p> : null}
        {list.map((n) => (
          <button
            type="button"
            key={`${n.name}|${n.scope}`}
            className={
              `${n.name}|${n.scope}` === selected ? 'kiln-gfx__level kiln-gfx__level--on' : 'kiln-gfx__level'
            }
            onClick={() => setSelected(`${n.name}|${n.scope}`)}
            onDoubleClick={() => !n.table && setEditing(n)}
            data-testid="grid-name-row"
          >
            <span>{n.table ? `▦ ${n.name}` : n.name}</span>
            <span>{n.value}</span>
            <span>{n.refers_to}</span>
            <span>{n.scope === null ? t('Workbook') : (sheets[n.scope]?.name ?? '')}</span>
            <span>{n.comment}</span>
          </button>
        ))}
      </div>
      {current ? (
        <p className="kiln-gfx__hint">{t('Refers to: {refers_to}', { refers_to: current.refers_to })}</p>
      ) : null}
    </FeatureFrame>
  );
}

export function CreateNamesDialog({ features }: { features: GridFeatures }) {
  const [top, setTop] = useState(true);
  const [left, setLeft] = useState(false);
  const [bottom, setBottom] = useState(false);
  const [right, setRight] = useState(false);
  return (
    <FeatureFrame
      features={features}
      title={t('Create Names from Selection')}
      testId="grid-create-names"
      okDisabled={!top && !left && !bottom && !right}
      onOk={() => features.tryExec({ CreateNames: { top, left, bottom, right } })}
    >
      <p className="kiln-gfx__prompt">{t('Create names from values in the:')}</p>
      <Check label={t('Top row')} checked={top} onChange={setTop} />
      <Check label={t('Left column')} checked={left} onChange={setLeft} />
      <Check label={t('Bottom row')} checked={bottom} onChange={setBottom} />
      <Check label={t('Right column')} checked={right} onChange={setRight} />
    </FeatureFrame>
  );
}

/** Paste Name (F3) and Use in Formula: the name goes into the entry. */
export function PasteNameDialog({ features }: { features: GridFeatures }) {
  const [names] = useNames(features);
  const [pick, setPick] = useState<string | null>(null);
  const paste = async (name: string) => {
    const m = features.model;
    if (m.state.grid?.mode === 'Ready') await m.edit('grid.editing.begin', { text: `=${name}` });
    else {
      const ed = (await m.fetch())?.editor;
      const text = ed?.text ?? '';
      const at = ed?.end ?? text.length;
      await m.edit('grid.editing.set_text', {
        text: text.slice(0, at) + name + text.slice(at),
        start: at + name.length,
        end: at + name.length,
      });
    }
  };
  return (
    <FeatureFrame
      features={features}
      title={t('Paste Name')}
      testId="grid-paste-name"
      okDisabled={!pick}
      onOk={() => (pick ? paste(pick) : undefined)}
    >
      <div className="kiln-griddlg__list" role="listbox" aria-label={t('Paste name')}>
        {(names ?? []).map((n) => (
          <div
            key={`${n.name}|${n.scope}`}
            role="option"
            tabIndex={-1}
            aria-selected={n.name === pick}
            className={
              n.name === pick ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
            }
            onClick={() => setPick(n.name)}
            onKeyDown={(e) => e.key === 'Enter' && setPick(n.name)}
            onDoubleClick={async () => {
              await paste(n.name);
              features.closeDialog();
            }}
          >
            {n.name}
          </div>
        ))}
      </div>
    </FeatureFrame>
  );
}

/** Underlines `[start, end)` of `text`. */
function Underlined({ text, start, end }: { text: string; start: number; end: number }) {
  if (end <= start) return <code className="kiln-gfx__formula">{text}</code>;
  return (
    <code className="kiln-gfx__formula">
      {text.slice(0, start)}
      <u className="kiln-gfx__next">{text.slice(start, end)}</u>
      {text.slice(end)}
    </code>
  );
}

/** Evaluate Formula: Evaluate replaces the underlined part with its value, step by step. */
export function EvaluateDialog({ features }: { features: GridFeatures }) {
  const [trace, setTrace] = useState<GridFormulaTrace | null>(null);
  const [step, setStep] = useState(0);
  const lab = features.state.pane === 'lab';
  // The trace follows the active cell while the dialog is open.
  const grid = useGrid(features.model, (s) => s.grid);
  useEffect(() => {
    void (async () => {
      if (!lab) await features.exec({ FormulaLab: { open: true } });
      const r = await features.ask('FormulaTrace');
      if (r && 'Trace' in r) {
        setTrace(r.Trace);
        setStep(0);
      }
    })();
  }, [features, lab, grid?.name_box]);
  useEffect(
    () => () => {
      if (features.state.pane !== 'lab') void features.exec({ FormulaLab: { open: false } });
    },
    [features],
  );
  const s = trace?.steps[Math.min(step, (trace?.steps.length ?? 1) - 1)];
  const done = !!trace && step >= trace.steps.length - 1;
  return (
    <FeatureFrame
      features={features}
      title={t('Evaluate Formula')}
      size="lg"
      testId="grid-evaluate"
      footerStart={
        <Button
          size="sm"
          disabled={!trace?.steps.length}
          onClick={() => setStep(done ? 0 : step + 1)}
          data-testid="grid-evaluate-step"
        >
          {done ? t('Restart') : t('Evaluate')}
        </Button>
      }
    >
      <p className="kiln-gfx__prompt">
        {t('Reference:')} {trace?.cell ?? ''}
      </p>
      {trace?.message ? <p className="kiln-gfx__hint">{trace.message}</p> : null}
      {s ? (
        <>
          <div className="kiln-griddlg__label">{t('Evaluation')}</div>
          <div className="kiln-gfx__evaluation" data-testid="grid-evaluate-text">
            <Underlined text={s.text} start={s.start} end={s.end} />
          </div>
          <p className="kiln-gfx__hint">
            {done
              ? t('The formula is evaluated.')
              : t('To show the result of the underlined expression, click Evaluate. It gives {result}.', {
                  result: s.value,
                })}
          </p>
        </>
      ) : null}
    </FeatureFrame>
  );
}

/** Error Checking: each error cell in turn, with what the error means. */
export function ErrorCheckingDialog({ features }: { features: GridFeatures }) {
  const [errors, setErrors] = useState<GridErrorInfo[] | null>(null);
  const [at, setAt] = useState(0);
  useEffect(() => {
    void features.ask('Errors').then((r) => {
      if (r && 'Errors' in r) setErrors(r.Errors);
    });
  }, [features]);
  const e = errors?.[at];
  useEffect(() => {
    if (e) void features.model.core('grid.nav.select_cell', { row: e.row, col: e.col });
  }, [e, features]);
  return (
    <FeatureFrame features={features} title={t('Error Checking')} size="md" testId="grid-error-checking">
      {errors && errors.length === 0 ? (
        <p className="kiln-gfx__prompt">{t('The error check of the sheet is complete.')}</p>
      ) : null}
      {e ? (
        <>
          <p className="kiln-gfx__prompt">
            {t('Error in cell {cell}:', { cell: e.cell })} <code>{e.formula}</code>
          </p>
          <p className="kiln-gfx__title">{e.title}</p>
          <p className="kiln-gfx__hint">{e.help}</p>
          <div className="kiln-gfx__toolbar">
            <Button size="sm" disabled={at === 0} onClick={() => setAt(at - 1)}>
              {t('Previous')}
            </Button>
            <Button size="sm" disabled={!errors || at >= errors.length - 1} onClick={() => setAt(at + 1)}>
              {t('Next')}
            </Button>
            <Button
              size="sm"
              onClick={async () => {
                await features.model.run('grid.editing.edit_cell');
                features.closeDialog();
              }}
            >
              {t('Edit in Formula Bar')}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                features.set({ pane: 'lab' });
                void features.exec({ FormulaLab: { open: true } });
                features.closeDialog();
              }}
            >
              {t('Show Calculation Steps…')}
            </Button>
          </div>
          <p className="kiln-gfx__hint">
            {t('{n, number} of {total, number}', { n: at + 1, total: errors?.length ?? 0 })}
          </p>
        </>
      ) : null}
    </FeatureFrame>
  );
}
