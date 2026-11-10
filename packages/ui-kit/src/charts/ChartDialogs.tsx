import { t, tx } from '@kiln/i18n';
import { ArrowLeftRight } from 'lucide-react';
import { type FormEvent, useEffect, useId, useState } from 'react';
import { returnToDocument } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';
import { Dialog, DialogClose, DialogContent } from '../primitives/Dialog.tsx';
import { ChartThumb } from './ChartThumb.tsx';
import { ChartTypeGallery } from './ChartTypeGallery.tsx';
import { CHART_FAMILIES, CHART_KIND_HINTS, CHART_KIND_LABELS, familyOf } from './kinds.ts';
import type { ChartKind, ChartUiHost } from './types.ts';

interface OpenProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Change Chart Type (and Insert Chart's "All Charts" tab): families on the left, the family's
 * variants on the right with a larger preview and Office's description. OK runs `ChartChangeType`
 * (or `ChartInsert` with `mode="insert"`).
 */
export function ChangeChartTypeDialog({
  host,
  open,
  onOpenChange,
  mode = 'change',
}: OpenProps & { host: ChartUiHost; mode?: 'change' | 'insert' }) {
  const [kind, setKind] = useState<ChartKind>(host.state.kind ?? 'ColumnClustered');
  const [family, setFamily] = useState(familyOf(kind).id);
  useEffect(() => {
    if (open) {
      const k = host.state.kind ?? 'ColumnClustered';
      setKind(k);
      setFamily(familyOf(k).id);
    }
  }, [open, host.state.kind]);
  const fam = CHART_FAMILIES.find((f) => f.id === family) ?? familyOf(kind);
  const apply = () => {
    host.run(mode === 'insert' ? { ChartInsert: { kind } } : { ChartChangeType: { kind } });
    onOpenChange(false);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="xl"
        title={mode === 'insert' ? t('Insert Chart') : t('Change Chart Type')}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={() => returnToDocument()}
        footer={
          <>
            <Button variant="primary" onClick={apply}>
              {t('OK')}
            </Button>
            <DialogClose asChild>
              <Button>{t('Cancel')}</Button>
            </DialogClose>
          </>
        }
      >
        <div className="kiln-chart-dialog">
          <div
            role="tablist"
            aria-orientation="vertical"
            aria-label={t('Chart families')}
            className="kiln-chart-dialog__families"
          >
            {CHART_FAMILIES.map((f) => (
              <button
                key={f.id}
                type="button"
                role="tab"
                aria-selected={f.id === family}
                className={cx('kiln-chart-dialog__family', f.id === family && 'is-selected')}
                onClick={() => {
                  setFamily(f.id);
                  setKind(f.kinds[0] ?? kind);
                }}
              >
                <ChartThumb kind={f.kinds[0] ?? 'ColumnClustered'} size={28} />
                <span>{tx(f.label)}</span>
              </button>
            ))}
          </div>
          <div role="tabpanel" className="kiln-chart-dialog__panel">
            <ChartTypeGallery
              families={[fam]}
              value={kind}
              onSelect={setKind}
              thumbSize={64}
              label={tx(fam.label)}
            />
            <div className="kiln-chart-dialog__preview">
              <ChartThumb kind={kind} size={240} />
              <div className="kiln-chart-dialog__name">{tx(CHART_KIND_LABELS[kind])}</div>
              {CHART_KIND_HINTS[kind] ? (
                <p className="kiln-chart-dialog__hint">{tx(CHART_KIND_HINTS[kind])}</p>
              ) : null}
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Insert ▸ Recommended Charts: the core's recommendations for the selection's shape (best first),
 * each with a preview. OK runs `ChartInsert` with the chosen kind.
 */
export function RecommendedChartsDialog({
  host,
  recommended,
  open,
  onOpenChange,
}: OpenProps & { host: ChartUiHost; recommended: readonly ChartKind[] }) {
  const [kind, setKind] = useState<ChartKind | undefined>(recommended[0]);
  useEffect(() => {
    if (open) setKind(recommended[0]);
  }, [open, recommended]);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="lg"
        title={t('Recommended Charts')}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={() => returnToDocument()}
        description={t('Charts that suit the shape of your selection, best first.')}
        footer={
          <>
            <Button
              variant="primary"
              disabled={!kind}
              onClick={() => {
                host.run({ ChartInsert: { kind: kind ?? null } });
                onOpenChange(false);
              }}
            >
              {t('OK')}
            </Button>
            <DialogClose asChild>
              <Button>{t('Cancel')}</Button>
            </DialogClose>
          </>
        }
      >
        <div className="kiln-chart-dialog kiln-chart-dialog--recommended">
          <div role="listbox" aria-label={t('Recommended charts')} className="kiln-chart-dialog__families">
            {recommended.map((k) => (
              <div
                key={k}
                role="option"
                tabIndex={0}
                aria-selected={k === kind}
                className={cx('kiln-chart-dialog__family', k === kind && 'is-selected')}
                onClick={() => setKind(k)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    setKind(k);
                  }
                }}
              >
                <ChartThumb kind={k} size={40} />
                <span>{tx(CHART_KIND_LABELS[k])}</span>
              </div>
            ))}
          </div>
          {kind ? (
            <div className="kiln-chart-dialog__preview">
              <ChartThumb kind={kind} size={240} />
              <div className="kiln-chart-dialog__name">{tx(CHART_KIND_LABELS[kind])}</div>
              {CHART_KIND_HINTS[kind] ? (
                <p className="kiln-chart-dialog__hint">{tx(CHART_KIND_HINTS[kind])}</p>
              ) : null}
            </div>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** `Sheet1!$A$1:$D$5`, `'Q1 sales'!A1:B9` — what Select Data accepts. */
export const RANGE_PATTERN =
  /^(?:'(?:[^']|'')+'|[^\s!'(),[\]]+)!\$?[A-Za-z]{1,3}\$?\d{1,7}(?::\$?[A-Za-z]{1,3}\$?\d{1,7})?$/;

/** A reference typed in Select Data's series and label boxes (`=Sheet1!$B$2:$B$5`). */
const REFERENCE_PATTERN =
  /^=?\s*(?:'(?:[^']|'')+'|[^\s!'(),[\]]+)!\$?[A-Za-z]{1,3}\$?\d{1,7}(?::\$?[A-Za-z]{1,3}\$?\d{1,7})?$/;

type Editing =
  | { kind: 'add'; name: string; values: string }
  | { kind: 'edit'; series: number; name: string; values: string }
  | { kind: 'labels'; range: string }
  | null;

/**
 * Select Data Source: the chart data range; the series (legend entries) with Add, Edit, Remove
 * and Move Up/Down; the horizontal axis labels with Edit; Switch Row/Column; and Hidden and Empty
 * Cells. Each change applies at once, as Switch Row/Column does in Office.
 */
export function SelectDataDialog({ host, open, onOpenChange }: OpenProps & { host: ChartUiHost }) {
  const id = useId();
  const [range, setRange] = useState(host.state.data_range);
  const [selected, setSelected] = useState<number | null>(host.state.series[0]?.idx ?? null);
  const [editing, setEditing] = useState<Editing>(null);
  useEffect(() => {
    if (open) {
      setRange(host.state.data_range);
      setEditing(null);
    }
  }, [open, host.state.data_range]);
  useEffect(() => {
    if (selected === null || !host.state.series.some((s) => s.idx === selected))
      setSelected(host.state.series[0]?.idx ?? null);
  }, [host.state.series, selected]);
  const cells = host.state.has_cells;
  const valid = RANGE_PATTERN.test(range.trim());
  const submit = (e?: FormEvent) => {
    e?.preventDefault();
    if (!valid) return;
    if (range.trim() !== host.state.data_range) host.run({ ChartSelectData: { range: range.trim() } });
    onOpenChange(false);
  };
  const current = host.state.series.find((s) => s.idx === selected);
  const position = host.state.series.findIndex((s) => s.idx === selected);
  const editValid =
    editing === null
      ? false
      : editing.kind === 'labels'
        ? REFERENCE_PATTERN.test(editing.range.trim())
        : REFERENCE_PATTERN.test(editing.values.trim());
  const applyEdit = () => {
    if (!editing || !editValid) return;
    if (editing.kind === 'add')
      host.run({ ChartEditSeries: { op: { Add: { name: editing.name, values: editing.values } } } });
    else if (editing.kind === 'edit')
      host.run({
        ChartEditSeries: {
          op: { Edit: { series: editing.series, name: editing.name, values: editing.values } },
        },
      });
    else host.run({ ChartEditSeries: { op: { Categories: { range: editing.range } } } });
    setEditing(null);
  };
  const blanks = host.state.blanks;
  const showHidden = host.state.show_hidden;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="lg"
        title={t('Select Data Source')}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={() => returnToDocument()}
        footer={
          <>
            <Button variant="primary" disabled={!valid} onClick={() => submit()}>
              {t('OK')}
            </Button>
            <DialogClose asChild>
              <Button>{t('Cancel')}</Button>
            </DialogClose>
          </>
        }
      >
        <form className="kiln-chart-data" onSubmit={submit}>
          <label htmlFor={`${id}-range`} className="kiln-chart-data__label">
            {t('Chart data range')}
          </label>
          <input
            id={`${id}-range`}
            className={cx('kiln-field', 'kiln-chart-data__range', !valid && 'is-invalid')}
            value={range}
            spellCheck={false}
            aria-invalid={!valid}
            aria-describedby={`${id}-range-hint`}
            disabled={!cells}
            onChange={(e) => setRange(e.target.value)}
          />
          <p id={`${id}-range-hint`} className="kiln-chart-data__hint">
            {cells
              ? valid
                ? t('Series and labels are read from this range.')
                : t('Type a range such as Sheet1!$A$1:$D$5.')
              : t('This chart keeps its own data; edit it in the chart’s workbook.')}
          </p>
          <div className="kiln-chart-data__switch">
            <Button
              icon={<Icon icon={ArrowLeftRight} size={16} />}
              disabled={!cells}
              onClick={() => host.run('ChartSwitchRowColumn')}
            >
              {t('Switch Row/Column')}
            </Button>
          </div>
          <div className="kiln-chart-data__lists">
            <section aria-labelledby={`${id}-series`}>
              <h3 id={`${id}-series`} className="kiln-chart-data__heading">
                {t('Legend Entries (Series)')}
              </h3>
              <div className="kiln-chart-data__tools">
                <Button
                  size="sm"
                  disabled={!cells}
                  onClick={() => setEditing({ kind: 'add', name: '', values: '' })}
                >
                  {t('Add')}
                </Button>
                <Button
                  size="sm"
                  disabled={!cells || !current}
                  onClick={() =>
                    current &&
                    setEditing({
                      kind: 'edit',
                      series: current.idx,
                      name: current.name_ref,
                      values: current.values_ref,
                    })
                  }
                >
                  {t('Edit')}
                </Button>
                <Button
                  size="sm"
                  disabled={!cells || !current || host.state.series.length < 2}
                  onClick={() =>
                    current && host.run({ ChartEditSeries: { op: { Remove: { series: current.idx } } } })
                  }
                >
                  {t('Remove')}
                </Button>
                <Button
                  size="sm"
                  aria-label={t('Move Up')}
                  disabled={!cells || position <= 0}
                  onClick={() =>
                    current &&
                    host.run({ ChartEditSeries: { op: { Move: { series: current.idx, up: true } } } })
                  }
                >
                  ▲
                </Button>
                <Button
                  size="sm"
                  aria-label={t('Move Down')}
                  disabled={!cells || position < 0 || position >= host.state.series.length - 1}
                  onClick={() =>
                    current &&
                    host.run({ ChartEditSeries: { op: { Move: { series: current.idx, up: false } } } })
                  }
                >
                  ▼
                </Button>
              </div>
              <div role="listbox" aria-labelledby={`${id}-series`} className="kiln-chart-data__list">
                {host.state.series.map((s) => (
                  <div
                    key={s.idx}
                    role="option"
                    tabIndex={0}
                    aria-selected={s.idx === selected}
                    className={cx('kiln-chart-data__item', s.idx === selected && 'is-selected')}
                    onClick={() => setSelected(s.idx)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setSelected(s.idx);
                      }
                    }}
                  >
                    <span className="kiln-chart-data__swatch" style={{ background: `#${s.color}` }} />
                    {s.name}
                    {s.secondary ? <span className="kiln-chart-data__tag">{t('secondary axis')}</span> : null}
                  </div>
                ))}
              </div>
            </section>
            <section aria-labelledby={`${id}-cats`}>
              <h3 id={`${id}-cats`} className="kiln-chart-data__heading">
                {t('Horizontal (Category) Axis Labels')}
              </h3>
              <div className="kiln-chart-data__tools">
                <Button
                  size="sm"
                  disabled={!cells}
                  onClick={() => setEditing({ kind: 'labels', range: host.state.categories_range })}
                >
                  {t('Edit')}
                </Button>
              </div>
              <ul className="kiln-chart-data__list">
                {host.state.categories.map((c, i) => (
                  // Labels repeat; their position is their identity.
                  <li key={`${i}:${c}`}>{c}</li>
                ))}
              </ul>
            </section>
          </div>
          {editing ? (
            <fieldset className="kiln-chart-data__edit" aria-label={editTitle(editing)}>
              <legend className="kiln-chart-data__heading">{editTitle(editing)}</legend>
              {editing.kind === 'labels' ? (
                <label className="kiln-chart-data__field">
                  {t('Axis label range')}
                  <input
                    className="kiln-field"
                    spellCheck={false}
                    value={editing.range}
                    onChange={(e) => setEditing({ ...editing, range: e.target.value })}
                  />
                </label>
              ) : (
                <>
                  <label className="kiln-chart-data__field">
                    {t('Series name')}
                    <input
                      className="kiln-field"
                      spellCheck={false}
                      value={editing.name}
                      placeholder={t('=Sheet1!$B$1 or a name')}
                      onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                    />
                  </label>
                  <label className="kiln-chart-data__field">
                    {t('Series values')}
                    <input
                      className="kiln-field"
                      spellCheck={false}
                      value={editing.values}
                      placeholder={t('=Sheet1!$B$2:$B$5')}
                      onChange={(e) => setEditing({ ...editing, values: e.target.value })}
                    />
                  </label>
                </>
              )}
              <div className="kiln-chart-data__tools">
                <Button size="sm" variant="primary" disabled={!editValid} onClick={applyEdit}>
                  {t('Apply')}
                </Button>
                <Button size="sm" onClick={() => setEditing(null)}>
                  {t('Cancel')}
                </Button>
              </div>
            </fieldset>
          ) : null}
          <fieldset className="kiln-chart-data__edit" disabled={!cells}>
            <legend className="kiln-chart-data__heading">{t('Hidden and Empty Cells')}</legend>
            <div className="kiln-chart-data__radios" role="radiogroup" aria-label={t('Show empty cells as')}>
              {(
                [
                  ['Gap', t('Gaps')],
                  ['Zero', t('Zero')],
                  ['Span', t('Connect data points with line')],
                ] as const
              ).map(([v, label]) => (
                <label key={v} className="kiln-chart-pane__check">
                  <input
                    type="radio"
                    name={`${id}-blanks`}
                    checked={blanks === v}
                    onChange={() => host.run({ ChartHiddenEmpty: { blanks: v, show_hidden: showHidden } })}
                  />
                  {label}
                </label>
              ))}
            </div>
            <label className="kiln-chart-pane__check">
              <input
                type="checkbox"
                checked={showHidden}
                onChange={(e) => host.run({ ChartHiddenEmpty: { blanks, show_hidden: e.target.checked } })}
              />
              {t('Show data in hidden rows and columns')}
            </label>
          </fieldset>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function editTitle(e: NonNullable<Editing>): string {
  return e.kind === 'add' ? t('Add Series') : e.kind === 'edit' ? t('Edit Series') : t('Axis Labels');
}
