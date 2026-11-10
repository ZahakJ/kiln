import { msg, t } from '@kiln/i18n';
import {
  ArrowLeftRight,
  BarChart3,
  ChevronDown,
  Database,
  MoveRight,
  PaintBucket,
  PanelRight,
  PenLine,
  Sheet,
} from 'lucide-react';
import { useEffect, useId, useState } from 'react';
import { returnToDocument } from '../chrome/CommandContext.tsx';
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
        {t('Change Chart Type')}
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
        {t('Select Data')}
      </Button>
      <SelectDataDialog host={host} open={open} onOpenChange={setOpen} />
    </>
  );
}

/** Move Chart: to a new chart sheet or as an object on an existing sheet (Grid). */
export function MoveChartDialog({
  host,
  sheets,
  taken = sheets,
  open,
  onOpenChange,
}: {
  host: ChartUiHost;
  /** The worksheets the chart can go to as an object. */
  sheets: readonly string[];
  /** Every sheet name (a new chart sheet takes the first free `ChartN`, as in Excel). */
  taken?: readonly string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const id = useId();
  const firstFree = () => {
    const used = new Set(taken.map((t) => t.toLowerCase()));
    let n = 1;
    while (used.has(`chart${n}`)) n++;
    return `Chart${n}`;
  };
  const [where, setWhere] = useState<'new' | 'object'>('new');
  const [name, setName] = useState(firstFree);
  const [sheet, setSheet] = useState(sheets[0] ?? '');
  useEffect(() => {
    if (open) {
      setName(firstFree());
      setSheet((s) => (sheets.includes(s) ? s : (sheets[0] ?? '')));
    }
    // The defaults follow the sheets each time the dialog opens (firstFree reads `taken`).
  }, [open, sheets, taken]);
  const clash = taken.some((t) => t.toLowerCase() === name.trim().toLowerCase());
  const valid =
    where === 'new'
      ? name.trim().length > 0 && name.length <= 31 && !/[\\/?*[\]:]/.test(name) && !clash
      : sheet !== '';
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="sm"
        title={t('Move Chart')}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={() => returnToDocument()}
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
              {t('OK')}
            </Button>
            <DialogClose asChild>
              <Button>{t('Cancel')}</Button>
            </DialogClose>
          </>
        }
      >
        <fieldset className="kiln-chart-move">
          <legend className="kiln-chart-data__heading">
            {t('Choose where you want the chart to be placed')}
          </legend>
          <label className="kiln-chart-pane__check">
            <input
              type="radio"
              name={`${id}-where`}
              checked={where === 'new'}
              onChange={() => setWhere('new')}
            />
            {t('New sheet:')}
            <input
              className="kiln-field"
              aria-label={t('New sheet name')}
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
            {t('Object in:')}
            <select
              className="kiln-field"
              aria-label={t('Sheet')}
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
  taken,
  layout,
}: {
  host: ChartUiHost;
  sheets: readonly string[];
  taken: readonly string[];
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
        {t('Move Chart')}
      </Button>
      <MoveChartDialog host={host} sheets={sheets} taken={taken} open={open} onOpenChange={setOpen} />
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
  const title = part === 'fill' ? t('Shape Fill') : t('Shape Outline');
  return (
    <Menu>
      <MenuTrigger asChild>
        <Button
          variant="toolbar"
          layout={layout}
          icon={<Icon icon={part === 'fill' ? PaintBucket : PenLine} size={20} />}
          iconAfter={layout === 'stacked' ? undefined : <Icon icon={ChevronDown} size={12} />}
          disabled={host.state.series.length === 0}
        >
          {layout === 'stacked' ? (
            // Stacked, the arrow follows the label's last line (as Office's "Shape Outline ▾"),
            // so a two-line label keeps it inside the button, clear of the group's name.
            <span className="kiln-chart-ribbon__label">
              {title}
              <Icon icon={ChevronDown} size={12} />
            </span>
          ) : (
            title
          )}
        </Button>
      </MenuTrigger>
      <MenuContent aria-label={part === 'fill' ? t('Series fill') : t('Series outline')}>
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
              <MenuRadioItem value="">{t('Automatic')}</MenuRadioItem>
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
  /** Worksheet names for Move Chart ▸ Object in (Grid); Move Chart is left out without them. */
  sheets?: readonly string[];
  /** Every sheet name, so a new chart sheet's name is free (defaults to `sheets`). */
  allSheets?: readonly string[];
  /** Opens the Format Chart pane (Chart Format ▸ Format Selection). */
  onFormatSelection?: () => void;
  /** Whether a chart is selected (the contextual tabs show only then). */
  isChartSelected?: () => boolean;
  /** Opens the chart's data sheet (Chart Design ▸ Data ▸ Edit Data, Write and Deck). */
  onEditData?: () => void;
}

/**
 * The Chart Design and Chart Format contextual tabs (Office's "Chart Tools"), for Grid, Write and
 * Deck ribbons. Every control runs a `SuiteCommand::Chart*` through `host.run`.
 */
export function chartTabs(given: ChartUiHost, options: ChartTabsOptions = {}): RibbonTab[] {
  // Office puts the keys back in the document after a chart command from the ribbon.
  const host: ChartUiHost = {
    state: given.state,
    run: (command) => {
      given.run(command);
      returnToDocument();
    },
  };
  const when = options.isChartSelected ? () => options.isChartSelected?.() === true : undefined;
  const contextual = { set: 'chart-tools', label: msg('Chart Tools') };
  const design = tab(
    'chart-design',
    msg('Chart Design'),
    [
      group('chart-layouts', msg('Chart Layouts'), [
        custom('chart.addElement', msg('Add Chart Element'), (l) => (
          <ChartElementsMenu host={host} layout={l} />
        )),
        custom('chart.quickLayout', msg('Quick Layout'), (l) => <QuickLayoutMenu host={host} layout={l} />),
      ]),
      group('chart-styles', msg('Chart Styles'), [
        custom('chart.colors', msg('Change Colors'), (l) => <ChangeColorsMenu host={host} layout={l} />),
        custom('chart.styles', msg('Chart Styles'), (l) => <ChartStylesMenu host={host} layout={l} />),
      ]),
      group('chart-data', msg('Data'), [
        custom('chart.switchRowColumn', msg('Switch Row/Column'), (l) => (
          <Button
            variant="toolbar"
            layout={l}
            icon={<Icon icon={ArrowLeftRight} size={20} />}
            disabled={!host.state.has_cells}
            onClick={() => host.run('ChartSwitchRowColumn')}
          >
            {t('Switch Row/Column')}
          </Button>
        )),
        custom('chart.selectData', msg('Select Data'), (l) => <SelectDataButton host={host} layout={l} />),
        ...(options.onEditData
          ? [
              custom('chart.editData', msg('Edit Data'), (l) => (
                <Button
                  variant="toolbar"
                  layout={l}
                  icon={<Icon icon={Sheet} size={20} />}
                  data-testid="chart-edit-data"
                  onClick={() => options.onEditData?.()}
                >
                  {t('Edit Data')}
                </Button>
              )),
            ]
          : []),
      ]),
      group('chart-type', msg('Type'), [
        custom('chart.changeType', msg('Change Chart Type'), (l) => (
          <ChangeTypeButton host={host} layout={l} />
        )),
      ]),
      ...(options.sheets
        ? [
            group('chart-location', msg('Location'), [
              custom('chart.move', msg('Move Chart'), (l) => (
                <MoveChartButton
                  host={host}
                  sheets={options.sheets ?? []}
                  taken={options.allSheets ?? options.sheets ?? []}
                  layout={l}
                />
              )),
            ]),
          ]
        : []),
    ],
    { keyTip: 'JC', contextual, ...(when ? { when } : {}) },
  );
  const format = tab(
    'chart-format',
    msg('Format'),
    [
      group('chart-selection', msg('Current Selection'), [
        custom('chart.formatSelection', msg('Format Selection'), (l) => (
          <Button
            variant="toolbar"
            layout={l}
            icon={<Icon icon={PanelRight} size={20} />}
            onClick={() => options.onFormatSelection?.()}
          >
            {t('Format Selection')}
          </Button>
        )),
      ]),
      group('chart-shape-styles', msg('Shape Styles'), [
        custom('chart.seriesFill', msg('Shape Fill'), (l) => (
          <SeriesColourMenu host={host} part="fill" layout={l} />
        )),
        custom('chart.seriesOutline', msg('Shape Outline'), (l) => (
          <SeriesColourMenu host={host} part="line" layout={l} />
        )),
      ]),
    ],
    { keyTip: 'JO', contextual, ...(when ? { when } : {}) },
  );
  return [design, format];
}
