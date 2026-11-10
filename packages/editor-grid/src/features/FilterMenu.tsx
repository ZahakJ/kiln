/**
 * The AutoFilter dropdown of a header cell (a click on its button, Alt+Down on the header): Sort A
 * to Z and Z to A, Sort by Colour, Clear Filter, Filter by Colour, Text, Number or Date Filters,
 * the search box and the list of the column's values with Select All and Blanks — all the core's
 * answer (`Query::Grid(FilterMenu)`). OK, a sort or a criterion is one core command; Esc and
 * Cancel give the keys back to the sheet. Custom AutoFilter and Top 10 AutoFilter are dialogs.
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type {
  GridColumnFilter,
  GridCriterion,
  GridDynamicFilter,
  GridFilterItem,
  GridFilterMenu,
  GridFilterOp,
  GridTotals,
} from '@kiln/protocol';
import { BottomSheet, Button, Icon, useFormFactor } from '@kiln/ui-kit';
import { ArrowDownAZ, ArrowDownZA, ChevronDown, ChevronRight, FunnelX, Search } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from 'zustand';
import type { GridFeatures } from './model.ts';
import { Choice, FeatureFrame, Radios, TextField } from './ui.tsx';

const TEXT_OPS: readonly (readonly [GridFilterOp, string])[] = [
  ['Equal', msg('Equals…')],
  ['NotEqual', msg('Does Not Equal…')],
  ['BeginsWith', msg('Begins With…')],
  ['EndsWith', msg('Ends With…')],
  ['Contains', msg('Contains…')],
  ['NotContains', msg('Does Not Contain…')],
];
const NUMBER_OPS: readonly (readonly [GridFilterOp, string])[] = [
  ['Equal', msg('Equals…')],
  ['NotEqual', msg('Does Not Equal…')],
  ['GreaterThan', msg('Greater Than…')],
  ['GreaterThanOrEqual', msg('Greater Than Or Equal To…')],
  ['LessThan', msg('Less Than…')],
  ['LessThanOrEqual', msg('Less Than Or Equal To…')],
];
const DATE_DYNAMIC: readonly (readonly [GridDynamicFilter, string])[] = [
  ['Tomorrow', msg('Tomorrow')],
  ['Today', msg('Today')],
  ['Yesterday', msg('Yesterday')],
  ['NextWeek', msg('Next Week')],
  ['ThisWeek', msg('This Week')],
  ['LastWeek', msg('Last Week')],
  ['NextMonth', msg('Next Month')],
  ['ThisMonth', msg('This Month')],
  ['LastMonth', msg('Last Month')],
  ['NextQuarter', msg('Next Quarter')],
  ['ThisQuarter', msg('This Quarter')],
  ['LastQuarter', msg('Last Quarter')],
  ['NextYear', msg('Next Year')],
  ['ThisYear', msg('This Year')],
  ['LastYear', msg('Last Year')],
  ['YearToDate', msg('Year to Date')],
];

const ALL_OPS: readonly (readonly [GridFilterOp, string])[] = [
  ['Equal', 'equals'],
  ['NotEqual', msg('does not equal')],
  ['GreaterThan', msg('is greater than')],
  ['GreaterThanOrEqual', msg('is greater than or equal to')],
  ['LessThan', msg('is less than')],
  ['LessThanOrEqual', msg('is less than or equal to')],
  ['BeginsWith', msg('begins with')],
  ['EndsWith', msg('ends with')],
  ['Contains', 'contains'],
  ['NotContains', msg('does not contain')],
];

export function FilterMenu({ features }: { features: GridFeatures }) {
  const at = useStore(features.store, (s) => s.filterMenu);
  const [menu, setMenu] = useState<GridFilterMenu | null>(null);
  const [search, setSearch] = useState('');
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [blanks, setBlanks] = useState(true);
  const [sub, setSub] = useState<'criteria' | 'color' | 'sortColor' | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  // a bottom sheet on a phone.
  const phone = useFormFactor() === 'phone';
  useEffect(() => {
    if (!at) return;
    setMenu(null);
    setSearch('');
    setSub(null);
    // The menu takes the keys at once (typing searches; the arrows walk the items).
    searchRef.current?.focus();
    void features.ask({ FilterMenu: { row: at.row, col: at.col } }).then((r) => {
      if (r && 'FilterMenu' in r) {
        const m = r.FilterMenu;
        setMenu(m);
        setChecked(new Set(m.items.filter((i) => i.checked).map((i) => i.text)));
        setBlanks(m.blanks ?? true);
      }
    });
  }, [at, features]);
  const shown = useMemo(
    () => (menu?.items ?? []).filter((i) => !search || i.text.toLowerCase().includes(search.toLowerCase())),
    [menu, search],
  );
  if (!at) return null;
  const close = () => {
    features.set({ filterMenu: null });
    features.model.surface?.focus();
  };
  const apply = async (filter: GridColumnFilter | null) => {
    close();
    await features.exec({ FilterColumn: { row: at.row, col: at.col, filter } });
  };
  const sort = async (
    descending: boolean,
    on: 'Values' | 'CellColor' | 'FontColor' = 'Values',
    color: string | null = null,
  ) => {
    close();
    await features.exec({ SortFilterColumn: { row: at.row, col: at.col, descending, on, color } });
  };
  const all = menu ? menu.items.every((i) => checked.has(i.text)) && (menu.blanks === null || blanks) : false;
  const ok = () => {
    if (!menu) return;
    if (search) {
      // Excel: the values found by the search, checked.
      const values = shown.filter((i) => checked.has(i.text)).map((i) => i.text);
      return apply({ Values: { values, blanks: false } });
    }
    if (all) return apply(null);
    return apply({
      Values: { values: menu.items.filter((i) => checked.has(i.text)).map((i) => i.text), blanks },
    });
  };
  const kind = menu?.kind ?? 'Text';
  const ops = kind === 'Text' ? TEXT_OPS : NUMBER_OPS;
  const custom = (op: GridFilterOp) => {
    close();
    features.openDialog('customFilter', { row: at.row, col: at.col, op, kind });
  };
  const header = menu?.header || 'column';
  const left = Math.max(8, Math.min(at.x, window.innerWidth - 300));
  const top = Math.max(8, Math.min(at.y, window.innerHeight - 480));
  const menuBody = (
    <div
      ref={box}
      className="kiln-gfx__filter"
      role="dialog"
      aria-label={t('Filter {header}', { header })}
      data-testid="grid-filter-menu"
      data-kiln-shortcuts="off"
      style={phone ? undefined : { left, top }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          close();
          return;
        }
        const target = e.target as HTMLElement;
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          moveFocus(box.current, e.key === 'ArrowDown' ? 1 : -1);
          return;
        }
        // Enter on a value applies the list (Space ticks it, as a checkbox does).
        if (e.key === 'Enter' && target instanceof HTMLInputElement && target.type === 'checkbox') {
          e.preventDefault();
          void ok();
          return;
        }
        // Typing anywhere in the menu searches the values.
        if (
          e.key.length === 1 &&
          e.key !== ' ' &&
          !e.ctrlKey &&
          !e.metaKey &&
          !e.altKey &&
          target !== searchRef.current
        ) {
          e.preventDefault();
          setSearch((v) => v + e.key);
          searchRef.current?.focus();
        }
      }}
    >
      <button
        type="button"
        className="kiln-gfx__fitem"
        onClick={() => sort(false)}
        data-testid="grid-filter-sort-asc"
      >
        <Icon icon={ArrowDownAZ} size={16} />
        {kind === 'Text'
          ? t('Sort A to Z')
          : kind === 'Date'
            ? t('Sort Oldest to Newest')
            : t('Sort Smallest to Largest')}
        {menu?.sorted === false ? <span className="kiln-gfx__ftick">✓</span> : null}
      </button>
      <button
        type="button"
        className="kiln-gfx__fitem"
        onClick={() => sort(true)}
        data-testid="grid-filter-sort-desc"
      >
        <Icon icon={ArrowDownZA} size={16} />
        {kind === 'Text'
          ? t('Sort Z to A')
          : kind === 'Date'
            ? t('Sort Newest to Oldest')
            : t('Sort Largest to Smallest')}
        {menu?.sorted === true ? <span className="kiln-gfx__ftick">✓</span> : null}
      </button>
      <button
        type="button"
        className="kiln-gfx__fitem"
        disabled={!menu?.fill_colors.length && !menu?.font_colors.length}
        aria-expanded={sub === 'sortColor'}
        onClick={() => setSub(sub === 'sortColor' ? null : 'sortColor')}
      >
        <span className="kiln-gfx__fpad" />
        {t('Sort by Color')}
        <Icon icon={ChevronRight} size={14} />
      </button>
      {sub === 'sortColor' && menu ? (
        <div className="kiln-gfx__fsub">
          {menu.fill_colors.map((c) => (
            <button
              type="button"
              key={`f${c}`}
              className="kiln-gfx__fitem"
              onClick={() => sort(false, 'CellColor', c)}
            >
              <span className="kiln-gfx__fswatch" style={{ background: `#${c}` }} />
              {t('Cell colour #{c}', { c })}
            </button>
          ))}
          {menu.font_colors.map((c) => (
            <button
              type="button"
              key={`t${c}`}
              className="kiln-gfx__fitem"
              onClick={() => sort(false, 'FontColor', c)}
            >
              <span className="kiln-gfx__fswatch kiln-gfx__fswatch--text" style={{ color: `#${c}` }}>
                A
              </span>
              {t('Font colour #{c}', { c })}
            </button>
          ))}
        </div>
      ) : null}
      <hr className="kiln-gfx__fsep" />
      <button type="button" className="kiln-gfx__fitem" disabled={!menu?.filter} onClick={() => apply(null)}>
        <Icon icon={FunnelX} size={16} />
        {t('Clear Filter From “{header}”', { header })}
      </button>
      <button
        type="button"
        className="kiln-gfx__fitem"
        disabled={!menu?.fill_colors.length && !menu?.font_colors.length}
        aria-expanded={sub === 'color'}
        onClick={() => setSub(sub === 'color' ? null : 'color')}
      >
        <span className="kiln-gfx__fpad" />
        {t('Filter by Color')}
        <Icon icon={ChevronRight} size={14} />
      </button>
      {sub === 'color' && menu ? (
        <div className="kiln-gfx__fsub">
          {menu.fill_colors.map((c) => (
            <button
              type="button"
              key={`f${c}`}
              className="kiln-gfx__fitem"
              onClick={() => apply({ Color: { color: c, font: false } })}
            >
              <span className="kiln-gfx__fswatch" style={{ background: `#${c}` }} />
              {t('Cell colour #{c}', { c })}
            </button>
          ))}
          {menu.font_colors.map((c) => (
            <button
              type="button"
              key={`t${c}`}
              className="kiln-gfx__fitem"
              onClick={() => apply({ Color: { color: c, font: true } })}
            >
              <span className="kiln-gfx__fswatch kiln-gfx__fswatch--text" style={{ color: `#${c}` }}>
                A
              </span>
              {t('Font colour #{c}', { c })}
            </button>
          ))}
        </div>
      ) : null}
      <button
        type="button"
        className="kiln-gfx__fitem"
        aria-expanded={sub === 'criteria'}
        onClick={() => setSub(sub === 'criteria' ? null : 'criteria')}
        data-testid="grid-filter-criteria"
      >
        <span className="kiln-gfx__fpad" />
        {kind === 'Text' ? t('Text Filters') : kind === 'Number' ? t('Number Filters') : t('Date Filters')}
        {menu?.filter && !('Values' in menu.filter) ? <span className="kiln-gfx__ftick">✓</span> : null}
        <Icon icon={ChevronRight} size={14} />
      </button>
      {sub === 'criteria' ? (
        <div className="kiln-gfx__fsub">
          {(kind === 'Date'
            ? ([
                ['Equal', t('Equals…')],
                ['LessThan', t('Before…')],
                ['GreaterThan', t('After…')],
              ] as const)
            : ops
          ).map(([op, label]) => (
            <button type="button" key={op} className="kiln-gfx__fitem" onClick={() => custom(op)}>
              {tx(label)}
            </button>
          ))}
          {kind !== 'Text' ? (
            <button type="button" className="kiln-gfx__fitem" onClick={() => custom('GreaterThanOrEqual')}>
              {t('Between…')}
            </button>
          ) : null}
          {kind === 'Number' ? (
            <>
              <button
                type="button"
                className="kiln-gfx__fitem"
                onClick={() => {
                  close();
                  features.openDialog('top10', { row: at.row, col: at.col });
                }}
              >
                {t('Top 10…')}
              </button>
              <button
                type="button"
                className="kiln-gfx__fitem"
                onClick={() => apply({ Dynamic: { kind: 'AboveAverage' } })}
              >
                {t('Above Average')}
              </button>
              <button
                type="button"
                className="kiln-gfx__fitem"
                onClick={() => apply({ Dynamic: { kind: 'BelowAverage' } })}
              >
                {t('Below Average')}
              </button>
            </>
          ) : null}
          {kind === 'Date'
            ? DATE_DYNAMIC.map(([d, label]) => (
                <button
                  type="button"
                  key={d}
                  className="kiln-gfx__fitem"
                  onClick={() => apply({ Dynamic: { kind: d } })}
                >
                  {tx(label)}
                </button>
              ))
            : null}
          <button type="button" className="kiln-gfx__fitem" onClick={() => custom('Equal')}>
            {t('Custom Filter…')}
          </button>
        </div>
      ) : null}
      <label className="kiln-gfx__fsearch">
        <Icon icon={Search} size={14} />
        <input
          ref={searchRef}
          placeholder={t('Search')}
          aria-label={t('Search the values')}
          value={search}
          data-testid="grid-filter-search"
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void ok();
            }
          }}
        />
      </label>
      <div className="kiln-gfx__flist">
        {menu ? (
          <>
            <label className="kiln-griddlg__check">
              <input
                type="checkbox"
                checked={search ? shown.every((i) => checked.has(i.text)) : all}
                ref={(el) => {
                  if (el) el.indeterminate = !all && checked.size > 0 && !search;
                }}
                onChange={(e) => {
                  const n = new Set(checked);
                  for (const i of search ? shown : menu.items) {
                    if (e.target.checked) n.add(i.text);
                    else n.delete(i.text);
                  }
                  setChecked(n);
                  if (!search) setBlanks(e.target.checked);
                }}
              />
              {search ? t('(Select All Search Results)') : t('(Select All)')}
            </label>
            {kind === 'Date' && !search && shown.some((i) => i.date) ? (
              <DateTree items={shown} checked={checked} setChecked={setChecked} />
            ) : (
              shown.slice(0, 1000).map((i) => (
                <label key={i.text} className="kiln-griddlg__check" data-testid="grid-filter-value">
                  <input
                    type="checkbox"
                    checked={checked.has(i.text)}
                    onChange={(e) => {
                      const n = new Set(checked);
                      if (e.target.checked) n.add(i.text);
                      else n.delete(i.text);
                      setChecked(n);
                    }}
                  />
                  {i.text}
                </label>
              ))
            )}
            {menu.blanks !== null && !search ? (
              <label className="kiln-griddlg__check">
                <input type="checkbox" checked={blanks} onChange={(e) => setBlanks(e.target.checked)} />
                {t('(Blanks)')}
              </label>
            ) : null}
            {menu.truncated ? <p className="kiln-gfx__hint">{t('Not all items are shown.')}</p> : null}
          </>
        ) : (
          <p className="kiln-gfx__hint">{t('Loading…')}</p>
        )}
      </div>
      <div className="kiln-gfx__ffoot">
        <Button
          variant="primary"
          size="sm"
          disabled={!menu || (!search && checked.size === 0 && !blanks)}
          onClick={() => void ok()}
          data-testid="grid-filter-ok"
        >
          {t('OK')}
        </Button>
        <Button size="sm" onClick={close}>
          {t('Cancel')}
        </Button>
      </div>
    </div>
  );
  if (!phone) return menuBody;
  return (
    <BottomSheet
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={t('Filter {header}', { header })}
      className="kiln-grid-filter-sheet"
      testId="grid-filter-sheet"
    >
      {menuBody}
    </BottomSheet>
  );
}

/** Custom AutoFilter: one or two criteria joined by And or Or. */
export function CustomFilterDialog({
  features,
  at,
}: {
  features: GridFeatures;
  at: { row: number; col: number; op: GridFilterOp; kind: string };
}) {
  const between = at.op === 'GreaterThanOrEqual' && at.kind !== 'Text';
  const [a, setA] = useState<GridCriterion>({ op: at.op, value: '' });
  const [b, setB] = useState<GridCriterion>({ op: between ? 'LessThanOrEqual' : 'Equal', value: '' });
  const [and, setAnd] = useState(true);
  return (
    <FeatureFrame
      features={features}
      title={t('Custom AutoFilter')}
      size="md"
      testId="grid-custom-filter"
      okDisabled={!a.value.trim()}
      onOk={() =>
        features.tryExec({
          FilterColumn: {
            row: at.row,
            col: at.col,
            filter: { Custom: { and, criteria: b.value.trim() ? [a, b] : [a] } },
          },
        })
      }
    >
      <p className="kiln-gfx__prompt">{t('Show rows where:')}</p>
      <div className="kiln-gfx__row">
        <Choice
          label={t('Comparison')}
          value={a.op}
          options={txPairs(ALL_OPS)}
          width={220}
          onChange={(op) => setA({ ...a, op })}
        />
        <TextField
          label={t('Value')}
          value={a.value}
          onChange={(value) => setA({ ...a, value })}
          autoFocus
          testId="grid-custom-filter-value"
        />
      </div>
      <Radios
        label={t('Join')}
        value={and ? 'and' : 'or'}
        options={[
          ['and', t('And')],
          ['or', t('Or')],
        ]}
        onChange={(v) => setAnd(v === 'and')}
      />
      <div className="kiln-gfx__row">
        <Choice
          label={t('Second comparison')}
          value={b.op}
          options={txPairs(ALL_OPS)}
          width={220}
          onChange={(op) => setB({ ...b, op })}
        />
        <TextField label={t('Value')} value={b.value} onChange={(value) => setB({ ...b, value })} />
      </div>
      <p className="kiln-gfx__hint">
        {t('Use ? to represent any single character, * any series of characters.')}
      </p>
    </FeatureFrame>
  );
}

/** Top 10 AutoFilter. */
export function Top10Dialog({ features, at }: { features: GridFeatures; at: { row: number; col: number } }) {
  const [top, setTop] = useState(true);
  const [n, setN] = useState('10');
  const [percent, setPercent] = useState(false);
  return (
    <FeatureFrame
      features={features}
      title={t('Top 10 AutoFilter')}
      testId="grid-top10"
      onOk={() =>
        features.tryExec({
          FilterColumn: {
            row: at.row,
            col: at.col,
            filter: { Top10: { top, percent, value: Math.max(0, Math.round(Number(n) || 0)) } },
          },
        })
      }
    >
      <div className="kiln-gfx__row">
        <Choice
          label={t('Show')}
          value={top ? 't' : 'b'}
          width={110}
          options={[
            ['t', t('Top')],
            ['b', t('Bottom')],
          ]}
          onChange={(v) => setTop(v === 't')}
        />
        <TextField label={t('Number')} value={n} onChange={setN} autoFocus />
        <Choice
          label={t('Of')}
          value={percent ? 'p' : 'i'}
          width={110}
          options={[
            ['i', t('Items')],
            ['p', t('Percent')],
          ]}
          onChange={(v) => setPercent(v === 'p')}
        />
      </div>
    </FeatureFrame>
  );
}

/** Moves the focus to the next (or previous) enabled item, field or box of a menu. */
function moveFocus(root: HTMLElement | null, step: 1 | -1): void {
  if (!root) return;
  const items = [...root.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)')].filter(
    (el) => el.offsetParent !== null || el === document.activeElement,
  );
  if (!items.length) return;
  const i = items.indexOf(document.activeElement as HTMLElement);
  const next = items[(i < 0 ? (step > 0 ? 0 : items.length - 1) : i + step + items.length) % items.length];
  next?.focus();
}

const TOTALS: readonly (readonly [GridTotals, string])[] = [
  ['None', msg('None')],
  ['Average', msg('Average')],
  ['Count', msg('Count')],
  ['CountNumbers', msg('Count Numbers')],
  ['Max', msg('Max')],
  ['Min', msg('Min')],
  ['Sum', msg('Sum')],
  ['StdDev', 'StdDev'],
  ['Var', msg('Var')],
];

/** A table's total row cell list (its button, Alt+Down): the SUBTOTAL functions, as in Excel. */
export function TotalsMenu({ features }: { features: GridFeatures }) {
  const at = useStore(features.store, (s) => s.totalsMenu);
  const box = useRef<HTMLDivElement>(null);
  const table = features.model.state.grid?.features.table ?? null;
  const current = at && table ? table.columns[at.col - table.range.c0]?.totals : undefined;
  useEffect(() => {
    if (!at) return;
    const take = () => {
      if (box.current?.contains(document.activeElement)) return;
      const items = box.current?.querySelectorAll<HTMLElement>('[role="option"]');
      const i = TOTALS.findIndex(([t]) => t === current);
      items?.[Math.max(0, i)]?.focus();
    };
    take();
    // The sheet may take the keys back as the key that opened the list is released.
    const raf = requestAnimationFrame(take);
    const timer = window.setTimeout(take, 120);
    return () => {
      cancelAnimationFrame(raf);
      window.clearTimeout(timer);
    };
  }, [at, current]);
  if (!at) return null;
  const close = () => {
    features.set({ totalsMenu: null });
    features.model.surface?.focus();
  };
  const pick = async (totals: GridTotals) => {
    close();
    await features.exec({ TableTotals: { col: at.col, totals } });
  };
  return (
    <div
      ref={box}
      className="kiln-gfx__filter kiln-gfx__filter--totals"
      role="listbox"
      aria-label={t('Total row function')}
      data-testid="grid-totals-menu"
      data-kiln-shortcuts="off"
      style={{ left: Math.max(8, Math.min(at.x, window.innerWidth - 200)), top: Math.max(8, at.y) }}
      onKeyDown={(e) => {
        if (e.key === 'Escape' || e.key === 'Tab') {
          e.preventDefault();
          close();
        } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault();
          moveFocus(box.current, e.key === 'ArrowDown' ? 1 : -1);
        }
      }}
      onBlur={(e) => {
        if (!box.current?.contains(e.relatedTarget as Node | null)) features.set({ totalsMenu: null });
      }}
    >
      {TOTALS.map(([t, label]) => (
        <button
          type="button"
          role="option"
          aria-selected={t === current}
          key={t}
          className="kiln-gfx__fitem"
          onClick={() => void pick(t)}
        >
          {tx(label)}
          {t === current ? <span className="kiln-gfx__ftick">✓</span> : null}
        </button>
      ))}
      <button
        type="button"
        role="option"
        aria-selected={false}
        className="kiln-gfx__fitem"
        onClick={() => {
          close();
          features.openDialog('functionArgs', 'SUBTOTAL');
        }}
      >
        {t('More Functions…')}
      </button>
    </div>
  );
}

const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

/**
 * The dates of a column as Excel lists them: years, their months and days, each with a box that
 * ticks everything under it (part-ticked when some are). Values without a day come after.
 */
function DateTree({
  items,
  checked,
  setChecked,
}: {
  items: GridFilterItem[];
  checked: Set<string>;
  setChecked: (s: Set<string>) => void;
}) {
  const [open, setOpen] = useState<Set<string>>(new Set());
  const tree = useMemo(() => {
    const years = new Map<string, Map<string, GridFilterItem[]>>();
    const rest: GridFilterItem[] = [];
    for (const i of items) {
      if (!i.date) {
        rest.push(i);
        continue;
      }
      const [y = '', m = ''] = i.date.split('-');
      const months = years.get(y) ?? new Map<string, GridFilterItem[]>();
      years.set(y, months);
      months.set(m, [...(months.get(m) ?? []), i]);
    }
    return { years: [...years.entries()].sort(([a], [b]) => a.localeCompare(b)), rest };
  }, [items]);
  const toggle = (list: GridFilterItem[], on: boolean) => {
    const n = new Set(checked);
    for (const i of list) {
      if (on) n.add(i.text);
      else n.delete(i.text);
    }
    setChecked(n);
  };
  const box = (key: string, label: string, list: GridFilterItem[], depth: number, expandable: boolean) => {
    const all = list.every((i) => checked.has(i.text));
    const some = !all && list.some((i) => checked.has(i.text));
    const isOpen = open.has(key);
    return (
      <div className="kiln-gfx__ftree-row" style={{ paddingLeft: depth * 16 }}>
        {expandable ? (
          <button
            type="button"
            className="kiln-gfx__icon"
            aria-label={`${isOpen ? 'Collapse' : 'Expand'} ${label}`}
            aria-expanded={isOpen}
            onClick={() => {
              const n = new Set(open);
              if (isOpen) n.delete(key);
              else n.add(key);
              setOpen(n);
            }}
          >
            <Icon icon={isOpen ? ChevronDown : ChevronRight} size={12} />
          </button>
        ) : (
          <span className="kiln-gfx__ftree-pad" />
        )}
        <label
          className="kiln-griddlg__check"
          data-testid={expandable ? 'grid-filter-group' : 'grid-filter-value'}
        >
          <input
            type="checkbox"
            checked={all}
            ref={(el) => {
              if (el) el.indeterminate = some;
            }}
            onChange={(e) => toggle(list, e.target.checked)}
          />
          {label}
        </label>
      </div>
    );
  };
  return (
    <fieldset className="kiln-gfx__ftree" aria-label={t('Dates')}>
      {tree.years.map(([y, months]) => {
        const yearItems = [...months.values()].flat();
        return (
          <div key={y}>
            {box(y, y, yearItems, 0, true)}
            {open.has(y)
              ? [...months.entries()]
                  .sort(([a], [b]) => a.localeCompare(b))
                  .map(([m, days]) => (
                    <div key={m}>
                      {box(`${y}-${m}`, MONTHS[Number(m) - 1] ?? m, days, 1, true)}
                      {open.has(`${y}-${m}`)
                        ? days.map((d) => (
                            <div key={d.text}>{box(d.text, d.date?.slice(8) ?? d.text, [d], 2, false)}</div>
                          ))
                        : null}
                    </div>
                  ))
              : null}
          </div>
        );
      })}
      {tree.rest.map((i) => (
        <div key={i.text}>{box(i.text, i.text, [i], 0, false)}</div>
      ))}
    </fieldset>
  );
}
