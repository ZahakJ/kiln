/**
 * The PivotTable Fields pane: the source fields with their check boxes (a ticked number
 * goes to Values, anything else to Rows, as in Excel), a search box, and the four areas —
 * Filters, Columns, Rows, Values — whose fields are dragged between areas, reordered, dragged out
 * to remove them, or moved with each field's menu (Move Up, Move to Rows…, Field Settings). Every
 * change is one core command; what the pane shows is the core's PivotTable state.
 */
import { msg, t, tx } from '@kiln/i18n';
import type { GridPivotArea, GridPivotInfo } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import { ChevronDown, GripVertical, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useStore } from 'zustand';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { useFitInWindow } from './fit.ts';
import { analysisOf, type GridAnalysis } from './model.ts';

/** What is dragged: a field from the list, a field of an area, a value field or Σ Values. */
type Drag =
  | { kind: 'field'; field: number }
  | { kind: 'axis'; field: number; from: GridPivotArea }
  | { kind: 'value'; index: number }
  | { kind: 'values'; from: GridPivotArea };

const MIME = 'application/x-kiln-pivot-field';
const AREAS: readonly (readonly [GridPivotArea, string])[] = [
  ['Filters', msg('Filters')],
  ['Columns', msg('Columns')],
  ['Rows', msg('Rows')],
  ['Values', msg('Σ Values')],
];

/** An area's entries: (label, what dragging it moves, field index for menus). */
function entries(p: GridPivotInfo, area: GridPivotArea): { label: string; drag: Drag; key: string }[] {
  const axis = (list: number[], from: GridPivotArea) =>
    list.map((x, i) =>
      x < 0
        ? { label: t('Σ Values'), drag: { kind: 'values', from } as Drag, key: `v${i}` }
        : {
            label: p.fields[x]?.settings.caption ?? p.fields[x]?.name ?? '',
            drag: { kind: 'axis', field: x, from } as Drag,
            key: `f${x}`,
          },
    );
  switch (area) {
    case 'Rows':
      return axis(p.rows, 'Rows');
    case 'Columns':
      return axis(p.cols, 'Columns');
    case 'Filters':
      return p.pages.map(([f]) => ({
        label: p.fields[f]?.settings.caption ?? p.fields[f]?.name ?? '',
        drag: { kind: 'axis', field: f, from: 'Filters' } as Drag,
        key: `f${f}`,
      }));
    case 'Values':
      return p.values.map((v, i) => ({
        label: v.name,
        drag: { kind: 'value', index: i } as Drag,
        key: `d${i}`,
      }));
  }
}

export function PivotFieldsPane({ model }: { model: GridEditorModel }) {
  const a = analysisOf(model);
  const p = useGrid(model, (s) => s.grid?.analysis.pivot ?? null);
  const closed = useStore(a.store, (s) => s.fieldsClosed);
  const [search, setSearch] = useState('');
  const [menu, setMenu] = useState<MenuAt | null>(null);
  if (!p || closed) return null;
  const shown = p.fields
    .map((f, i) => ({ f, i }))
    .filter(({ f }) => f.name.toLowerCase().includes(search.toLowerCase()));
  const used = (i: number) => p.fields[i]?.area !== null || (p.fields[i]?.values ?? 0) > 0;
  const toggle = (i: number) => {
    const f = p.fields[i];
    if (!f) return;
    if (used(i)) return a.edit({ Remove: { field: i } });
    return a.edit({ Place: { field: i, area: f.numeric ? 'Values' : 'Rows', position: null } });
  };
  const drop = async (d: Drag, area: GridPivotArea | null, at: number | null) => {
    if (area === null) {
      // Dragged out of the areas: removed.
      if (d.kind === 'axis') await a.edit({ Remove: { field: d.field } });
      else if (d.kind === 'value') await a.edit({ RemoveValue: { index: d.index } });
      return;
    }
    // A position counted with the dragged entry in place, as the core takes it without.
    const from =
      d.kind === 'value' && area === 'Values'
        ? d.index
        : (d.kind === 'axis' || d.kind === 'values') && d.from === area
          ? entries(p, area).findIndex((en) => JSON.stringify(en.drag) === JSON.stringify(d))
          : -1;
    const position = at !== null && from >= 0 && at > from ? at - 1 : at;
    if (d.kind === 'value') {
      if (area === 'Values') {
        if (position !== null && position !== from) await a.edit({ MoveValue: { index: d.index, position } });
        return;
      }
      // A value field dragged to an axis: the field goes there instead.
      const v = p.values[d.index];
      if (!v) return;
      await a.edit({ RemoveValue: { index: d.index } });
      await a.edit({ Place: { field: v.field, area, position } });
      return;
    }
    if (d.kind === 'values') {
      if (area === 'Rows' || area === 'Columns') await a.edit({ Place: { field: null, area, position } });
      return;
    }
    if (d.kind === 'axis' && area === 'Values') {
      // From an axis to Values: the field leaves the axis.
      await a.edit({ Remove: { field: d.field } });
    }
    await a.edit({ Place: { field: d.field, area, position } });
  };
  return (
    <aside
      className="kiln-gpane kiln-gan__pane"
      aria-label={t('PivotTable Fields')}
      data-testid="grid-pivot-fields"
      data-kiln-shortcuts="off"
      onKeyDown={(e) => {
        // Esc hands the keys back to the sheet, as from the other panes (a menu closes first).
        if (e.key !== 'Escape' || e.defaultPrevented) return;
        e.preventDefault();
        model.surface?.focus();
      }}
    >
      <header className="kiln-gpane__head">
        <h2>{t('PivotTable Fields')}</h2>
        <button
          type="button"
          className="kiln-gpane__close"
          aria-label={t('Close PivotTable Fields')}
          onClick={() => {
            a.set({ fieldsClosed: true });
            model.surface?.focus();
          }}
        >
          <Icon icon={X} size={16} />
        </button>
      </header>
      <div className="kiln-gpane__body">
        <p className="kiln-gfx__hint">{t('Choose fields to add to report:')}</p>
        <input
          className="kiln-griddlg__input"
          type="search"
          placeholder={t('Search')}
          aria-label={t('Search fields')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <ul
          className="kiln-gan__fields"
          aria-label={t('Fields')}
          onDragOver={(e) => {
            if (e.dataTransfer.types.includes(MIME)) e.preventDefault();
          }}
          onDrop={(e) => {
            const d = readDrag(e);
            if (d) {
              e.preventDefault();
              void drop(d, null, null);
            }
          }}
        >
          {shown.map(({ f, i }) => (
            <li
              key={i}
              className="kiln-gan__field"
              draggable
              onDragStart={(e) => writeDrag(e, { kind: 'field', field: i })}
            >
              <label>
                <input
                  type="checkbox"
                  checked={used(i)}
                  data-testid="grid-pivot-field"
                  onChange={() => void toggle(i)}
                />
                <span className={f.filtered ? 'kiln-gan__name kiln-gan__name--filtered' : 'kiln-gan__name'}>
                  {f.name}
                </span>
              </label>
              <Icon icon={GripVertical} size={14} className="kiln-gan__grip" />
            </li>
          ))}
        </ul>
        <p className="kiln-gfx__hint">{t('Drag fields between areas below:')}</p>
        <div className="kiln-gan__areas">
          {AREAS.map(([area, title]) => (
            <AreaBox
              key={area}
              area={area}
              title={tx(title)}
              list={entries(p, area)}
              onDrop={(d, pos) => void drop(d, area, pos)}
              onMenu={(drag, chip) => {
                const r = chip.getBoundingClientRect();
                setMenu({ drag, x: r.left, y: r.bottom + 2, chip });
              }}
            />
          ))}
        </div>
      </div>
      {menu ? (
        <EntryMenu
          a={a}
          p={p}
          at={menu}
          onClose={(to) => {
            setMenu(null);
            // The keys go back to the field's button (Esc, a click outside), or to the sheet
            // after a move, as after any menu command; a dialog takes them itself.
            if (to === 'chip' && menu.chip.isConnected) menu.chip.focus();
            else if (to === 'sheet') model.surface?.focus();
          }}
          drop={drop}
        />
      ) : null}
    </aside>
  );
}

function writeDrag(e: React.DragEvent, d: Drag): void {
  e.dataTransfer.setData(MIME, JSON.stringify(d));
  e.dataTransfer.setData('text/plain', JSON.stringify(d));
  e.dataTransfer.effectAllowed = 'move';
}

function readDrag(e: React.DragEvent): Drag | null {
  try {
    const t = e.dataTransfer.getData(MIME) || e.dataTransfer.getData('text/plain');
    return t ? (JSON.parse(t) as Drag) : null;
  } catch {
    return null;
  }
}

function AreaBox({
  area,
  title,
  list,
  onDrop,
  onMenu,
}: {
  area: GridPivotArea;
  title: string;
  list: { label: string; drag: Drag; key: string }[];
  onDrop: (d: Drag, position: number | null) => void;
  onMenu: (d: Drag, chip: HTMLElement) => void;
}) {
  const [over, setOver] = useState<number | null>(null);
  const at = (e: React.DragEvent): number => {
    // The position among the entries the pointer is over (before the entry under it).
    const items = Array.from((e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[data-entry]'));
    const y = e.clientY;
    const i = items.findIndex((el) => {
      const r = el.getBoundingClientRect();
      return y < r.top + r.height / 2;
    });
    return i < 0 ? items.length : i;
  };
  return (
    <section
      className={over !== null ? 'kiln-gan__area kiln-gan__area--over' : 'kiln-gan__area'}
      aria-label={t('{area} area', { area })}
      data-testid={`grid-pivot-area-${area.toLowerCase()}`}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(MIME) && !e.dataTransfer.types.includes('text/plain')) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        setOver(at(e));
      }}
      onDragLeave={() => setOver(null)}
      onDrop={(e) => {
        const d = readDrag(e);
        const pos = at(e);
        setOver(null);
        if (!d) return;
        e.preventDefault();
        e.stopPropagation();
        onDrop(d, pos);
      }}
    >
      <h3>{title}</h3>
      <ul>
        {list.map((en, i) => (
          <li key={en.key} data-entry>
            {over === i ? <span className="kiln-gan__insert" aria-hidden="true" /> : null}
            <button
              type="button"
              className="kiln-gan__chip"
              draggable
              data-testid="grid-pivot-chip"
              onDragStart={(e) => writeDrag(e, en.drag)}
              onClick={(e) => onMenu(en.drag, e.currentTarget)}
            >
              <span>{en.label}</span>
              <Icon icon={ChevronDown} size={12} />
            </button>
          </li>
        ))}
        {over === list.length ? <span className="kiln-gan__insert" aria-hidden="true" /> : null}
      </ul>
    </section>
  );
}

/** Where a field's menu opens, and the field's button it belongs to. */
interface MenuAt {
  drag: Drag;
  x: number;
  y: number;
  chip: HTMLElement;
}

/** Arrow keys, Home and End move between the menu's items that can run, wrapping round. */
function rove(menu: HTMLElement, key: string): void {
  const items = [...menu.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)')];
  if (!items.length) return;
  const i = items.indexOf(document.activeElement as HTMLElement);
  const n = items.length;
  const next =
    key === 'Home'
      ? 0
      : key === 'End'
        ? n - 1
        : i < 0
          ? key === 'ArrowDown'
            ? 0
            : n - 1
          : (i + (key === 'ArrowDown' ? 1 : -1) + n) % n;
  items[next]?.focus();
}

/** A field's menu in an area: move it, or open its settings. */
function EntryMenu({
  a,
  p,
  at,
  onClose,
  drop,
}: {
  a: GridAnalysis;
  p: GridPivotInfo;
  at: MenuAt;
  onClose: (focus: 'chip' | 'sheet' | 'none') => void;
  drop: (d: Drag, area: GridPivotArea | null, position: number | null) => Promise<void>;
}) {
  const fit = useFitInWindow<HTMLDivElement>(at.x, at.y);
  const d = at.drag;
  const area: GridPivotArea =
    d.kind === 'value' ? 'Values' : d.kind === 'values' || d.kind === 'axis' ? d.from : 'Rows';
  const list = entries(p, area);
  const index = list.findIndex((en) => JSON.stringify(en.drag) === JSON.stringify(d));
  const run =
    (f: () => Promise<void> | void, focus: 'sheet' | 'none' = 'sheet') =>
    async () => {
      onClose(focus);
      await f();
    };
  const items: [string, () => Promise<void>, boolean][] = [
    ['Move Up', run(() => drop(d, area, Math.max(0, index - 1))), index > 0],
    ['Move Down', run(() => drop(d, area, index + 2)), index < list.length - 1],
    ['Move to Beginning', run(() => drop(d, area, 0)), index > 0],
    ['Move to End', run(() => drop(d, area, list.length)), index < list.length - 1],
    ['Move to Report Filter', run(() => drop(d, 'Filters', null)), area !== 'Filters' && d.kind !== 'values'],
    ['Move to Row Labels', run(() => drop(d, 'Rows', null)), area !== 'Rows'],
    ['Move to Column Labels', run(() => drop(d, 'Columns', null)), area !== 'Columns'],
    ['Move to Values', run(() => drop(d, 'Values', null)), area !== 'Values' && d.kind !== 'values'],
    ['Remove Field', run(() => drop(d, null, null)), d.kind !== 'values'],
  ];
  const settings =
    d.kind === 'value'
      ? (['Value Field Settings…', run(() => a.openDialog('valueSettings', d.index), 'none')] as const)
      : d.kind === 'axis'
        ? (['Field Settings…', run(() => a.openDialog('fieldSettings', d.field), 'none')] as const)
        : null;
  useEffect(() => {
    // The first item that can run has the keys (Move Up is off for the first field).
    fit.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
  }, [fit]);
  return (
    <div
      className="kiln-gan__menu-backdrop"
      onPointerDown={(e) => {
        // The focus would fall to the page as the menu goes: it goes back to the field.
        e.preventDefault();
        onClose('chip');
      }}
    >
      <div
        ref={fit}
        className="kiln-gan__menu"
        role="menu"
        aria-label={t('Field')}
        style={{ left: at.x, top: at.y }}
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            onClose('chip');
          } else if (e.key === 'Tab') {
            e.preventDefault();
            onClose('chip');
          } else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) {
            e.preventDefault();
            rove(e.currentTarget, e.key);
          }
        }}
      >
        {items.map(([label, go, enabled]) => (
          <button
            key={label}
            type="button"
            role="menuitem"
            tabIndex={-1}
            disabled={!enabled}
            onClick={() => void go()}
          >
            {label}
          </button>
        ))}
        {settings ? (
          <>
            <hr />
            <button type="button" role="menuitem" tabIndex={-1} onClick={() => void settings[1]()}>
              {settings[0]}
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
