/**
 * The PivotTable dialogs: Create PivotTable, Value Field Settings, Field Settings (with
 * the sort, the item filter and the label and value filters), PivotTable Options, Move
 * PivotTable and Change PivotTable Data Source. What they show is the core's state; OK runs one
 * core command.
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type {
  GridFilterItem,
  GridPivotFieldSettings,
  GridPivotFilterOp,
  GridPivotFunction,
  GridPivotInfo,
  GridPivotSort,
  GridShowAs,
} from '@kiln/protocol';
import { useEffect, useMemo, useState } from 'react';
import { Check, Choice, FeatureFrame, Radios, TextField } from '../features/ui.tsx';
import { useGrid } from '../hooks.ts';
import { selectionText } from './commands.ts';
import type { GridAnalysis } from './model.ts';

export const FUNCTIONS: readonly (readonly [GridPivotFunction, string])[] = [
  ['Sum', msg('Sum')],
  ['Count', msg('Count')],
  ['Average', msg('Average')],
  ['Max', msg('Max')],
  ['Min', msg('Min')],
  ['Product', msg('Product')],
  ['CountNums', msg('Count Numbers')],
  ['StdDev', 'StdDev'],
  ['StdDevP', msg('StdDevp')],
  ['Var', msg('Var')],
  ['VarP', msg('Varp')],
];

const CAPTION: Record<GridPivotFunction, string> = {
  Sum: 'Sum',
  Count: 'Count',
  Average: 'Average',
  Max: 'Max',
  Min: 'Min',
  Product: 'Product',
  CountNums: 'Count',
  StdDev: 'StdDev',
  StdDevP: 'StdDevp',
  Var: 'Var',
  VarP: 'Varp',
};

const SHOW_AS: readonly (readonly [GridShowAs, string])[] = [
  ['Normal', msg('No Calculation')],
  ['PercentOfTotal', msg('% of Grand Total')],
  ['PercentOfColumn', msg('% of Column Total')],
  ['PercentOfRow', msg('% of Row Total')],
  ['Percent', msg('% Of')],
  ['PercentOfParentRow', msg('% of Parent Row Total')],
  ['PercentOfParentColumn', msg('% of Parent Column Total')],
  ['Difference', msg('Difference From')],
  ['PercentDifference', msg('% Difference From')],
  ['RunningTotal', msg('Running Total In')],
  ['PercentRunningTotal', msg('% Running Total In')],
  ['RankAscending', msg('Rank Smallest to Largest')],
  ['RankDescending', msg('Rank Largest to Smallest')],
  ['Index', msg('Index')],
];

const NEEDS_FIELD = new Set<GridShowAs>([
  'Percent',
  'Difference',
  'PercentDifference',
  'RunningTotal',
  'PercentRunningTotal',
  'RankAscending',
  'RankDescending',
]);
const NEEDS_ITEM = new Set<GridShowAs>(['Percent', 'Difference', 'PercentDifference']);

const FORMATS: readonly (readonly [string, string])[] = [
  ['General', msg('General')],
  ['0', msg('Number (0)')],
  ['0.00', msg('Number (0.00)')],
  ['#,##0', msg('Number (#,##0)')],
  ['#,##0.00', msg('Number (#,##0.00)')],
  ['0%', msg('Percentage (0%)')],
  ['0.00%', msg('Percentage (0.00%)')],
  ['"£"#,##0.00', msg('Currency (£)')],
  ['"$"#,##0.00', msg('Currency ($)')],
  ['m/d/yyyy', msg('Date')],
];

/** The items of a field, as the core lists them. */
function useItems(a: GridAnalysis, field: number | null): GridFilterItem[] {
  const [items, setItems] = useState<GridFilterItem[]>([]);
  useEffect(() => {
    if (field === null) {
      setItems([]);
      return;
    }
    let live = true;
    void a.features.ask({ PivotItems: { field } }).then((r) => {
      if (live && r && 'Items' in r) setItems(r.Items);
    });
    return () => {
      live = false;
    };
  }, [a, field]);
  return items;
}

function usePivot(a: GridAnalysis): GridPivotInfo | null {
  return useGrid(a.model, (s) => s.grid?.analysis.pivot ?? null);
}

/** Insert ▸ PivotTable. */
export function CreatePivotDialog({ a }: { a: GridAnalysis }) {
  const [source, setSource] = useState('');
  const [newSheet, setNewSheet] = useState<'new' | 'existing'>('new');
  const [to, setTo] = useState('');
  useEffect(() => {
    let live = true;
    void (async () => {
      const g = a.model.state.grid;
      const t = g?.features.table;
      if (t) {
        setSource(t.name);
        return;
      }
      const r = await a.features.ask('DataRange');
      if (!live) return;
      if (r && 'DataRange' in r) {
        const d = r.DataRange.range;
        const sheet = g?.sheets[g.active_sheet]?.name ?? '';
        const quoted = /^[A-Za-z_][\w.]*$/.test(sheet) ? sheet : `'${sheet.replace(/'/g, "''")}'`;
        setSource(`${quoted}!${rangeText(d)}`);
      } else {
        setSource(selectionText(a.model));
      }
    })();
    return () => {
      live = false;
    };
  }, [a]);
  return (
    <FeatureFrame
      features={a}
      title={t('Create PivotTable')}
      size="md"
      testId="grid-create-pivot"
      okDisabled={!source.trim() || (newSheet === 'existing' && !to.trim())}
      onOk={() =>
        a.tryExec({ CreatePivot: { source: source.trim(), to: newSheet === 'new' ? null : to.trim() } })
      }
    >
      <p className="kiln-gfx__title">{t('Choose the data that you want to analyse')}</p>
      <TextField
        label={t('Table/Range')}
        value={source}
        onChange={setSource}
        autoFocus
        wide
        testId="grid-pivot-source"
      />
      <p className="kiln-gfx__title">{t('Choose where you want the PivotTable to be placed')}</p>
      <Radios
        label={t('Place the PivotTable on')}
        value={newSheet}
        onChange={setNewSheet}
        options={[
          ['new', t('New Worksheet')],
          ['existing', t('Existing Worksheet')],
        ]}
      />
      {newSheet === 'existing' ? (
        <TextField label={t('Location')} value={to} onChange={setTo} placeholder={t('Sheet1!$H$3')} wide />
      ) : null}
    </FeatureFrame>
  );
}

function rangeText(r: { r0: number; c0: number; r1: number; c1: number }): string {
  const col = (c: number) => {
    let n = c + 1;
    let s = '';
    while (n > 0) {
      const m = (n - 1) % 26;
      s = String.fromCharCode(65 + m) + s;
      n = Math.floor((n - 1) / 26);
    }
    return s;
  };
  return `$${col(r.c0)}$${r.r0 + 1}:$${col(r.c1)}$${r.r1 + 1}`;
}

/** Value Field Settings: Summarize Values By, Show Values As, Number Format. */
export function ValueSettingsDialog({ a, index }: { a: GridAnalysis; index: number }) {
  const p = usePivot(a);
  const v = p?.values[index];
  const [name, setName] = useState(v?.name ?? '');
  const [fn, setFn] = useState<GridPivotFunction>(v?.function ?? 'Sum');
  const [showAs, setShowAs] = useState<GridShowAs>(v?.show_as ?? 'Normal');
  const [baseField, setBaseField] = useState<number | null>(v?.base_field ?? null);
  const [baseItem, setBaseItem] = useState<string>(v?.base_item ?? '(previous)');
  const [fmt, setFmt] = useState(v?.num_fmt ?? 'General');
  const [tab, setTab] = useState<'summarize' | 'show'>('summarize');
  const field = v ? p?.fields[v.field] : undefined;
  const items = useItems(a, NEEDS_ITEM.has(showAs) ? baseField : null);
  // A default name follows the function, as in Excel.
  const auto = v && field ? `${CAPTION[v.function]} of ${field.name}` : '';
  const pick = (f: GridPivotFunction) => {
    if (field && (name === auto || name.startsWith(`${CAPTION[fn]} of `)))
      setName(`${CAPTION[f]} of ${field.name}`);
    setFn(f);
  };
  if (!p || !v || !field) return null;
  const fields = p.fields.map((f, i) => [String(i), f.name] as const);
  return (
    <FeatureFrame
      features={a}
      title={t('Value Field Settings')}
      size="md"
      testId="grid-value-settings"
      onOk={() =>
        a.tryExec({
          Pivot: {
            edit: {
              Value: {
                index,
                value: {
                  field: v.field,
                  name,
                  function: fn,
                  show_as: showAs,
                  base_field: NEEDS_FIELD.has(showAs) ? (baseField ?? 0) : null,
                  base_item: NEEDS_ITEM.has(showAs)
                    ? baseItem
                    : NEEDS_FIELD.has(showAs)
                      ? '(previous)'
                      : null,
                  num_fmt: fmt === 'General' ? null : fmt,
                },
              },
            },
          },
        })
      }
    >
      <p className="kiln-gfx__hint">{t('Source Name: {name}', { name: field.name })}</p>
      <TextField
        label={t('Custom Name')}
        value={name}
        onChange={setName}
        autoFocus
        wide
        testId="grid-value-name"
      />
      <div className="kiln-gfx__toolbar" role="tablist" aria-label={t('Value field settings')}>
        {(
          [
            ['summarize', t('Summarize Values By')],
            ['show', t('Show Values As')],
          ] as const
        ).map(([t, label]) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={tab === t}
            className={tab === t ? 'kiln-gan__tab kiln-gan__tab--on' : 'kiln-gan__tab'}
            onClick={() => setTab(t)}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === 'summarize' ? (
        <div
          role="listbox"
          aria-label={t('Summarize value field by')}
          className="kiln-griddlg__list kiln-gan__list"
        >
          {FUNCTIONS.map(([f, label]) => (
            <button
              key={f}
              type="button"
              role="option"
              aria-selected={fn === f}
              className={fn === f ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'}
              onClick={() => pick(f)}
            >
              {tx(label)}
            </button>
          ))}
        </div>
      ) : (
        <div className="kiln-gfx__col">
          <Choice
            label={t('Show values as')}
            value={showAs}
            options={txPairs(SHOW_AS)}
            onChange={setShowAs}
            width={260}
          />
          {NEEDS_FIELD.has(showAs) ? (
            <Choice
              label={t('Base field')}
              value={String(baseField ?? 0)}
              options={fields}
              onChange={(f) => setBaseField(Number(f))}
              width={260}
            />
          ) : null}
          {NEEDS_ITEM.has(showAs) ? (
            <Choice
              label={t('Base item')}
              value={baseItem}
              options={[
                ['(previous)', t('(previous)')],
                ['(next)', t('(next)')],
                ...items.map((it) => [it.text, it.text] as const),
              ]}
              onChange={setBaseItem}
              width={260}
            />
          ) : null}
        </div>
      )}
      <Choice
        label={t('Number Format')}
        value={FORMATS.some((f) => f[0] === fmt) ? fmt : 'General'}
        options={txPairs(FORMATS)}
        onChange={setFmt}
        width={260}
      />
    </FeatureFrame>
  );
}

const LABEL_OPS: readonly (readonly [GridPivotFilterOp, string])[] = [
  ['Equal', msg('Equals')],
  ['NotEqual', msg('Does Not Equal')],
  ['BeginsWith', msg('Begins With')],
  ['NotBeginsWith', msg('Does Not Begin With')],
  ['EndsWith', msg('Ends With')],
  ['NotEndsWith', msg('Does Not End With')],
  ['Contains', msg('Contains')],
  ['NotContains', msg('Does Not Contain')],
  ['GreaterThan', msg('Is Greater Than')],
  ['GreaterThanOrEqual', msg('Is Greater Than Or Equal To')],
  ['LessThan', msg('Is Less Than')],
  ['LessThanOrEqual', msg('Is Less Than Or Equal To')],
  ['Between', msg('Is Between')],
  ['NotBetween', msg('Is Not Between')],
];
const VALUE_OPS = LABEL_OPS.filter(([o]) => !/With|Contain/.test(o));

type ValueFilterKind = 'none' | 'top' | 'bottom' | 'compare';

/** Field Settings with the field's sort, items, label filter and value filter. */
export function FieldSettingsDialog({
  a,
  field,
  section,
}: {
  a: GridAnalysis;
  field: number;
  section?: string;
}) {
  const p = usePivot(a);
  const f = p?.fields[field];
  const s: GridPivotFieldSettings | undefined = f?.settings;
  const items = useItems(a, field);
  const [caption, setCaption] = useState(s?.caption ?? '');
  const [subtotals, setSubtotals] = useState<'Automatic' | 'None' | 'Custom'>(
    s ? (typeof s.subtotals === 'string' ? s.subtotals : 'Custom') : 'Automatic',
  );
  const [custom, setCustom] = useState<GridPivotFunction[]>(
    s && typeof s.subtotals === 'object' ? s.subtotals.Custom : [],
  );
  const [sort, setSort] = useState<GridPivotSort>(s?.sort ?? 'Ascending');
  const [sortBy, setSortBy] = useState<string>(
    s?.sort_by_value === null || !s ? '' : String(s.sort_by_value),
  );
  const [hidden, setHidden] = useState<string[] | null>(null);
  const shownHidden = hidden ?? items.filter((i) => !i.checked).map((i) => i.text);
  const [labelOp, setLabelOp] = useState<GridPivotFilterOp | ''>(s?.label_filter?.op ?? '');
  const [labelA, setLabelA] = useState(s?.label_filter?.value ?? '');
  const [labelB, setLabelB] = useState(s?.label_filter?.value2 ?? '');
  const vf = s?.value_filter;
  const [vKind, setVKind] = useState<ValueFilterKind>(
    !vf ? 'none' : vf.top ? (vf.top[1] ? 'bottom' : 'top') : 'compare',
  );
  const [vValue, setVValue] = useState(String(vf?.value ?? 0));
  const [vCount, setVCount] = useState(String(vf?.top?.[0] ?? 10));
  const [vOp, setVOp] = useState<GridPivotFilterOp>(vf?.op ?? 'GreaterThan');
  const [vA, setVA] = useState(String(vf?.a ?? 0));
  const [vB, setVB] = useState(String(vf?.b ?? 0));
  const [search, setSearch] = useState('');
  const shownItems = useMemo(
    () => items.filter((i) => i.text.toLowerCase().includes(search.toLowerCase())),
    [items, search],
  );
  useEffect(() => {
    if (section) document.getElementById(`kiln-gan-${section}`)?.scrollIntoView({ block: 'nearest' });
  }, [section]);
  if (!p || !f || !s) return null;
  const values = p.values.map((v, i) => [String(i), v.name] as const);
  const all = shownHidden.length === 0;
  return (
    <FeatureFrame
      features={a}
      title={t('Field Settings')}
      size="lg"
      testId="grid-field-settings"
      onOk={() => {
        const settings: GridPivotFieldSettings = {
          caption: caption.trim() && caption.trim() !== f.name ? caption.trim() : null,
          subtotals: subtotals === 'Custom' ? { Custom: custom } : subtotals,
          sort,
          sort_by_value: sortBy === '' ? null : Number(sortBy),
          hidden: shownHidden,
          label_filter: labelOp ? { op: labelOp, value: labelA, value2: labelB } : null,
          value_filter:
            vKind === 'none'
              ? null
              : {
                  value: Number(vValue),
                  top: vKind === 'compare' ? null : [Math.max(1, Number(vCount) || 10), vKind === 'bottom'],
                  op: vOp,
                  a: Number(vA) || 0,
                  b: Number(vB) || 0,
                },
        };
        return a.tryExec({ Pivot: { edit: { Field: { field, settings } } } });
      }}
    >
      <p className="kiln-gfx__hint">{t('Source Name: {name}', { name: f.name })}</p>
      <TextField label={t('Custom Name')} value={caption || f.name} onChange={setCaption} autoFocus wide />
      <div className="kiln-gan__cols">
        <section className="kiln-gfx__section" aria-label={t('Subtotals')}>
          <Radios
            label={t('Subtotals')}
            value={subtotals}
            onChange={setSubtotals}
            options={[
              ['Automatic', t('Automatic')],
              ['None', t('None')],
              ['Custom', t('Custom')],
            ]}
          />
          {subtotals === 'Custom' ? (
            <div className="kiln-gan__checks">
              {FUNCTIONS.map(([fn, label]) => (
                <Check
                  key={fn}
                  label={tx(label)}
                  checked={custom.includes(fn)}
                  onChange={(on) => setCustom(on ? [...custom, fn] : custom.filter((x) => x !== fn))}
                />
              ))}
            </div>
          ) : null}
        </section>
        <section className="kiln-gfx__section" id="kiln-gan-sort" aria-label={t('Sort')}>
          <Radios
            label={t('Sort')}
            value={sort}
            onChange={setSort}
            options={[
              ['Ascending', t('Ascending (A to Z)')],
              ['Descending', t('Descending (Z to A)')],
              ['Source', t('Data source order')],
            ]}
          />
          {sort !== 'Source' ? (
            <Choice
              label={t('by')}
              value={sortBy}
              options={[['', f.name], ...values]}
              onChange={setSortBy}
              width={220}
            />
          ) : null}
        </section>
      </div>
      <section className="kiln-gfx__section" id="kiln-gan-items" aria-label={t('Items')}>
        <p className="kiln-gfx__title">{t('Show items')}</p>
        <input
          className="kiln-griddlg__input"
          type="search"
          placeholder={t('Search')}
          aria-label={t('Search items')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="kiln-gan__items" data-testid="grid-pivot-items">
          <Check
            label={t('(Select All)')}
            checked={all}
            onChange={(on) => setHidden(on ? [] : items.map((i) => i.text))}
          />
          {shownItems.map((it) => (
            <Check
              key={it.text}
              label={it.text}
              checked={!shownHidden.includes(it.text)}
              onChange={(on) =>
                setHidden(on ? shownHidden.filter((h) => h !== it.text) : [...shownHidden, it.text])
              }
            />
          ))}
        </div>
      </section>
      <div className="kiln-gan__cols">
        <section className="kiln-gfx__section" id="kiln-gan-label" aria-label={t('Label Filter')}>
          <p className="kiln-gfx__title">{t('Label Filter')}</p>
          <Choice
            label={t('Show items for which the label')}
            value={labelOp}
            options={[['', t('(none)')], ...txPairs(LABEL_OPS)]}
            onChange={setLabelOp}
            width={240}
          />
          {labelOp ? <TextField label={t('Value')} value={labelA} onChange={setLabelA} /> : null}
          {labelOp === 'Between' || labelOp === 'NotBetween' ? (
            <TextField label={t('and')} value={labelB} onChange={setLabelB} />
          ) : null}
        </section>
        <section className="kiln-gfx__section" id="kiln-gan-value" aria-label={t('Value Filter')}>
          <p className="kiln-gfx__title">{t('Value Filter')}</p>
          <Choice
            label={t('Show')}
            value={vKind}
            options={[
              ['none', t('(none)')],
              ['top', t('Top')],
              ['bottom', t('Bottom')],
              ['compare', t('Items whose value')],
            ]}
            onChange={setVKind}
            width={200}
          />
          {vKind !== 'none' && values.length > 0 ? (
            <Choice label={t('of')} value={vValue} options={values} onChange={setVValue} width={200} />
          ) : null}
          {vKind === 'top' || vKind === 'bottom' ? (
            <TextField label={t('Items')} value={vCount} onChange={setVCount} />
          ) : null}
          {vKind === 'compare' ? (
            <>
              <Choice
                label={t('is')}
                value={vOp}
                options={txPairs(VALUE_OPS)}
                onChange={setVOp}
                width={240}
              />
              <TextField label={t('Number')} value={vA} onChange={setVA} />
              {vOp === 'Between' || vOp === 'NotBetween' ? (
                <TextField label={t('and')} value={vB} onChange={setVB} />
              ) : null}
            </>
          ) : null}
        </section>
      </div>
    </FeatureFrame>
  );
}

/** PivotTable Options. */
export function PivotOptionsDialog({ a }: { a: GridAnalysis }) {
  const p = usePivot(a);
  const [name, setName] = useState(p?.name ?? '');
  const [empty, setEmpty] = useState(p?.options.empty_text ?? '');
  const [showEmpty, setShowEmpty] = useState(
    p?.options.empty_text !== null && p?.options.empty_text !== undefined,
  );
  const [rowTotals, setRowTotals] = useState(p?.options.row_totals ?? true);
  const [colTotals, setColTotals] = useState(p?.options.col_totals ?? true);
  const [repeat, setRepeat] = useState(p?.options.repeat_labels ?? false);
  const [blank, setBlank] = useState(p?.options.blank_rows ?? false);
  if (!p) return null;
  return (
    <FeatureFrame
      features={a}
      title={t('PivotTable Options')}
      size="md"
      testId="grid-pivot-options"
      onOk={async () => {
        if (name.trim() && name.trim() !== p.name) {
          const e = await a.tryExec({ Pivot: { edit: { Rename: { name: name.trim() } } } });
          if (e) return e;
        }
        return a.tryExec({
          Pivot: {
            edit: {
              Options: {
                options: {
                  ...p.options,
                  empty_text: showEmpty ? empty : null,
                  row_totals: rowTotals,
                  col_totals: colTotals,
                  repeat_labels: repeat,
                  blank_rows: blank,
                },
              },
            },
          },
        });
      }}
    >
      <TextField label={t('PivotTable Name')} value={name} onChange={setName} autoFocus wide />
      <p className="kiln-gfx__title">{t('Layout & Format')}</p>
      <div className="kiln-gfx__row">
        <Check label={t('For empty cells show')} checked={showEmpty} onChange={setShowEmpty} />
        <input
          className="kiln-griddlg__input"
          aria-label={t('Empty cells show')}
          value={empty}
          disabled={!showEmpty}
          onChange={(e) => setEmpty(e.target.value)}
        />
      </div>
      <Check label={t('Repeat all item labels')} checked={repeat} onChange={setRepeat} />
      <Check label={t('Insert a blank line after each item')} checked={blank} onChange={setBlank} />
      <p className="kiln-gfx__title">{t('Totals & Filters')}</p>
      <Check label={t('Show grand totals for rows')} checked={rowTotals} onChange={setRowTotals} />
      <Check label={t('Show grand totals for columns')} checked={colTotals} onChange={setColTotals} />
    </FeatureFrame>
  );
}

/** Move PivotTable. */
export function MovePivotDialog({ a }: { a: GridAnalysis }) {
  const [where, setWhere] = useState<'new' | 'existing'>('existing');
  const [to, setTo] = useState('');
  return (
    <FeatureFrame
      features={a}
      title={t('Move PivotTable')}
      size="sm"
      testId="grid-move-pivot"
      okDisabled={where === 'existing' && !to.trim()}
      onOk={async () => {
        if (where === 'new') {
          const ok = await a.exec('InsertSheet');
          if (!ok) return t('The sheet could not be added.');
          const g = await a.model.fetch();
          const sheet = g?.sheets[g.active_sheet]?.name ?? 'Sheet1';
          // Back to the PivotTable's sheet: the move goes from there.
          await a.exec('PreviousSheet');
          return a.tryExec({ Pivot: { edit: { Move: { to: `'${sheet.replace(/'/g, "''")}'!A3` } } } });
        }
        return a.tryExec({ Pivot: { edit: { Move: { to: to.trim() } } } });
      }}
    >
      <Radios
        label={t('Choose where you want the PivotTable to be placed')}
        value={where}
        onChange={setWhere}
        options={[
          ['new', t('New Worksheet')],
          ['existing', t('Existing Worksheet')],
        ]}
      />
      {where === 'existing' ? (
        <TextField
          label={t('Location')}
          value={to}
          onChange={setTo}
          placeholder={t('Sheet2!$A$3')}
          autoFocus
          wide
        />
      ) : null}
    </FeatureFrame>
  );
}

/** Change PivotTable Data Source. */
export function PivotSourceDialog({ a }: { a: GridAnalysis }) {
  const p = usePivot(a);
  const [source, setSource] = useState(p?.source ?? '');
  return (
    <FeatureFrame
      features={a}
      title={t('Change PivotTable Data Source')}
      size="sm"
      testId="grid-pivot-source-dialog"
      okDisabled={!source.trim()}
      onOk={() => a.tryExec({ Pivot: { edit: { Source: { source: source.trim() } } } })}
    >
      <TextField label={t('Table/Range')} value={source} onChange={setSource} autoFocus wide />
    </FeatureFrame>
  );
}
