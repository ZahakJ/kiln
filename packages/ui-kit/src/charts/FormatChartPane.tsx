import { RotateCcw } from 'lucide-react';
import { type ReactNode, useEffect, useId, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { Select } from '../primitives/Select.tsx';
import { bound, type ChartAxisTarget, type ChartCommand, type ChartUiHost } from './types.ts';

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
  onPick,
}: {
  label: string;
  value?: string;
  onPick: (hex: string) => void;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a radio-like row of colour buttons.
    <div role="group" aria-label={label} className="kiln-chart-pane__swatches">
      <button
        type="button"
        className="kiln-chart-pane__swatch kiln-chart-pane__swatch--auto"
        aria-label={`${label}: Automatic`}
        aria-pressed={value === ''}
        onClick={() => onPick('')}
      >
        A
      </button>
      {SERIES_SWATCHES.map((s) => (
        <button
          key={s.hex}
          type="button"
          className="kiln-chart-pane__swatch"
          style={{ background: `#${s.hex}` }}
          aria-label={`${label}: ${s.label}`}
          aria-pressed={value?.toUpperCase() === s.hex}
          onClick={() => onPick(s.hex)}
        />
      ))}
    </div>
  );
}

/**
 * The Format Chart pane (Format Selection / double-click): title, series fill and line, secondary
 * axis, and axis options with Auto/Reset for bounds and units. Every change runs one chart command.
 */
export function FormatChartPane({ host }: { host: ChartUiHost }) {
  const { state, run } = host;
  const ids = useId();
  const [title, setTitle] = useState(state.title ?? '');
  useEffect(() => setTitle(state.title ?? ''), [state.title]);
  const [seriesIdx, setSeriesIdx] = useState<number>(state.series[0]?.idx ?? 0);
  const series = state.series.find((s) => s.idx === seriesIdx) ?? state.series[0];
  const [width, setWidth] = useState(2.25);
  const [axis, setAxis] = useState<ChartAxisTarget>('PrimaryValue');
  const [min, setMin] = useState(0);
  const [max, setMax] = useState(100);
  const [unit, setUnit] = useState(10);
  const [log, setLog] = useState(false);
  const [reversed, setReversed] = useState(false);

  const formatAxis = (patch: Partial<Omit<FormatAxisArgs, 'axis'>>) =>
    run({
      ChartFormatAxis: {
        axis,
        min: null,
        max: null,
        major_unit: null,
        log_base: null,
        reversed: null,
        number_format: null,
        ...patch,
      },
    });

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
            onPick={(hex) =>
              run({
                ChartFormatSeries: {
                  series: series.idx,
                  fill: hex,
                  line: null,
                  line_width: null,
                  secondary: null,
                },
              })
            }
          />
          <SwatchRow
            label="Line"
            onPick={(hex) =>
              run({
                ChartFormatSeries: {
                  series: series.idx,
                  fill: null,
                  line: hex,
                  line_width: null,
                  secondary: null,
                },
              })
            }
          />
          <div className="kiln-chart-pane__row">
            <label htmlFor={`${ids}-width`}>Line width</label>
            <NumberInput
              id={`${ids}-width`}
              aria-label="Line width"
              unit="pt"
              min={0}
              max={20}
              step={0.25}
              value={width}
              size="sm"
              width={96}
              onValueChange={(v) => {
                setWidth(v);
                run({
                  ChartFormatSeries: {
                    series: series.idx,
                    fill: null,
                    line: null,
                    line_width: v,
                    secondary: null,
                  },
                });
              }}
            />
          </div>
          <label className="kiln-chart-pane__check">
            <input
              type="checkbox"
              checked={series.secondary}
              onChange={(e) =>
                run({
                  ChartFormatSeries: {
                    series: series.idx,
                    fill: null,
                    line: null,
                    line_width: null,
                    secondary: e.target.checked,
                  },
                })
              }
            />
            Plot on secondary axis
          </label>
        </Section>
      ) : null}

      <Section title="Axis Options">
        <Select
          aria-label="Axis"
          value={axis}
          onValueChange={(v) => setAxis(v as ChartAxisTarget)}
          options={[
            { value: 'PrimaryValue', label: 'Vertical (Value) Axis' },
            { value: 'PrimaryCategory', label: 'Horizontal (Category) Axis' },
            { value: 'SecondaryValue', label: 'Secondary Vertical Axis' },
            { value: 'SecondaryCategory', label: 'Secondary Horizontal Axis' },
          ]}
        />
        {(
          [
            ['Minimum', min, setMin, 'min'],
            ['Maximum', max, setMax, 'max'],
            ['Major unit', unit, setUnit, 'major_unit'],
          ] as const
        ).map(([label, value, set, key]) => (
          <div key={key} className="kiln-chart-pane__row">
            <label htmlFor={`${ids}-${key}`}>{label}</label>
            <NumberInput
              id={`${ids}-${key}`}
              aria-label={label}
              value={value}
              size="sm"
              width={96}
              min={key === 'major_unit' ? 0.000001 : undefined}
              onValueChange={(v) => {
                set(v);
                formatAxis({ [key]: bound.fixed(v) });
              }}
            />
            <Button
              variant="ghost"
              size="sm"
              aria-label={`Reset ${label.toLowerCase()} to automatic`}
              icon={<Icon icon={RotateCcw} size={14} />}
              onClick={() => formatAxis({ [key]: bound.auto })}
            >
              Auto
            </Button>
          </div>
        ))}
        <label className="kiln-chart-pane__check">
          <input
            type="checkbox"
            checked={log}
            onChange={(e) => {
              setLog(e.target.checked);
              formatAxis({ log_base: e.target.checked ? bound.fixed(10) : bound.auto });
            }}
          />
          Logarithmic scale (base 10)
        </label>
        <label className="kiln-chart-pane__check">
          <input
            type="checkbox"
            checked={reversed}
            onChange={(e) => {
              setReversed(e.target.checked);
              formatAxis({ reversed: e.target.checked });
            }}
          />
          Values in reverse order
        </label>
        <div className="kiln-chart-pane__row">
          <span id={`${ids}-fmt`}>Number</span>
          <Select
            labelId={`${ids}-fmt`}
            defaultValue="source"
            onValueChange={(v) => formatAxis({ number_format: v === 'source' ? '' : v })}
            options={NUMBER_FORMATS.map((f) => ({
              value: f.value === '' ? 'source' : f.value,
              label: f.label,
            }))}
          />
        </div>
      </Section>
    </div>
  );
}
