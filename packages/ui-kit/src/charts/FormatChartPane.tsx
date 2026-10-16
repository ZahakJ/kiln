import { RotateCcw } from 'lucide-react';
import { type ReactNode, useEffect, useId, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { Select } from '../primitives/Select.tsx';
import {
  bound,
  type ChartAxisInfo,
  type ChartAxisTarget,
  type ChartCommand,
  type ChartUiHost,
} from './types.ts';

type FormatAxisArgs = Extract<ChartCommand, { ChartFormatAxis: unknown }>['ChartFormatAxis'];

/** Swatches offered for series fill and line: the Kiln palette, then neutrals. */
export const SERIES_SWATCHES: readonly { hex: string; label: string }[] = [
  { hex: '1D8A60', label: 'Jade' },
  { hex: 'E05A27', label: 'Ember' },
  { hex: '3159C6', label: 'Cobalt' },
  { hex: 'E3A126', label: 'Saffron' },
  { hex: '8A5A3F', label: 'Clay' },
  { hex: '8D4E85', label: 'Plum' },
  { hex: '2B1C14', label: 'Soot' },
  { hex: '8F7A6A', label: 'Ash' },
];

const NUMBER_FORMATS: readonly { value: string; label: string }[] = [
  { value: '', label: 'Linked to source' },
  { value: 'General', label: 'General' },
  { value: '0', label: 'Number (0)' },
  { value: '0.00', label: 'Number (0.00)' },
  { value: '#,##0', label: 'Thousands (#,##0)' },
  { value: '0%', label: 'Percentage' },
  { value: '"£"#,##0', label: 'Currency (£)' },
  { value: '"$"#,##0', label: 'Currency ($)' },
  { value: '#,##0,"K"', label: 'Thousands (K)' },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section className="kiln-chart-pane__section" aria-labelledby={id}>
      <h3 id={id} className="kiln-chart-pane__heading">
        {title}
      </h3>
      {children}
    </section>
  );
}

function SwatchRow({
  label,
  value,
  current,
  onPick,
}: {
  label: string;
  /** The colour set on the series (`RRGGBB`), `null` when automatic. */
  value: string | null;
  /** The colour drawn (`RRGGBB`), shown on the Automatic chip; `null` when nothing is drawn. */
  current: string | null;
  onPick: (hex: string) => void;
}) {
  const id = useId();
  const set = value?.toUpperCase() ?? null;
  const custom = set !== null && !SERIES_SWATCHES.some((s) => s.hex === set);
  return (
    <div className="kiln-chart-pane__colour">
      <span id={id} className="kiln-chart-pane__label">
        {label}
      </span>
      {/* biome-ignore lint/a11y/useSemanticElements: a radio-like row of colour buttons. */}
      <div role="group" aria-labelledby={id} className="kiln-chart-pane__swatches">
        <button
          type="button"
          className="kiln-chart-pane__swatch kiln-chart-pane__swatch--auto"
          aria-label={`${label}: Automatic`}
          title="Automatic"
          aria-pressed={set === null}
          onClick={() => onPick('')}
        >
          {current === null ? (
            <span className="kiln-chart-pane__auto-chip kiln-chart-pane__auto-chip--none" />
          ) : (
            <span className="kiln-chart-pane__auto-chip" style={{ background: `#${current}` }} />
          )}
        </button>
        {SERIES_SWATCHES.map((s) => (
          <button
            key={s.hex}
            type="button"
            className="kiln-chart-pane__swatch"
            style={{ background: `#${s.hex}` }}
            aria-label={`${label}: ${s.label}`}
            title={s.label}
            aria-pressed={set === s.hex}
            onClick={() => onPick(s.hex)}
          />
        ))}
        {custom ? (
          <span
            className="kiln-chart-pane__swatch kiln-chart-pane__swatch--custom"
            style={{ background: `#${set}` }}
            role="img"
            aria-label={`${label}: #${set} (current)`}
            title={`#${set}`}
          />
        ) : null}
      </div>
    </div>
  );
}

function AxisBoundRow({
  label,
  id,
  value,
  auto,
  min,
  onSet,
  onAuto,
}: {
  label: string;
  id: string;
  value: number;
  auto: boolean;
  min?: number;
  onSet: (v: number) => void;
  onAuto: () => void;
}) {
  return (
    <div className="kiln-chart-pane__row">
      <label htmlFor={id}>{label}</label>
      <NumberInput
        id={id}
        aria-label={label}
        value={value}
        precision={6}
        size="sm"
        width={104}
        {...(min !== undefined ? { min } : {})}
        onValueChange={onSet}
      />
      {auto ? (
        <span className="kiln-chart-pane__auto" title="Office chooses this value from the data">
          Auto
        </span>
      ) : (
        <Button
          variant="ghost"
          size="sm"
          aria-label={`Reset ${label.toLowerCase()} to automatic`}
          icon={<Icon icon={RotateCcw} size={14} />}
          onClick={onAuto}
        >
          Reset
        </Button>
      )}
    </div>
  );
}

/**
 * The Format Chart pane (Format Selection / double-click): title, series fill and line, secondary
 * axis, and axis options with the bounds the chart is drawn with (automatic ones marked Auto,
 * fixed ones with Reset). Every value comes from the core's `ChartState`; every change runs one
 * chart command.
 */
export function FormatChartPane({ host }: { host: ChartUiHost }) {
  const { state, run } = host;
  const ids = useId();
  const [title, setTitle] = useState(state.title ?? '');
  useEffect(() => setTitle(state.title ?? ''), [state.title]);
  const [seriesIdx, setSeriesIdx] = useState<number>(state.series[0]?.idx ?? 0);
  const series = state.series.find((s) => s.idx === seriesIdx) ?? state.series[0];
  const [target, setTarget] = useState<ChartAxisTarget>(state.axes[0]?.target ?? 'PrimaryValue');
  const axis: ChartAxisInfo | undefined = state.axes.find((a) => a.target === target) ?? state.axes[0];

  const formatSeries = (
    patch: Partial<
      Omit<Extract<ChartCommand, { ChartFormatSeries: unknown }>['ChartFormatSeries'], 'series'>
    >,
  ) => {
    if (!series) return;
    run({
      ChartFormatSeries: {
        series: series.idx,
        fill: null,
        line: null,
        line_width: null,
        secondary: null,
        ...patch,
      },
    });
  };

  const formatAxis = (patch: Partial<Omit<FormatAxisArgs, 'axis'>>) => {
    if (!axis) return;
    run({
      ChartFormatAxis: {
        axis: axis.target,
        min: null,
        max: null,
        major_unit: null,
        log_base: null,
        reversed: null,
        number_format: null,
        ...patch,
      },
    });
  };

  const formatValue = axis?.number_format ?? '';
  const formatOptions = NUMBER_FORMATS.some((f) => f.value === formatValue)
    ? NUMBER_FORMATS
    : [...NUMBER_FORMATS, { value: formatValue, label: `Custom (${formatValue})` }];

  return (
    <div className="kiln-chart-pane">
      <Section title="Chart Title">
        <input
          className="kiln-field"
          aria-label="Chart title"
          placeholder="No title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => {
            if (title !== (state.title ?? '')) run({ ChartSetTitle: { text: title } });
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run({ ChartSetTitle: { text: title } });
          }}
        />
      </Section>

      {series ? (
        <Section title="Series">
          <Select
            aria-label="Series"
            value={String(series.idx)}
            onValueChange={(v) => setSeriesIdx(Number(v))}
            options={state.series.map((s) => ({ value: String(s.idx), label: s.name, textValue: s.name }))}
          />
          <SwatchRow
            label="Fill"
            value={series.fill}
            current={series.color}
            onPick={(hex) => formatSeries({ fill: hex })}
          />
          <SwatchRow
            label="Line"
            value={series.line}
            current={series.line ?? (series.line_width > 0 ? series.color : null)}
            onPick={(hex) => formatSeries({ line: hex })}
          />
          <div className="kiln-chart-pane__row kiln-chart-pane__row--2">
            <label htmlFor={`${ids}-width`}>Line width</label>
            <NumberInput
              id={`${ids}-width`}
              aria-label="Line width"
              unit="pt"
              min={0}
              max={20}
              step={0.25}
              value={series.line_width}
              size="sm"
              width={104}
              onValueChange={(v) => formatSeries({ line_width: v })}
            />
          </div>
          <label className="kiln-chart-pane__check">
            <input
              type="checkbox"
              checked={series.secondary}
              onChange={(e) => formatSeries({ secondary: e.target.checked })}
            />
            Plot on secondary axis
          </label>
        </Section>
      ) : null}

      {axis ? (
        <Section title="Axis Options">
          <Select
            aria-label="Axis"
            value={axis.target}
            onValueChange={(v) => setTarget(v as ChartAxisTarget)}
            options={state.axes.map((a) => ({ value: a.target, label: a.label, textValue: a.label }))}
          />
          {axis.is_value && axis.min !== null && axis.max !== null && axis.major_unit !== null ? (
            <>
              <AxisBoundRow
                label="Minimum"
                id={`${ids}-min`}
                value={axis.min}
                auto={axis.min_auto}
                onSet={(v) => formatAxis({ min: bound.fixed(v) })}
                onAuto={() => formatAxis({ min: bound.auto })}
              />
              <AxisBoundRow
                label="Maximum"
                id={`${ids}-max`}
                value={axis.max}
                auto={axis.max_auto}
                onSet={(v) => formatAxis({ max: bound.fixed(v) })}
                onAuto={() => formatAxis({ max: bound.auto })}
              />
              <AxisBoundRow
                label="Major unit"
                id={`${ids}-unit`}
                value={axis.major_unit}
                auto={axis.major_unit_auto}
                min={0.000001}
                onSet={(v) => formatAxis({ major_unit: bound.fixed(v) })}
                onAuto={() => formatAxis({ major_unit: bound.auto })}
              />
              <label className="kiln-chart-pane__check">
                <input
                  type="checkbox"
                  checked={axis.log_base !== null}
                  onChange={(e) => formatAxis({ log_base: e.target.checked ? bound.fixed(10) : bound.auto })}
                />
                Logarithmic scale (base 10)
              </label>
            </>
          ) : null}
          <label className="kiln-chart-pane__check">
            <input
              type="checkbox"
              checked={axis.reversed}
              onChange={(e) => formatAxis({ reversed: e.target.checked })}
            />
            {axis.is_value ? 'Values in reverse order' : 'Categories in reverse order'}
          </label>
          <div className="kiln-chart-pane__row kiln-chart-pane__row--2">
            <span id={`${ids}-fmt`}>Number</span>
            <Select
              labelId={`${ids}-fmt`}
              value={formatValue === '' ? 'source' : formatValue}
              onValueChange={(v) => formatAxis({ number_format: v === 'source' ? '' : v })}
              options={formatOptions.map((f) => ({
                value: f.value === '' ? 'source' : f.value,
                label: f.label,
              }))}
            />
          </div>
        </Section>
      ) : null}
    </div>
  );
}
