import {
  ArrowLeftRight,
  BarChart3,
  ChevronDown,
  Database,
  MoveRight,
  PaintBucket,
  PanelRight,
  PenLine,
} from 'lucide-react';
import { useId, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogClose, DialogContent } from '../primitives/Dialog.tsx';
import {
  Menu,
  MenuContent,
  MenuRadioGroup,
  MenuRadioItem,
  MenuTrigger,
  Submenu,
} from '../primitives/Menu.tsx';
import { group, tab } from '../ribbon/builders.ts';
import type { RibbonCustomItem, RibbonTab } from '../ribbon/types.ts';
import { ChangeChartTypeDialog, SelectDataDialog } from './ChartDialogs.tsx';
import { ChangeColorsMenu, ChartElementsMenu, ChartStylesMenu, QuickLayoutMenu } from './ChartMenus.tsx';
import { SERIES_SWATCHES } from './FormatChartPane.tsx';
import type { ChartUiHost } from './types.ts';

function ChangeTypeButton({ host, layout }: { host: ChartUiHost; layout: ButtonLayout }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        variant="toolbar"
        layout={layout}
        icon={<Icon icon={BarChart3} size={20} />}
        onClick={() => setOpen(true)}
      >
        Change Chart Type
      </Button>
      <ChangeChartTypeDialog host={host} open={open} onOpenChange={setOpen} />
    </>
  );
}

function SelectDataButton({ host, layout }: { host: ChartUiHost; layout: ButtonLayout }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        variant="toolbar"
        layout={layout}
        icon={<Icon icon={Database} size={20} />}
        disabled={!host.state.has_cells}
        onClick={() => setOpen(true)}
      >
        Select Data
      </Button>
      <SelectDataDialog host={host} open={open} onOpenChange={setOpen} />
    </>
  );
}

/** Move Chart: to a new chart sheet or as an object on an existing sheet (Grid). */
export function MoveChartDialog({
  host,
  sheets,
  open,
  onOpenChange,
}: {
  host: ChartUiHost;
  sheets: readonly string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const id = useId();
  const [where, setWhere] = useState<'new' | 'object'>('new');
  const [name, setName] = useState('Chart1');
  const [sheet, setSheet] = useState(sheets[0] ?? '');
  const valid =
    where === 'new' ? name.trim().length > 0 && name.length <= 31 && !/[\\/?*[\]:]/.test(name) : sheet !== '';
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="sm"
        title="Move Chart"
        footer={
          <>
            <Button
              variant="primary"
              disabled={!valid}
              onClick={() => {
                host.run({
                  ChartMove: {
                    to: where === 'new' ? { NewSheet: { name: name.trim() } } : { ObjectIn: { sheet } },
                  },
                });
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
        <fieldset className="kiln-chart-move">
          <legend className="kiln-chart-data__heading">Choose where you want the chart to be placed</legend>
          <label className="kiln-chart-pane__check">
            <input
              type="radio"
              name={`${id}-where`}
              checked={where === 'new'}
              onChange={() => setWhere('new')}
            />
            New sheet:
            <input
              className="kiln-field"
              aria-label="New sheet name"
              value={name}
              disabled={where !== 'new'}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className="kiln-chart-pane__check">
            <input
              type="radio"
              name={`${id}-where`}
              checked={where === 'object'}
              onChange={() => setWhere('object')}
            />
            Object in:
            <select
              className="kiln-field"
              aria-label="Sheet"
              value={sheet}
              disabled={where !== 'object'}
              onChange={(e) => setSheet(e.target.value)}
            >
              {sheets.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          </label>
        </fieldset>
      </DialogContent>
    </Dialog>
  );
}

function MoveChartButton({
  host,
  sheets,
  layout,
}: {
  host: ChartUiHost;
  sheets: readonly string[];
  layout: ButtonLayout;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        variant="toolbar"
        layout={layout}
        icon={<Icon icon={MoveRight} size={20} />}
        onClick={() => setOpen(true)}
      >
        Move Chart
      </Button>
      <MoveChartDialog host={host} sheets={sheets} open={open} onOpenChange={setOpen} />
    </>
  );
}

type ButtonLayout = 'stacked' | 'inline';

/**
 * A chart control as a large ribbon item: a full-height stacked button in the Classic ribbon (one
 * per column, like Office's Chart Design tab), an inline button in the Simplified ribbon.
 */
const custom = (
  id: string,
  label: string,
  render: (layout: ButtonLayout) => ReturnType<RibbonCustomItem['render']>,
): RibbonCustomItem => ({
  kind: 'custom',
  id,
  label,
  size: 'large',
  render: (ctx) => render(ctx.layout === 'classic' ? 'stacked' : 'inline'),
});

/** Format ▸ Shape Fill / Shape Outline: a submenu per series with the current colour checked. */
function SeriesColourMenu({
  host,
  part,
  layout,
}: {
  host: ChartUiHost;
  part: 'fill' | 'line';
  layout: ButtonLayout;
}) {
  const title = part === 'fill' ? 'Shape Fill' : 'Shape Outline';
  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="toolbar"
          layout={layout}
          icon={<Icon icon={part === 'fill' ? PaintBucket : PenLine} size={20} />}
          iconAfter={<Icon icon={ChevronDown} size={12} />}
          disabled={host.state.series.length === 0}
        >
          {title}
        </Button>
      </MenuTrigger>
      <MenuContent aria-label={part === 'fill' ? 'Series fill' : 'Series outline'}>
        {host.state.series.map((s) => (
          <Submenu
            key={s.idx}
            label={
              <>
                <span className="kiln-chart-data__swatch" style={{ background: `#${s.color}` }} />
                {s.name}
              </>
            }
          >
            <MenuRadioGroup
              value={(part === 'fill' ? s.fill : s.line)?.toUpperCase() ?? ''}
              onValueChange={(hex) =>
                host.run({
                  ChartFormatSeries: {
                    series: s.idx,
                    fill: part === 'fill' ? hex : null,
                    line: part === 'line' ? hex : null,
                    line_width: null,
                    secondary: null,
                  },
                })
              }
            >
              <MenuRadioItem value="">Automatic</MenuRadioItem>
              {SERIES_SWATCHES.map((c) => (
                <MenuRadioItem key={c.hex} value={c.hex}>
                  <span className="kiln-chart-data__swatch" style={{ background: `#${c.hex}` }} />
                  {c.label}
                </MenuRadioItem>
              ))}
            </MenuRadioGroup>
          </Submenu>
        ))}
      </MenuContent>
    </Menu>
  );
}

export interface ChartTabsOptions {
  /** Sheet names for Move Chart (Grid); Move Chart is left out without them. */
  sheets?: readonly string[];
  /** Opens the Format Chart pane (Chart Format ▸ Format Selection). */
  onFormatSelection?: () => void;
  /** Whether a chart is selected (the contextual tabs show only then). */
  isChartSelected?: () => boolean;
}

/**
 * The Chart Design and Chart Format contextual tabs (Office's "Chart Tools"), for Grid, Write and
 * Deck ribbons. Every control runs a `SuiteCommand::Chart*` through `host.run`.
 */
export function chartTabs(host: ChartUiHost, options: ChartTabsOptions = {}): RibbonTab[] {
  const when = options.isChartSelected ? () => options.isChartSelected?.() === true : undefined;
  const contextual = { set: 'chart-tools', label: 'Chart Tools' };
  const design = tab(
    'chart-design',
    'Chart Design',
    [
      group('chart-layouts', 'Chart Layouts', [
        custom('chart.addElement', 'Add Chart Element', (l) => <ChartElementsMenu host={host} layout={l} />),
        custom('chart.quickLayout', 'Quick Layout', (l) => <QuickLayoutMenu host={host} layout={l} />),
      ]),
      group('chart-styles', 'Chart Styles', [
        custom('chart.colors', 'Change Colors', (l) => <ChangeColorsMenu host={host} layout={l} />),
        custom('chart.styles', 'Chart Styles', (l) => <ChartStylesMenu host={host} layout={l} />),
      ]),
      group('chart-data', 'Data', [
        custom('chart.switchRowColumn', 'Switch Row/Column', (l) => (
          <Button
            variant="toolbar"
            layout={l}
            icon={<Icon icon={ArrowLeftRight} size={20} />}
            disabled={!host.state.has_cells}
            onClick={() => host.run('ChartSwitchRowColumn')}
          >
            Switch Row/Column
          </Button>
        )),
        custom('chart.selectData', 'Select Data', (l) => <SelectDataButton host={host} layout={l} />),
      ]),
      group('chart-type', 'Type', [
        custom('chart.changeType', 'Change Chart Type', (l) => <ChangeTypeButton host={host} layout={l} />),
      ]),
      ...(options.sheets
        ? [
            group('chart-location', 'Location', [
              custom('chart.move', 'Move Chart', (l) => (
                <MoveChartButton host={host} sheets={options.sheets ?? []} layout={l} />
              )),
            ]),
          ]
        : []),
    ],
    { keyTip: 'JC', contextual, ...(when ? { when } : {}) },
  );
  const format = tab(
    'chart-format',
    'Format',
    [
      group('chart-selection', 'Current Selection', [
        custom('chart.formatSelection', 'Format Selection', (l) => (
          <Button
            variant="toolbar"
            layout={l}
            icon={<Icon icon={PanelRight} size={20} />}
            onClick={() => options.onFormatSelection?.()}
          >
            Format Selection
          </Button>
        )),
      ]),
      group('chart-shape-styles', 'Shape Styles', [
        custom('chart.seriesFill', 'Shape Fill', (l) => (
          <SeriesColourMenu host={host} part="fill" layout={l} />
        )),
        custom('chart.seriesOutline', 'Shape Outline', (l) => (
          <SeriesColourMenu host={host} part="line" layout={l} />
        )),
      ]),
    ],
    { keyTip: 'JO', contextual, ...(when ? { when } : {}) },
  );
  return [design, format];
}
