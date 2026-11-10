/**
 * Data ▸ Sort: levels (Sort by / Then by), Sort On (cell values, cell colour, font colour,
 * conditional-formatting icon), Order (A to Z, Z to A, a custom list, the colour on top or at
 * the bottom), Add, Delete and Copy Level, My data has headers, and Options (case sensitive,
 * left to right). The range and its headers are the core's answer.
 */
import { t } from '@kiln/i18n';
import type { GridDataRange, GridSortKey, GridSortOn } from '@kiln/protocol';
import { Button, Select } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import { ColorButton, hexOfChoice } from '../../dialogs/ColorButton.tsx';
import type { GridFeatures } from '../model.ts';
import { Check, columnLabel, FeatureFrame, rangeA1 } from '../ui.tsx';

/** Excel's built-in custom lists. */
export const CUSTOM_LISTS: readonly (readonly string[])[] = [
  ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'],
  ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'],
  [
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
  ],
  ['Low', 'Medium', 'High'],
];

const LISTS_KEY = 'kiln.grid.custom_lists';

/** The custom lists: Excel's built-in ones, then the user's (Custom List… adds them). */
function customLists(): (readonly string[])[] {
  try {
    const v = JSON.parse(localStorage.getItem(LISTS_KEY) ?? '[]') as unknown;
    if (Array.isArray(v) && v.every((l) => Array.isArray(l) && l.every((x) => typeof x === 'string')))
      return [...CUSTOM_LISTS, ...(v as string[][])];
  } catch {
    // Private mode or a damaged entry.
  }
  return [...CUSTOM_LISTS];
}

function saveList(list: string[]): void {
  try {
    const own = customLists().slice(CUSTOM_LISTS.length);
    localStorage.setItem(LISTS_KEY, JSON.stringify([...own, list]));
  } catch {
    // Private mode: the list lasts for the dialog.
  }
}

interface Level {
  index: number;
  on: GridSortOn;
  order: 'asc' | 'desc' | `list:${number}`;
  color: string | null;
}

export function SortDialog({ features }: { features: GridFeatures }) {
  const [info, setInfo] = useState<GridDataRange | null>(null);
  const [header, setHeader] = useState(true);
  const [levels, setLevels] = useState<Level[]>([]);
  const [selected, setSelected] = useState(0);
  const [caseSensitive, setCase] = useState(false);
  const [byColumns, setByColumns] = useState(false);
  const [options, setOptions] = useState(false);
  const [lists, setLists] = useState(customLists);
  // Custom List… of a level: the list picked, or a new one typed.
  const [listFor, setListFor] = useState<{ level: number; pick: number; entries: string } | null>(null);
  useEffect(() => {
    void features.ask('DataRange').then((r) => {
      if (r && 'DataRange' in r) {
        const d = r.DataRange;
        setInfo(d);
        setHeader(d.header);
        setLevels([{ index: d.range.c0, on: 'Values', order: 'asc', color: null }]);
        // Excel opens on Sort by.
        requestAnimationFrame(() =>
          document.querySelector<HTMLElement>('[data-testid="grid-sort"] [aria-label="Sort by"]')?.focus(),
        );
      }
    });
  }, [features]);
  if (!info)
    return (
      <FeatureFrame features={features} title={t('Sort')} testId="grid-sort">
        {null}
      </FeatureFrame>
    );
  const r = info.range;
  const span = byColumns ? [r.r0, r.r1] : [r.c0, r.c1];
  const columns: { value: string; label: string }[] = [];
  for (let i = span[0] ?? 0; i <= (span[1] ?? 0) && columns.length < 500; i++) {
    const label = byColumns
      ? t('Row {n, number}', { n: i + 1 })
      : columnLabel(header ? info.headers[i - r.c0] : undefined, i);
    columns.push({ value: String(i), label });
  }
  const update = (k: number, patch: Partial<Level>) =>
    setLevels((ls) => ls.map((l, i) => (i === k ? { ...l, ...patch } : l)));
  const keys = (): GridSortKey[] =>
    levels.map((l) => {
      const list = l.order.startsWith('list:') ? lists[Number(l.order.slice(5))] : undefined;
      return {
        index: l.index,
        descending: l.order === 'desc',
        on: l.on,
        color: l.on === 'CellColor' || l.on === 'FontColor' ? l.color : null,
        icon: null,
        custom_list: list ? [...list] : null,
      };
    });
  return (
    <FeatureFrame
      features={features}
      title={t('Sort')}
      size="lg"
      testId="grid-sort"
      okDisabled={levels.length === 0}
      onOk={() =>
        features.tryExec({
          ApplySort: {
            spec: { range: r, keys: keys(), header, case_sensitive: caseSensitive, by_columns: byColumns },
          },
        })
      }
    >
      <div className="kiln-gfx__toolbar">
        <Button
          size="sm"
          onClick={() =>
            setLevels((ls) =>
              [...ls, { index: r.c0, on: 'Values' as const, order: 'asc' as const, color: null }].slice(
                0,
                64,
              ),
            )
          }
        >
          {t('Add Level')}
        </Button>
        <Button
          size="sm"
          disabled={levels.length === 0}
          onClick={() => {
            setLevels((ls) => ls.filter((_, i) => i !== selected));
            setSelected((s) => Math.max(0, s - 1));
          }}
        >
          {t('Delete Level')}
        </Button>
        <Button
          size="sm"
          disabled={levels.length === 0}
          onClick={() =>
            setLevels((ls) => [
              ...ls.slice(0, selected + 1),
              { ...ls[selected] } as Level,
              ...ls.slice(selected + 1),
            ])
          }
        >
          {t('Copy Level')}
        </Button>
        <Button size="sm" onClick={() => setOptions((o) => !o)} aria-expanded={options}>
          {t('Options…')}
        </Button>
        <span className="kiln-gfx__grow" />
        <Check
          label={t('My data has headers')}
          checked={header}
          onChange={setHeader}
          testId="grid-sort-header"
        />
      </div>
      {options ? (
        <div className="kiln-gfx__options">
          <Check label={t('Case sensitive')} checked={caseSensitive} onChange={setCase} />
          <Check label={t('Sort left to right')} checked={byColumns} onChange={setByColumns} />
        </div>
      ) : null}
      <div className="kiln-gfx__table">
        <div className="kiln-gfx__thead">
          <span>{t('Column')}</span>
          <span>{t('Sort On')}</span>
          <span>{t('Order')}</span>
        </div>
        {levels.map((l, k) => (
          // biome-ignore lint/a11y/noStaticElementInteractions: clicking a level row selects it; its selects take the keys.
          <div
            key={`${k}-${l.index}`}
            tabIndex={-1}
            className={k === selected ? 'kiln-gfx__level kiln-gfx__level--on' : 'kiln-gfx__level'}
            onFocus={() => setSelected(k)}
            onClick={() => setSelected(k)}
            onKeyDown={() => undefined}
          >
            <span className="kiln-gfx__level-label">{k === 0 ? t('Sort by') : t('Then by')}</span>
            <Select
              aria-label={k === 0 ? t('Sort by') : t('Then by')}
              value={String(l.index)}
              options={columns}
              width="100%"
              onValueChange={(v) => update(k, { index: Number(v) })}
            />
            <Select
              aria-label={t('Sort On')}
              value={l.on}
              width="100%"
              options={[
                { value: 'Values', label: t('Cell Values') },
                { value: 'CellColor', label: t('Cell Color') },
                { value: 'FontColor', label: t('Font Color') },
                { value: 'Icon', label: t('Conditional Formatting Icon') },
              ]}
              onValueChange={(v) => update(k, { on: v as GridSortOn })}
            />
            {l.on === 'CellColor' || l.on === 'FontColor' ? (
              <span className="kiln-gfx__inline">
                <ColorButton
                  label={t('Colour')}
                  value={l.color}
                  onChange={(c) => update(k, { color: hexOfChoice(c) })}
                />
                <Select
                  aria-label={t('Order')}
                  value={l.order === 'desc' ? 'desc' : 'asc'}
                  width={110}
                  options={[
                    { value: 'asc', label: t('On Top') },
                    { value: 'desc', label: t('On Bottom') },
                  ]}
                  onValueChange={(v) => update(k, { order: v as 'asc' | 'desc' })}
                />
              </span>
            ) : (
              <Select
                aria-label={t('Order')}
                value={l.order}
                width="100%"
                options={[
                  { value: 'asc', label: t('A to Z (Smallest to Largest)') },
                  { value: 'desc', label: t('Z to A (Largest to Smallest)') },
                  ...lists.map((list, i) => ({
                    value: `list:${i}`,
                    label: list.length > 3 ? `${list.slice(0, 3).join(', ')}…` : list.join(', '),
                  })),
                  { value: 'custom', label: t('Custom List…') },
                ]}
                onValueChange={(v) => {
                  if (v === 'custom') {
                    const pick = l.order.startsWith('list:') ? Number(l.order.slice(5)) : 0;
                    setListFor({ level: k, pick, entries: '' });
                    return;
                  }
                  update(k, { order: v as Level['order'] });
                }}
              />
            )}
          </div>
        ))}
      </div>
      {listFor ? (
        <fieldset
          className="kiln-gfx__options"
          aria-label={t('Custom Lists')}
          data-testid="grid-custom-lists"
        >
          <legend>{t('Custom Lists')}</legend>
          <Select
            aria-label={t('Custom lists')}
            value={String(listFor.pick)}
            width="100%"
            options={[
              ...lists.map((list, i) => ({ value: String(i), label: list.join(', ') })),
              { value: '-1', label: t('NEW LIST') },
            ]}
            onValueChange={(v) => setListFor({ ...listFor, pick: Number(v) })}
          />
          {listFor.pick < 0 ? (
            <label className="kiln-griddlg__field">
              <span>{t('List entries (one per line or separated by commas)')}</span>
              <textarea
                aria-label={t('List entries')}
                rows={4}
                value={listFor.entries}
                onChange={(e) => setListFor({ ...listFor, entries: e.target.value })}
              />
            </label>
          ) : null}
          <div className="kiln-gfx__toolbar">
            <span className="kiln-gfx__grow" />
            <Button size="sm" onClick={() => setListFor(null)}>
              {t('Cancel')}
            </Button>
            <Button
              size="sm"
              variant="primary"
              onClick={() => {
                let pick = listFor.pick;
                if (pick < 0) {
                  const entries = listFor.entries
                    .split(/[\n,]/)
                    .map((x) => x.trim())
                    .filter(Boolean);
                  if (entries.length === 0) return;
                  saveList(entries);
                  setLists((ls) => [...ls, entries]);
                  pick = lists.length;
                }
                update(listFor.level, { order: `list:${pick}` });
                setListFor(null);
              }}
            >
              {t('OK')}
            </Button>
          </div>
        </fieldset>
      ) : null}
      <p className="kiln-gfx__hint">{t('Sorts {rangeA1}.', { rangeA1: rangeA1(r) })}</p>
    </FeatureFrame>
  );
}
