/**
 * The analysis layer over the Grid sheet: the open analysis dialog, the prompt of an
 * empty PivotTable, Flash Fill's greyed suggestions (Enter accepts them), and the menu of a
 * PivotTable field button (sort and the item filter). Positions are the core's (surface CSS
 * pixels).
 */
import { t } from '@kiln/i18n';
import type {
  GridFilterItem,
  GridHit,
  GridPivotInfo,
  GridSheetPermissions,
  GridSparkKind,
} from '@kiln/protocol';
import { Button, Icon } from '@kiln/ui-kit';
import { ArrowDownAZ, ArrowUpZA, Filter, ListFilter } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useStore } from 'zustand';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { useFitInWindow } from './fit.ts';
import { analysisOf, type FieldMenu, type GridAnalysis } from './model.ts';
import {
  AlertDialog,
  ConfirmPasswordDialog,
  GoalSeekDialog,
  GoalSeekStatusDialog,
  ProtectSheetDialog,
  ProtectWorkbookDialog,
  SparkDataDialog,
  SparklinesDialog,
  SparkSettingsDialog,
  SubtotalDialog,
  UnprotectSheetDialog,
  UnprotectWorkbookDialog,
} from './OtherDialogs.tsx';
import {
  CreatePivotDialog,
  FieldSettingsDialog,
  MovePivotDialog,
  PivotOptionsDialog,
  PivotSourceDialog,
  ValueSettingsDialog,
} from './PivotDialogs.tsx';
import './analysis.css';

const NO_FLASH: never[] = [];

export function GridAnalysisLayer({ model }: { model: GridEditorModel }) {
  const a = analysisOf(model);
  const flash = useGrid(model, (s) => s.grid?.analysis.flash ?? NO_FLASH);
  const count = useGrid(model, (s) => s.grid?.analysis.flash_count ?? 0);
  const placeholder = useGrid(model, (s) => s.grid?.analysis.placeholder ?? null);
  const mode = useGrid(model, (s) => s.grid?.mode ?? 'Ready');
  const fieldMenu = useStore(a.store, (s) => s.fieldMenu);

  useEffect(() => {
    // A field button on the sheet opens the field's menu.
    const onHit = (hit: GridHit, client: { x: number; y: number }): boolean => {
      if (typeof hit !== 'object') return false;
      // The outline gutters: a level button, a group's − or +.
      if ('OutlineLevel' in hit) {
        void a.exec({ OutlineLevel: hit.OutlineLevel });
        return true;
      }
      if ('OutlineToggle' in hit) {
        void a.exec({ OutlineToggle: hit.OutlineToggle });
        return true;
      }
      if (!('PivotButton' in hit)) return false;
      const { row, col } = hit.PivotButton;
      void (async () => {
        await a.exec({ SelectCell: { row, col, extend: false, add: false } });
        const g = await model.fetch();
        const p = g?.analysis.pivot;
        const field = p ? buttonField(p, row, col) : null;
        if (field !== null) a.set({ fieldMenu: { field, x: client.x - 8, y: client.y + 12, filter: true } });
      })();
      return true;
    };
    model.hitHooks.push(onHit);
    // Enter accepts Flash Fill's suggestions while they show.
    const onKey = (e: KeyboardEvent) => {
      const s = model.state.grid?.analysis;
      const onSheet = document.activeElement?.hasAttribute('data-kiln-grid-proxy');
      if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && onSheet && s?.flash.length && !a.state.dialog) {
        e.preventDefault();
        e.stopPropagation();
        void a.exec('FlashFill');
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => {
      model.hitHooks.splice(model.hitHooks.indexOf(onHit), 1);
      window.removeEventListener('keydown', onKey, true);
    };
  }, [model, a]);

  return (
    <div className="kiln-gan__layer">
      {placeholder ? (
        <button
          type="button"
          className="kiln-gan__placeholder"
          style={{
            left: placeholder[1][0],
            top: placeholder[1][1],
            width: placeholder[1][2],
            height: placeholder[1][3],
          }}
          data-testid="grid-pivot-placeholder"
          onClick={() => a.set({ fieldsClosed: false })}
        >
          <strong>{placeholder[0]}</strong>
          <span>{t('To build a report, choose fields from the PivotTable Field List')}</span>
        </button>
      ) : null}
      {mode === 'Ready'
        ? flash.map((c) => (
            <span
              key={`${c.row}-${c.col}`}
              className="kiln-gan__ghost"
              style={{ left: c.rect[0], top: c.rect[1], width: c.rect[2], height: c.rect[3] }}
              data-testid="grid-flash-suggestion"
            >
              {c.text}
            </span>
          ))
        : null}
      {mode === 'Ready' && flash.length > 0 && flash[0] ? (
        <div
          className="kiln-gan__flash-tip"
          role="status"
          style={{ left: flash[0].rect[0] + flash[0].rect[2] + 8, top: flash[0].rect[1] }}
        >
          {t('grid.flashFill', { count })}
        </div>
      ) : null}
      {fieldMenu ? <PivotFieldMenu a={a} menu={fieldMenu} /> : null}
      <AnalysisDialogs a={a} />
    </div>
  );
}

/** The field a button of the report stands for. */
function buttonField(p: GridPivotInfo, row: number, col: number): number | null {
  const pageRow = row - p.range.r0;
  if (p.pages.length > 0 && pageRow >= 0 && pageRow < p.pages.length && col === p.range.c0 + 1) {
    return p.pages[pageRow]?.[0] ?? null;
  }
  const rows = p.rows.filter((x) => x >= 0);
  const cols = p.cols.filter((x) => x >= 0);
  if (p.options.layout !== 'Compact') {
    const k = col - p.range.c0;
    const f = p.rows[k];
    if (f !== undefined && f >= 0) return f;
  }
  if (p.active_field !== null && p.active_field !== undefined) return p.active_field;
  return rows[0] ?? cols[0] ?? null;
}

function AnalysisDialogs({ a }: { a: GridAnalysis }) {
  const dialog = useStore(a.store, (s) => s.dialog);
  if (!dialog) return null;
  const arg = dialog.arg;
  switch (dialog.id) {
    case 'createPivot':
      return <CreatePivotDialog a={a} />;
    case 'valueSettings':
      return <ValueSettingsDialog a={a} index={typeof arg === 'number' ? arg : 0} />;
    case 'fieldSettings': {
      const f = typeof arg === 'number' ? arg : (arg as { field: number; section?: string })?.field;
      const section = typeof arg === 'object' && arg ? (arg as { section?: string }).section : undefined;
      return <FieldSettingsDialog a={a} field={f ?? 0} section={section} />;
    }
    case 'pivotOptions':
      return <PivotOptionsDialog a={a} />;
    case 'pivotMove':
      return <MovePivotDialog a={a} />;
    case 'pivotSource':
      return <PivotSourceDialog a={a} />;
    case 'sparklines':
      return <SparklinesDialog a={a} kind={(arg as GridSparkKind) ?? 'Line'} />;
    case 'sparkData':
      return <SparkDataDialog a={a} />;
    case 'sparkHidden':
      return <SparkSettingsDialog a={a} />;
    case 'sparkColor':
      return <SparkSettingsDialog a={a} color={typeof arg === 'string' ? arg : 'markers'} />;
    case 'goalSeek':
      return <GoalSeekDialog a={a} />;
    case 'goalSeekStatus':
      return <GoalSeekStatusDialog a={a} />;
    case 'subtotal':
      return <SubtotalDialog a={a} />;
    case 'protectSheet':
      return <ProtectSheetDialog a={a} />;
    case 'unprotectSheet':
      return <UnprotectSheetDialog a={a} />;
    case 'protectWorkbook':
      return <ProtectWorkbookDialog a={a} />;
    case 'unprotectWorkbook':
      return <UnprotectWorkbookDialog a={a} />;
    case 'confirmPassword': {
      const c = arg as { target: 'sheet' | 'workbook'; password: string; allow?: GridSheetPermissions };
      return <ConfirmPasswordDialog a={a} target={c.target} password={c.password} allow={c.allow} />;
    }
    case 'alert':
      return <AlertDialog a={a} message={typeof arg === 'string' ? arg : ''} />;
    default:
      return null;
  }
}

/** A field button's menu: sort A to Z or Z to A, the filters, and the item list. */
function PivotFieldMenu({ a, menu }: { a: GridAnalysis; menu: FieldMenu }) {
  const fit = useFitInWindow<HTMLDivElement>(menu.x, menu.y);
  const p = useGrid(a.model, (s) => s.grid?.analysis.pivot ?? null);
  const [field, setField] = useState(menu.field);
  const [items, setItems] = useState<GridFilterItem[]>([]);
  const [checked, setChecked] = useState<Set<string> | null>(null);
  const [search, setSearch] = useState('');
  const [multi, setMulti] = useState(false);
  useEffect(() => {
    let live = true;
    void a.features.ask({ PivotItems: { field } }).then((r) => {
      if (!live || !r || !('Items' in r)) return;
      setItems(r.Items);
      setChecked(new Set(r.Items.filter((i) => i.checked).map((i) => i.text)));
    });
    return () => {
      live = false;
    };
  }, [a, field]);
  const close = () => {
    a.set({ fieldMenu: null });
    a.model.surface?.focus();
  };
  const shown = useMemo(
    () => items.filter((i) => i.text.toLowerCase().includes(search.toLowerCase())),
    [items, search],
  );
  if (!p) return null;
  const f = p.fields[field];
  if (!f) return null;
  const page = p.pages.find(([x]) => x === field);
  const axisFields = [...p.rows, ...p.cols].filter((x) => x >= 0);
  const sort = async (sortOrder: 'Ascending' | 'Descending') => {
    await a.edit({ Field: { field, settings: { ...f.settings, sort: sortOrder, sort_by_value: null } } });
    close();
  };
  const apply = async () => {
    const on = checked ?? new Set<string>();
    if (page && !multi) {
      const pick = [...on];
      await a.edit({ Page: { field, item: pick.length === 1 ? (pick[0] ?? null) : null } });
    } else {
      if (page) await a.edit({ Page: { field, item: null } });
      const hidden = items.map((i) => i.text).filter((t) => !on.has(t));
      await a.edit({ Field: { field, settings: { ...f.settings, hidden } } });
    }
    close();
  };
  const all = checked !== null && items.every((i) => checked.has(i.text));
  return (
    <div className="kiln-gan__menu-backdrop" onPointerDown={close}>
      <div
        ref={fit}
        className="kiln-gan__fmenu"
        role="dialog"
        aria-label={t('{name} filter', { name: f.name })}
        data-testid="grid-pivot-field-menu"
        data-kiln-shortcuts="off"
        style={{
          left: Math.max(
            8,
            Math.min(menu.x, (typeof window === 'undefined' ? 1e4 : window.innerWidth) - 270),
          ),
          top: menu.y,
        }}
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            close();
          }
        }}
      >
        {!page && axisFields.length > 1 ? (
          <label className="kiln-gan__select">
            {t('Select field:')}
            <select value={field} onChange={(e) => setField(Number(e.target.value))}>
              {axisFields.map((x) => (
                <option key={x} value={x}>
                  {p.fields[x]?.name}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        {!page ? (
          <>
            <button type="button" className="kiln-gan__mitem" onClick={() => void sort('Ascending')}>
              <Icon icon={ArrowDownAZ} size={14} /> {t('Sort A to Z')}
            </button>
            <button type="button" className="kiln-gan__mitem" onClick={() => void sort('Descending')}>
              <Icon icon={ArrowUpZA} size={14} /> {t('Sort Z to A')}
            </button>
            <button
              type="button"
              className="kiln-gan__mitem"
              onClick={() => {
                a.set({ fieldMenu: null });
                a.openDialog('fieldSettings', { field, section: 'label' });
              }}
            >
              <Icon icon={ListFilter} size={14} /> {t('Label Filters…')}
            </button>
            <button
              type="button"
              className="kiln-gan__mitem"
              onClick={() => {
                a.set({ fieldMenu: null });
                a.openDialog('fieldSettings', { field, section: 'value' });
              }}
            >
              <Icon icon={Filter} size={14} /> {t('Value Filters…')}
            </button>
            <hr />
          </>
        ) : null}
        <input
          className="kiln-griddlg__input"
          type="search"
          placeholder={t('Search')}
          aria-label={t('Search items')}
          // biome-ignore lint/a11y/noAutofocus: the menu takes the keys when it opens, as Excel's.
          autoFocus
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <div className="kiln-gan__items kiln-gan__items--menu">
          {page && !multi ? (
            <label className="kiln-griddlg__check">
              <input
                type="radio"
                name="kiln-gan-page"
                checked={checked !== null && checked.size === items.length}
                onChange={() => setChecked(new Set(items.map((i) => i.text)))}
              />
              {t('(All)')}
            </label>
          ) : (
            <label className="kiln-griddlg__check">
              <input
                type="checkbox"
                checked={all}
                onChange={(e) => setChecked(e.target.checked ? new Set(items.map((i) => i.text)) : new Set())}
              />
              {t('(Select All)')}
            </label>
          )}
          {shown.map((it) => (
            <label key={it.text} className="kiln-griddlg__check" data-testid="grid-pivot-item">
              <input
                type={page && !multi ? 'radio' : 'checkbox'}
                name={page && !multi ? 'kiln-gan-page' : undefined}
                checked={
                  page && !multi
                    ? checked !== null && checked.size === 1 && checked.has(it.text)
                    : !!checked?.has(it.text)
                }
                onChange={(e) => {
                  if (page && !multi) {
                    setChecked(new Set([it.text]));
                    return;
                  }
                  const next = new Set(checked ?? []);
                  if (e.target.checked) next.add(it.text);
                  else next.delete(it.text);
                  setChecked(next);
                }}
              />
              {it.text}
            </label>
          ))}
        </div>
        {page ? (
          <label className="kiln-griddlg__check">
            <input type="checkbox" checked={multi} onChange={(e) => setMulti(e.target.checked)} />
            {t('Select Multiple Items')}
          </label>
        ) : null}
        <div className="kiln-gan__fmenu-foot">
          <Button onClick={close}>{t('Cancel')}</Button>
          <Button
            variant="primary"
            disabled={checked === null || checked.size === 0}
            data-testid="grid-pivot-field-ok"
            onClick={() => void apply()}
          >
            {t('OK')}
          </Button>
        </div>
      </div>
    </div>
  );
}
