import { ChevronDown, Plus } from 'lucide-react';
import type { ReactNode } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';
import {
  Menu,
  MenuCheckboxItem,
  MenuContent,
  MenuItem,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuTrigger,
  Submenu,
} from '../primitives/Menu.tsx';
import { COLOR_SETS, QUICK_LAYOUTS } from './kinds.ts';
import type { ChartElementKind, ChartLabelPosition, ChartLegendPosition, ChartUiHost } from './types.ts';

const LEGEND: readonly [ChartLegendPosition, string][] = [
  ['Right', 'Right'],
  ['Top', 'Top'],
  ['Left', 'Left'],
  ['Bottom', 'Bottom'],
  ['TopRight', 'Top Right'],
];

const LABELS: readonly [ChartLabelPosition, string][] = [
  ['Center', 'Center'],
  ['InsideEnd', 'Inside End'],
  ['InsideBase', 'Inside Base'],
  ['OutsideEnd', 'Outside End'],
  ['BestFit', 'Best Fit'],
  ['Above', 'Above'],
  ['Below', 'Below'],
];

const TOGGLES: readonly { label: string; items: readonly [ChartElementKind, string][] }[] = [
  {
    label: 'Axes',
    items: [
      ['PrimaryHorizontalAxis', 'Primary Horizontal'],
      ['PrimaryVerticalAxis', 'Primary Vertical'],
    ],
  },
  {
    label: 'Axis Titles',
    items: [
      ['PrimaryHorizontalAxisTitle', 'Primary Horizontal'],
      ['PrimaryVerticalAxisTitle', 'Primary Vertical'],
    ],
  },
  {
    label: 'Gridlines',
    items: [
      ['MajorGridlines', 'Primary Major Horizontal'],
      ['MinorGridlines', 'Primary Minor Horizontal'],
      ['CategoryGridlines', 'Primary Major Vertical'],
    ],
  },
  {
    label: 'Trendline',
    items: [
      ['LinearTrendline', 'Linear'],
      ['ExponentialTrendline', 'Exponential'],
      ['MovingAverage', 'Moving Average'],
    ],
  },
];

/**
 * Chart Design ▸ Add Chart Element (and the + button beside a selected chart): every element with
 * its check mark, the legend and data label positions as radio submenus.
 */
export function ChartElementsMenu({ host, trigger }: { host: ChartUiHost; trigger?: ReactNode }) {
  const { state, run } = host;
  const on = (k: ChartElementKind) => state.elements[k] === true;
  return (
    <Menu>
      <MenuTrigger asChild>
        {trigger ?? (
          <Button
            variant="toolbar"
            layout="stacked"
            icon={<Icon icon={Plus} size={20} />}
            iconAfter={<Icon icon={ChevronDown} size={12} />}
          >
            Add Chart Element
          </Button>
        )}
      </MenuTrigger>
      <MenuContent className="kiln-chart-elements" aria-label="Chart elements">
        <MenuCheckboxItem
          checked={state.title !== null}
          onCheckedChange={(v) => run({ ChartSetElement: { element: 'ChartTitle', on: v === true } })}
        >
          Chart Title
        </MenuCheckboxItem>
        {TOGGLES.map((g) => (
          <Submenu key={g.label} label={g.label}>
            {g.items.map(([k, label]) => (
              <MenuCheckboxItem
                key={k}
                checked={on(k)}
                onCheckedChange={(v) => run({ ChartSetElement: { element: k, on: v === true } })}
              >
                {label}
              </MenuCheckboxItem>
            ))}
          </Submenu>
        ))}
        <Submenu label="Data Labels">
          <MenuRadioGroup
            value={state.dataLabels ?? 'None'}
            onValueChange={(v) =>
              run({ ChartSetDataLabels: { position: v === 'None' ? null : (v as ChartLabelPosition) } })
            }
          >
            <MenuRadioItem value="None">None</MenuRadioItem>
            {LABELS.map(([v, label]) => (
              <MenuRadioItem key={v} value={v}>
                {label}
              </MenuRadioItem>
            ))}
          </MenuRadioGroup>
        </Submenu>
        <Submenu label="Legend">
          <MenuRadioGroup
            value={state.legend ?? 'None'}
            onValueChange={(v) =>
              run({ ChartSetLegend: { position: v === 'None' ? null : (v as ChartLegendPosition) } })
            }
          >
            <MenuRadioItem value="None">None</MenuRadioItem>
            {LEGEND.map(([v, label]) => (
              <MenuRadioItem key={v} value={v}>
                {label}
              </MenuRadioItem>
            ))}
          </MenuRadioGroup>
        </Submenu>
      </MenuContent>
    </Menu>
  );
}

/** A miniature of a quick layout: where title, legend, labels and axis titles go. */
function LayoutThumb({ layout }: { layout: number }) {
  const title = ![4, 7, 11].includes(layout);
  const legend =
    layout === 5 || layout === 8 ? null : layout === 2 ? 'top' : [3, 4].includes(layout) ? 'bottom' : 'right';
  const axisTitles = [5, 6, 7, 8, 9].includes(layout);
  const labels = [2, 4, 10].includes(layout);
  const plotRight = legend === 'right' ? 34 : 44;
  const plotTop = (title ? 9 : 4) + (legend === 'top' ? 4 : 0);
  const plotBottom = legend === 'bottom' ? 26 : 30;
  const left = axisTitles ? 9 : 5;
  const bars = [0.5, 0.8, 0.6, 0.9];
  const w = (plotRight - left) / bars.length;
  return (
    <svg className="kiln-chart-thumb" viewBox="0 0 48 36" width={48} height={36} aria-hidden="true">
      {title ? <rect className="kiln-chart-thumb__text" x={14} y={2} width={20} height={3} rx={1} /> : null}
      {axisTitles ? (
        <rect className="kiln-chart-thumb__text" x={2} y={12} width={2} height={12} rx={1} />
      ) : null}
      {bars.map((b, i) => {
        const h = (plotBottom - plotTop) * b;
        return (
          <g key={b}>
            <rect
              className="kiln-chart-thumb__s1"
              x={left + i * w + w * 0.2}
              y={plotBottom - h}
              width={w * 0.6}
              height={h}
            />
            {labels ? (
              <rect
                className="kiln-chart-thumb__text"
                x={left + i * w + w * 0.25}
                y={plotBottom - h - 3}
                width={w * 0.5}
                height={1.6}
              />
            ) : null}
          </g>
        );
      })}
      <line
        className="kiln-chart-thumb__axis"
        x1={left}
        y1={plotBottom + 0.5}
        x2={plotRight}
        y2={plotBottom + 0.5}
      />
      {legend === 'right' ? (
        <rect className="kiln-chart-thumb__text" x={37} y={14} width={8} height={8} rx={1} />
      ) : null}
      {legend === 'top' ? (
        <rect className="kiln-chart-thumb__text" x={16} y={plotTop - 4} width={16} height={2} rx={1} />
      ) : null}
      {legend === 'bottom' ? (
        <rect className="kiln-chart-thumb__text" x={16} y={31} width={16} height={2} rx={1} />
      ) : null}
    </svg>
  );
}

/** Chart Design ▸ Quick Layout: Office's eleven layouts. */
export function QuickLayoutMenu({ host }: { host: ChartUiHost }) {
  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="toolbar"
          layout="stacked"
          icon={<LayoutThumb layout={1} />}
          iconAfter={<Icon icon={ChevronDown} size={12} />}
        >
          Quick Layout
        </Button>
      </MenuTrigger>
      <MenuContent aria-label="Quick layouts" className="kiln-chart-layouts">
        {QUICK_LAYOUTS.map((l) => (
          <MenuItem
            key={l.layout}
            aria-label={l.label}
            title={l.label}
            onSelect={() => host.run({ ChartQuickLayout: { layout: l.layout } })}
          >
            <LayoutThumb layout={l.layout} />
          </MenuItem>
        ))}
      </MenuContent>
    </Menu>
  );
}

/** Chart Design ▸ Change Colors: colourful, monochrome per accent, greys. */
export function ChangeColorsMenu({ host }: { host: ChartUiHost }) {
  const current =
    host.state.style === null
      ? 0
      : (() => {
          const col = (host.state.style - 1) % 8;
          return col === 0 ? 7 : col === 1 ? 0 : col - 1;
        })();
  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="toolbar"
          layout="stacked"
          icon={<Swatches palette={0} />}
          iconAfter={<Icon icon={ChevronDown} size={12} />}
        >
          Change Colors
        </Button>
      </MenuTrigger>
      <MenuContent aria-label="Colour sets" className="kiln-chart-colors">
        <MenuRadioGroup
          value={String(current)}
          onValueChange={(v) => host.run({ ChartSetColors: { palette: Number(v) } })}
        >
          {COLOR_SETS.map((c) => (
            <MenuRadioItem key={c.palette} value={String(c.palette)} aria-label={c.label}>
              <Swatches palette={c.palette} />
              <span className="kiln-chart-colors__label">{c.label}</span>
            </MenuRadioItem>
          ))}
        </MenuRadioGroup>
      </MenuContent>
    </Menu>
  );
}

function Swatches({ palette }: { palette: number }) {
  return (
    <span className={cx('kiln-chart-swatches', `kiln-chart-swatches--p${palette}`)} aria-hidden="true">
      {[1, 2, 3, 4, 5, 6].map((i) => (
        <span key={i} className={`kiln-chart-swatches__s${i}`} />
      ))}
    </span>
  );
}

/** Chart Design ▸ Chart Styles: the eight colour styles of a row (Office numbering 1–8). */
export function ChartStylesMenu({ host }: { host: ChartUiHost }) {
  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="toolbar"
          layout="stacked"
          icon={<Swatches palette={0} />}
          iconAfter={<Icon icon={ChevronDown} size={12} />}
        >
          Chart Styles
        </Button>
      </MenuTrigger>
      <MenuContent aria-label="Chart styles" className="kiln-chart-colors">
        <MenuRadioGroup
          value={String(host.state.style ?? 2)}
          onValueChange={(v) => host.run({ ChartSetStyle: { style: Number(v) } })}
        >
          {[1, 2, 3, 4, 5, 6, 7, 8].map((style) => (
            <MenuRadioItem key={style} value={String(style)} aria-label={`Style ${style}`}>
              <Swatches palette={style === 1 ? 7 : style === 2 ? 0 : style - 2} />
              <span className="kiln-chart-colors__label">Style {style}</span>
            </MenuRadioItem>
          ))}
        </MenuRadioGroup>
        <MenuSeparator />
        <MenuItem onSelect={() => host.run({ ChartSetStyle: { style: 2 } })}>Reset to Match Style</MenuItem>
      </MenuContent>
    </Menu>
  );
}
