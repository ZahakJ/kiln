import { ArrowLeftRight } from 'lucide-react';
import { type FormEvent, useEffect, useId, useState } from 'react';
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
        title={mode === 'insert' ? 'Insert Chart' : 'Change Chart Type'}
        footer={
          <>
            <Button variant="primary" onClick={apply}>
              OK
            </Button>
            <DialogClose asChild>
              <Button>Cancel</Button>
            </DialogClose>
          </>
        }
      >
        <div className="kiln-chart-dialog">
          <div
            role="tablist"
            aria-orientation="vertical"
            aria-label="Chart families"
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
                <span>{f.label}</span>
              </button>
            ))}
          </div>
          <div role="tabpanel" className="kiln-chart-dialog__panel">
            <ChartTypeGallery
              families={[fam]}
              value={kind}
              onSelect={setKind}
              thumbSize={64}
              label={fam.label}
            />
            <div className="kiln-chart-dialog__preview">
              <ChartThumb kind={kind} size={240} />
              <div className="kiln-chart-dialog__name">{CHART_KIND_LABELS[kind]}</div>
              {CHART_KIND_HINTS[kind] ? (
                <p className="kiln-chart-dialog__hint">{CHART_KIND_HINTS[kind]}</p>
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
        title="Recommended Charts"
        description="Charts that suit the shape of your selection, best first."
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
              OK
            </Button>
            <DialogClose asChild>
              <Button>Cancel</Button>
            </DialogClose>
          </>
        }
      >
        <div className="kiln-chart-dialog kiln-chart-dialog--recommended">
          <div role="listbox" aria-label="Recommended charts" className="kiln-chart-dialog__families">
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
                <span>{CHART_KIND_LABELS[k]}</span>
              </div>
            ))}
          </div>
          {kind ? (
            <div className="kiln-chart-dialog__preview">
              <ChartThumb kind={kind} size={240} />
              <div className="kiln-chart-dialog__name">{CHART_KIND_LABELS[kind]}</div>
              {CHART_KIND_HINTS[kind] ? (
                <p className="kiln-chart-dialog__hint">{CHART_KIND_HINTS[kind]}</p>
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

/**
 * Select Data Source: the chart data range, the series (legend entries) and the horizontal axis
 * labels, and Switch Row/Column (which applies at once, as in Office).
 */
export function SelectDataDialog({ host, open, onOpenChange }: OpenProps & { host: ChartUiHost }) {
  const id = useId();
  const [range, setRange] = useState(host.state.dataRange);
  useEffect(() => {
    if (open) setRange(host.state.dataRange);
  }, [open, host.state.dataRange]);
  const valid = RANGE_PATTERN.test(range.trim());
  const submit = (e?: FormEvent) => {
    e?.preventDefault();
    if (!valid) return;
    if (range.trim() !== host.state.dataRange) host.run({ ChartSelectData: { range: range.trim() } });
    onOpenChange(false);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="lg"
        title="Select Data Source"
        footer={
          <>
            <Button variant="primary" disabled={!valid} onClick={() => submit()}>
              OK
            </Button>
            <DialogClose asChild>
              <Button>Cancel</Button>
            </DialogClose>
          </>
        }
      >
        <form className="kiln-chart-data" onSubmit={submit}>
          <label htmlFor={`${id}-range`} className="kiln-chart-data__label">
            Chart data range
          </label>
          <input
            id={`${id}-range`}
            className={cx('kiln-field', 'kiln-chart-data__range', !valid && 'is-invalid')}
            value={range}
            spellCheck={false}
            aria-invalid={!valid}
            aria-describedby={`${id}-range-hint`}
            disabled={!host.state.hasCells}
            onChange={(e) => setRange(e.target.value)}
          />
          <p id={`${id}-range-hint`} className="kiln-chart-data__hint">
            {host.state.hasCells
              ? valid
                ? 'Series and labels are read from this range.'
                : 'Type a range such as Sheet1!$A$1:$D$5.'
              : 'This chart keeps its own data; edit it in the chart’s workbook.'}
          </p>
          <div className="kiln-chart-data__switch">
            <Button
              icon={<Icon icon={ArrowLeftRight} size={16} />}
              disabled={!host.state.hasCells}
              onClick={() => host.run('ChartSwitchRowColumn')}
            >
              Switch Row/Column
            </Button>
          </div>
          <div className="kiln-chart-data__lists">
            <section aria-labelledby={`${id}-series`}>
              <h3 id={`${id}-series`} className="kiln-chart-data__heading">
                Legend Entries (Series)
              </h3>
              <ul className="kiln-chart-data__list">
                {host.state.series.map((s) => (
                  <li key={s.idx}>
                    <span className="kiln-chart-data__swatch" style={{ background: s.color }} />
                    {s.name}
                    {s.secondary ? <span className="kiln-chart-data__tag">secondary axis</span> : null}
                  </li>
                ))}
              </ul>
            </section>
            <section aria-labelledby={`${id}-cats`}>
              <h3 id={`${id}-cats`} className="kiln-chart-data__heading">
                Horizontal (Category) Axis Labels
              </h3>
              <ul className="kiln-chart-data__list">
                {host.state.categories.map((c, i) => (
                  // Labels repeat; their position is their identity.
                  <li key={`${i}:${c}`}>{c}</li>
                ))}
              </ul>
            </section>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
