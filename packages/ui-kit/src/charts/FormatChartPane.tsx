import { msg, t, tx } from '@kiln/i18n';
import { RotateCcw } from 'lucide-react';
import { type ReactNode, useEffect, useId, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { Select } from '../primitives/Select.tsx';
import {
  bound,
  type ChartAreaState,
  type ChartAreaTarget,
  type ChartAxisInfo,
  type ChartAxisTarget,
  type ChartCommand,
  type ChartUiHost,
} from './types.ts';

type FormatAxisArgs = Extract<ChartCommand, { ChartFormatAxis: unknown }>['ChartFormatAxis'];

/** Swatches offered for series fill and line: the Kiln palette, then neutrals. */
export const SERIES_SWATCHES: readonly { hex: string; label: string }[] = [
  { hex: '1D8A60', label: msg('Jade') },
  { hex: 'E05A27', label: msg('Ember') },
  { hex: '3159C6', label: msg('Cobalt') },
  { hex: 'E3A126', label: msg('Saffron') },
  { hex: '8A5A3F', label: msg('Clay') },
  { hex: '8D4E85', label: msg('Plum') },
  { hex: '2B1C14', label: msg('Soot') },
  { hex: '8F7A6A', label: msg('Ash') },
];

const NUMBER_FORMATS: readonly { value: string; label: string }[] = [
  { value: '', label: msg('Linked to source') },
  { value: 'General', label: msg('General') },
  { value: '0', label: msg('Number (0)') },
  { value: '0.00', label: msg('Number (0.00)') },
  { value: '#,##0', label: msg('Thousands (#,##0)') },
  { value: '0%', label: msg('Percentage') },
  { value: '"£"#,##0', label: msg('Currency (£)') },
  { value: '"$"#,##0', label: msg('Currency ($)') },
  { value: '#,##0,"K"', label: msg('Thousands (K)') },
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
          aria-label={t('{label}: Automatic', { label })}
          title={t('Automatic')}
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
            aria-label={`${label}: ${tx(s.label)}`}
            title={tx(s.label)}
            aria-pressed={set === s.hex}
            onClick={() => onPick(s.hex)}
          />
        ))}
        {custom ? (
          <span
            className="kiln-chart-pane__swatch kiln-chart-pane__swatch--custom"
            style={{ background: `#${set}` }}
            role="img"
            aria-label={t('{label}: #{set} (current)', { label, set })}
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
        <span className="kiln-chart-pane__auto" title={t('Office chooses this value from the data')}>
          {t('Auto')}
        </span>
      ) : (
        <Button
          variant="ghost"
          size="sm"
          aria-label={t('Reset {label} to automatic', { label: label.toLowerCase() })}
          icon={<Icon icon={RotateCcw} size={14} />}
          onClick={onAuto}
        >
          {t('Reset')}
        </Button>
      )}
    </div>
  );
}

/** Fill or border of an area: Automatic, No fill/line, or a swatch. */
function AreaColourRow({
  label,
  value,
  none,
  noneLabel,
  onPick,
}: {
  label: string;
  value: string | null;
  none: boolean;
  noneLabel: string;
  onPick: (choice: string) => void;
}) {
  const id = useId();
  const set = value?.toUpperCase() ?? null;
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
          aria-label={t('{label}: Automatic', { label })}
          title={t('Automatic')}
          aria-pressed={set === null && !none}
          onClick={() => onPick('')}
        >
          <span className="kiln-chart-pane__auto-chip" />
        </button>
        <button
          type="button"
          className="kiln-chart-pane__swatch kiln-chart-pane__swatch--none"
          aria-label={`${label}: ${noneLabel}`}
          title={noneLabel}
          aria-pressed={none}
          onClick={() => onPick('none')}
        />
        {SERIES_SWATCHES.map((s) => (
          <button
            key={s.hex}
            type="button"
            className="kiln-chart-pane__swatch"
            style={{ background: `#${s.hex}` }}
            aria-label={`${label}: ${tx(s.label)}`}
            title={tx(s.label)}
            aria-pressed={set === s.hex}
            onClick={() => onPick(s.hex)}
          />
        ))}
        {set !== null && !SERIES_SWATCHES.some((s) => s.hex === set) ? (
          <span
            className="kiln-chart-pane__swatch kiln-chart-pane__swatch--custom"
            style={{ background: `#${set}` }}
            role="img"
            aria-label={t('{label}: #{set} (current)', { label, set })}
            title={`#${set}`}
          />
        ) : null}
      </div>
    </div>
  );
}

/** Chart Area or Plot Area: Fill & Line and Effects. */
function AreaSection({
  title,
  area,
  state,
  run,
}: {
  title: string;
  area: ChartAreaTarget;
  state: ChartAreaState;
  run: ChartUiHost['run'];
}) {
  const id = useId();
  const format = (patch: {
    fill?: string;
    line?: string;
    line_width?: number;
    shadow?: boolean;
    rounded?: boolean;
  }) =>
    run({
      ChartFormatArea: {
        area,
        fill: patch.fill ?? null,
        line: patch.line ?? null,
        line_width: patch.line_width ?? null,
        shadow: patch.shadow ?? null,
        rounded: patch.rounded ?? null,
      },
    });
  return (
    <Section title={title}>
      <AreaColourRow
        label={t('{title} fill', { title })}
        value={state.fill}
        none={state.no_fill}
        noneLabel={t('No fill')}
        onPick={(fill) => format({ fill })}
      />
      <AreaColourRow
        label={t('{title} border', { title })}
        value={state.line}
        none={state.no_line}
        noneLabel={t('No line')}
        onPick={(line) => format({ line })}
      />
      <div className="kiln-chart-pane__row kiln-chart-pane__row--2">
        <label htmlFor={`${id}-width`}>{t('Border width')}</label>
        <NumberInput
          id={`${id}-width`}
          aria-label={t('{title} border width', { title })}
          unit="pt"
          min={0}
          max={20}
          step={0.25}
          value={state.line_width}
          size="sm"
          width={104}
          onValueChange={(v) => format({ line_width: v })}
        />
      </div>
      <label className="kiln-chart-pane__check">
        <input
          type="checkbox"
          checked={state.shadow}
          onChange={(e) => format({ shadow: e.target.checked })}
        />
        {t('Shadow')}
      </label>
      {area === 'ChartArea' ? (
        <label className="kiln-chart-pane__check">
          <input
            type="checkbox"
            checked={state.rounded}
            onChange={(e) => format({ rounded: e.target.checked })}
          />
          {t('Rounded corners')}
        </label>
      ) : null}
    </Section>
  );
}

/**
 * The Format Chart pane (Format Selection / double-click): title, series fill and line, secondary
 * axis, the chart and plot areas' fill, border and shadow, and axis options with the bounds the chart is drawn with (automatic ones marked Auto,
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
    : [...NUMBER_FORMATS, { value: formatValue, label: t('Custom ({formatValue})', { formatValue }) }];

  return (
    <div className="kiln-chart-pane">
      <Section title={t('Chart Title')}>
        <input
          className="kiln-field"
          aria-label={t('Chart title')}
          placeholder={t('No title')}
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
        <Section title={t('Series')}>
          <Select
            aria-label={t('Series')}
            value={String(series.idx)}
            onValueChange={(v) => setSeriesIdx(Number(v))}
            options={state.series.map((s) => ({ value: String(s.idx), label: s.name, textValue: s.name }))}
          />
          <SwatchRow
            label={t('Fill')}
            value={series.fill}
            current={series.color}
            onPick={(hex) => formatSeries({ fill: hex })}
          />
          <SwatchRow
            label={t('Line')}
            value={series.line}
            current={series.line ?? (series.line_width > 0 ? series.color : null)}
            onPick={(hex) => formatSeries({ line: hex })}
          />
          <div className="kiln-chart-pane__row kiln-chart-pane__row--2">
            <label htmlFor={`${ids}-width`}>{t('Line width')}</label>
            <NumberInput
              id={`${ids}-width`}
              aria-label={t('Line width')}
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
            {t('Plot on secondary axis')}
          </label>
        </Section>
      ) : null}

      {axis ? (
        <Section title={t('Axis Options')}>
          <Select
            aria-label={t('Axis')}
            value={axis.target}
            onValueChange={(v) => setTarget(v as ChartAxisTarget)}
            options={state.axes.map((a) => ({ value: a.target, label: a.label, textValue: a.label }))}
          />
          {axis.is_value && axis.min !== null && axis.max !== null && axis.major_unit !== null ? (
            <>
              <AxisBoundRow
                label={t('Minimum')}
                id={`${ids}-min`}
                value={axis.min}
                auto={axis.min_auto}
                onSet={(v) => formatAxis({ min: bound.fixed(v) })}
                onAuto={() => formatAxis({ min: bound.auto })}
              />
              <AxisBoundRow
                label={t('Maximum')}
                id={`${ids}-max`}
                value={axis.max}
                auto={axis.max_auto}
                onSet={(v) => formatAxis({ max: bound.fixed(v) })}
                onAuto={() => formatAxis({ max: bound.auto })}
              />
              <AxisBoundRow
                label={t('Major unit')}
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
                {t('Logarithmic scale (base 10)')}
              </label>
            </>
          ) : null}
          <label className="kiln-chart-pane__check">
            <input
              type="checkbox"
              checked={axis.reversed}
              onChange={(e) => formatAxis({ reversed: e.target.checked })}
            />
            {axis.is_value ? t('Values in reverse order') : t('Categories in reverse order')}
          </label>
          <div className="kiln-chart-pane__row kiln-chart-pane__row--2">
            <span id={`${ids}-fmt`}>{t('Number')}</span>
            <Select
              labelId={`${ids}-fmt`}
              value={formatValue === '' ? 'source' : formatValue}
              onValueChange={(v) => formatAxis({ number_format: v === 'source' ? '' : v })}
              options={formatOptions.map((f) => ({
                value: f.value === '' ? 'source' : f.value,
                label: tx(f.label),
              }))}
            />
          </div>
        </Section>
      ) : null}

      <AreaSection title={t('Chart Area')} area="ChartArea" state={state.chart_area} run={run} />
      <AreaSection title={t('Plot Area')} area="PlotArea" state={state.plot_area} run={run} />
    </div>
  );
}
