/**
 * Home ▸ Fill ▸ Series and Edit Custom Lists. Each ends in core commands
 * (`grid.editing.fill_series`, `grid.editing.custom_lists`); the dialogs only collect choices.
 */

import { msg, t, tx, txPairs } from '@kiln/i18n';
import type { GridDateUnit, GridSeriesType } from '@kiln/protocol';
import { Button } from '@kiln/ui-kit';
import { useId, useState } from 'react';
import { GridDialogFrame, Radios } from '../dialogs/CellDialogs.tsx';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { BUILT_IN_LISTS, loadUserLists, saveUserLists } from './customLists.ts';
import './home.css';

const TYPES: readonly (readonly [GridSeriesType, string])[] = [
  ['Linear', msg('Linear')],
  ['Growth', msg('Growth')],
  ['Date', msg('Date')],
  ['AutoFill', 'AutoFill'],
];

const UNITS: readonly (readonly [GridDateUnit, string])[] = [
  ['Day', msg('Day')],
  ['Weekday', msg('Weekday')],
  ['Month', msg('Month')],
  ['Year', msg('Year')],
];

export function SeriesDialog({ model }: { model: GridEditorModel }) {
  // The core's choice for the selection's shape (rows when it is wider than tall).
  const down = useGrid(model, (s) => s.grid?.home?.series_columns ?? true);
  const [where, setWhere] = useState<'rows' | 'columns'>(down ? 'columns' : 'rows');
  const [kind, setKind] = useState<GridSeriesType>('Linear');
  const [unit, setUnit] = useState<GridDateUnit>('Day');
  const [trend, setTrend] = useState(false);
  const [step, setStep] = useState('1');
  const [stop, setStop] = useState('');
  const stepId = useId();
  const stopId = useId();
  const stepNum = Number(step);
  const stopNum = stop.trim() === '' ? null : Number(stop);
  const bad = !Number.isFinite(stepNum) || (stopNum !== null && !Number.isFinite(stopNum));
  return (
    <GridDialogFrame
      model={model}
      title={t('Series')}
      size="md"
      testId="grid-fill-series"
      okDisabled={bad}
      onOk={async () => {
        await model.run('grid.editing.fill_series', {
          columns: where === 'columns',
          kind,
          unit,
          step: stepNum,
          stop: stopNum,
          trend: trend && (kind === 'Linear' || kind === 'Growth'),
        });
      }}
    >
      <div className="kiln-griddlg__cols">
        <Radios
          label={t('Series in')}
          value={where}
          options={[
            ['rows', t('Rows')],
            ['columns', t('Columns')],
          ]}
          onChange={setWhere}
        />
        <Radios label={t('Type')} value={kind} options={txPairs(TYPES)} onChange={setKind} />
        <fieldset className="kiln-griddlg__radios" disabled={kind !== 'Date'}>
          <legend>{t('Date unit')}</legend>
          {UNITS.map(([v, text]) => (
            <label key={v} className="kiln-griddlg__check">
              <input type="radio" name={`${stepId}-unit`} checked={unit === v} onChange={() => setUnit(v)} />
              {tx(text)}
            </label>
          ))}
        </fieldset>
      </div>
      <label className="kiln-griddlg__check">
        <input
          type="checkbox"
          checked={trend}
          disabled={kind !== 'Linear' && kind !== 'Growth'}
          onChange={(e) => setTrend(e.target.checked)}
        />
        {t('Trend')}
      </label>
      <div className="kiln-griddlg__row">
        <span className="kiln-griddlg__field">
          <label className="kiln-griddlg__label" htmlFor={stepId}>
            {t('Step value:')}
          </label>
          <input
            id={stepId}
            className="kiln-griddlg__input"
            inputMode="decimal"
            value={step}
            disabled={trend}
            onChange={(e) => setStep(e.target.value)}
          />
        </span>
        <span className="kiln-griddlg__field">
          <label className="kiln-griddlg__label" htmlFor={stopId}>
            {t('Stop value:')}
          </label>
          <input
            id={stopId}
            className="kiln-griddlg__input"
            inputMode="decimal"
            value={stop}
            disabled={trend}
            onChange={(e) => setStop(e.target.value)}
          />
        </span>
      </div>
    </GridDialogFrame>
  );
}

/** Gives the user's lists to the core (when a workbook opens and after an edit). */
export async function sendCustomLists(model: GridEditorModel, lists: readonly (readonly string[])[]) {
  await model.core('grid.editing.custom_lists', { lists });
}

/** The lines of the List entries box (commas also separate, as in Excel). */
export function parseEntries(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

export function CustomListsDialog({ model }: { model: GridEditorModel }) {
  const [lists, setLists] = useState<string[][]>(loadUserLists);
  // -1: NEW LIST; 0…3 the built-in lists; 4… the user's.
  const [pick, setPick] = useState(-1);
  const [entries, setEntries] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const entriesId = useId();
  const all = [...BUILT_IN_LISTS, ...lists];
  const builtIn = pick >= 0 && pick < BUILT_IN_LISTS.length;
  const choose = (i: number, from: readonly (readonly string[])[] = all) => {
    setPick(i);
    setEntries(i < 0 ? '' : (from[i] ?? []).join('\n'));
    setMessage(null);
  };
  const save = async (next: string[][]) => {
    setLists(next);
    saveUserLists(next);
    await sendCustomLists(model, next);
  };
  const add = async () => {
    const items = parseEntries(entries);
    if (items.length < 2) {
      setMessage(t('A list needs at least two entries.'));
      return;
    }
    const own = pick - BUILT_IN_LISTS.length;
    const next = own >= 0 ? lists.map((l, i) => (i === own ? items : l)) : [...lists, items];
    await save(next);
    if (own < 0) choose(BUILT_IN_LISTS.length + next.length - 1, [...BUILT_IN_LISTS, ...next]);
  };
  return (
    <GridDialogFrame
      model={model}
      title={t('Custom Lists')}
      size="lg"
      testId="grid-custom-lists"
      onOk={async () => {
        // OK adds what was typed for a new list, as Excel's does.
        if (pick < 0 && parseEntries(entries).length >= 2) await add();
      }}
    >
      <div className="kiln-griddlg__cols kiln-ghome__lists">
        <div>
          <div className="kiln-griddlg__label">{t('Custom lists:')}</div>
          <div className="kiln-griddlg__list" role="listbox" aria-label={t('Custom lists')}>
            {[{ label: t('NEW LIST'), i: -1 }, ...all.map((l, i) => ({ label: l.join(', '), i }))].map(
              ({ label, i }) => (
                <div
                  key={i}
                  role="option"
                  aria-selected={pick === i}
                  tabIndex={pick === i ? 0 : -1}
                  className={
                    pick === i ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
                  }
                  onClick={() => choose(i)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') choose(i);
                  }}
                >
                  {label}
                </div>
              ),
            )}
          </div>
        </div>
        <div>
          <label className="kiln-griddlg__label" htmlFor={entriesId}>
            {t('List entries:')}
          </label>
          <textarea
            id={entriesId}
            className="kiln-griddlg__input kiln-griddlg__entries"
            rows={8}
            value={entries}
            readOnly={builtIn}
            onChange={(e) => setEntries(e.target.value)}
          />
          <div className="kiln-griddlg__row kiln-ghome__list-actions">
            <Button disabled={builtIn} onClick={() => void add()}>
              {pick >= BUILT_IN_LISTS.length ? t('Save') : t('Add')}
            </Button>
            <Button
              disabled={pick < BUILT_IN_LISTS.length}
              onClick={async () => {
                const own = pick - BUILT_IN_LISTS.length;
                await save(lists.filter((_, i) => i !== own));
                choose(-1);
              }}
            >
              {t('Delete')}
            </Button>
            <Button
              onClick={async () => {
                // Import list from cells: the selection's text becomes a list.
                if (!(await model.core('grid.editing.import_custom_list'))) {
                  setMessage(model.state.notice);
                  model.set({ notice: null });
                  return;
                }
                const r = await model.query('CustomLists');
                const next = r && 'Lists' in r ? r.Lists : lists;
                setLists(next);
                saveUserLists(next);
                choose(BUILT_IN_LISTS.length + next.length - 1, [...BUILT_IN_LISTS, ...next]);
              }}
            >
              {t('Import from Cells')}
            </Button>
          </div>
          {message ? (
            <p className="kiln-griddlg__hint" role="alert">
              {message}
            </p>
          ) : null}
        </div>
      </div>
    </GridDialogFrame>
  );
}
