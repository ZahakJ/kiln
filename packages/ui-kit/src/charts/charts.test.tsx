import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { createCommandRegistry } from '@kiln/commands';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { Ribbon } from '../ribbon/Ribbon.tsx';
import { validateRibbonSpec } from '../ribbon/validate.ts';
import { ChangeChartTypeDialog, RANGE_PATTERN, SelectDataDialog } from './ChartDialogs.tsx';
import { ChangeColorsMenu, ChartElementsMenu } from './ChartMenus.tsx';
import { ChartTypeGallery } from './ChartTypeGallery.tsx';
import { chartTabs } from './chartRibbon.tsx';
import { FormatChartPane } from './FormatChartPane.tsx';
import { ALL_CHART_KINDS, CHART_FAMILIES, CHART_KIND_LABELS } from './kinds.ts';
import { type ChartCommand, type ChartUiHost, type ChartUiState, EMPTY_CHART_STATE } from './types.ts';

// Vitest runs in the package directory.
// Radix menus, dialogs and 28 tooltips under jsdom are slow on a loaded machine.
vi.setConfig({ testTimeout: 30_000 });

const generated = (name: string) =>
  readFileSync(join(process.cwd(), '..', 'protocol', 'src', 'generated', name), 'utf8');

function host(state: Partial<ChartUiState> = {}) {
  const run = vi.fn<(c: ChartCommand) => void>();
  const h: ChartUiHost = {
    state: {
      ...EMPTY_CHART_STATE,
      kind: 'ColumnClustered',
      title: 'Spend by quarter',
      data_range: "'Budget 2026'!$A$1:$D$5",
      series: [
        {
          idx: 0,
          name: 'Materials',
          color: '1D8A60',
          fill: null,
          line: null,
          line_width: 0,
          secondary: false,
        },
        {
          idx: 1,
          name: 'Firing',
          color: 'E05A27',
          fill: 'E05A27',
          line: null,
          line_width: 0,
          secondary: false,
        },
      ],
      categories: ['Q1', 'Q2', 'Q3', 'Q4'],
      has_cells: true,
      axes: [
        {
          target: 'PrimaryValue',
          label: 'Vertical (Value) Axis',
          is_value: true,
          min: 0,
          max: 1400,
          major_unit: 200,
          min_auto: true,
          max_auto: false,
          major_unit_auto: true,
          log_base: null,
          reversed: false,
          number_format: null,
        },
        {
          target: 'PrimaryCategory',
          label: 'Horizontal (Category) Axis',
          is_value: false,
          min: null,
          max: null,
          major_unit: null,
          min_auto: true,
          max_auto: true,
          major_unit_auto: true,
          log_base: null,
          reversed: false,
          number_format: null,
        },
      ],
      ...state,
    },
    run,
  };
  return { h, run };
}

describe('chart kinds', () => {
  it('cover every protocol ChartKind exactly once, in gallery order', () => {
    const declared = [...generated('ChartKind.ts').matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1]);
    expect(new Set(ALL_CHART_KINDS)).toEqual(new Set(declared));
    expect(ALL_CHART_KINDS.length).toBe(declared.length);
    for (const k of ALL_CHART_KINDS) expect(CHART_KIND_LABELS[k].length).toBeGreaterThan(2);
  });

  it('send commands the protocol knows', () => {
    const suite = generated('SuiteCommand.ts');
    for (const v of [
      'ChartInsert',
      'ChartChangeType',
      'ChartSwitchRowColumn',
      'ChartSelectData',
      'ChartSetElement',
      'ChartSetLegend',
      'ChartSetDataLabels',
      'ChartQuickLayout',
      'ChartSetStyle',
      'ChartSetColors',
      'ChartSetTitle',
      'ChartMove',
      'ChartFormatSeries',
      'ChartFormatAxis',
    ]) {
      expect(suite).toContain(`"${v}"`);
    }
  });
});

describe('ChartTypeGallery', () => {
  it('moves with the arrow keys across families and selects with Enter', async () => {
    const onSelect = vi.fn();
    const onPreview = vi.fn();
    render(<ChartTypeGallery value="ColumnClustered" onSelect={onSelect} onPreview={onPreview} />);
    const box = screen.getByRole('listbox', { name: 'Chart types' });
    const options = within(box).getAllByRole('option');
    expect(options).toHaveLength(ALL_CHART_KINDS.length);
    const first = within(box).getByRole('option', { name: 'Clustered Column' });
    expect(first.getAttribute('aria-selected')).toBe('true');
    first.focus();
    fireEvent.keyDown(first, { key: 'ArrowRight' });
    expect(document.activeElement?.getAttribute('data-kind')).toBe('ColumnStacked');
    fireEvent.keyDown(document.activeElement as Element, { key: 'ArrowDown' });
    expect(document.activeElement?.getAttribute('data-kind')).toBe('BarStacked');
    expect(onPreview).toHaveBeenLastCalledWith('BarStacked');
    fireEvent.keyDown(document.activeElement as Element, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('BarStacked');
    fireEvent.keyDown(document.activeElement as Element, { key: 'End' });
    expect(document.activeElement?.getAttribute('data-kind')).toBe(
      ALL_CHART_KINDS[ALL_CHART_KINDS.length - 1],
    );
    await userEvent.click(within(box).getByRole('option', { name: 'Treemap' }));
    expect(onSelect).toHaveBeenLastCalledWith('Treemap');
  });

  it('shows one family when asked', () => {
    const line = CHART_FAMILIES.find((f) => f.id === 'line');
    if (!line) throw new Error('no line family');
    render(<ChartTypeGallery families={[line]} onSelect={() => {}} />);
    expect(screen.getAllByRole('option')).toHaveLength(line.kinds.length);
  });
});

describe('Change Chart Type', () => {
  it('runs ChartChangeType with the picked kind', async () => {
    const { h, run } = host();
    render(<ChangeChartTypeDialog host={h} open onOpenChange={() => {}} />);
    await userEvent.click(screen.getByRole('tab', { name: /Pie/ }));
    await userEvent.click(screen.getByRole('option', { name: 'Doughnut' }));
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(run).toHaveBeenCalledWith({ ChartChangeType: { kind: 'Doughnut' } });
  });

  it('inserts in insert mode', async () => {
    const { h, run } = host({ kind: null });
    render(<ChangeChartTypeDialog host={h} open onOpenChange={() => {}} mode="insert" />);
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(run).toHaveBeenCalledWith({ ChartInsert: { kind: 'ColumnClustered' } });
  });
});

describe('Select Data', () => {
  it('validates the range, switches rows and columns and sends the new range', async () => {
    const { h, run } = host();
    render(<SelectDataDialog host={h} open onOpenChange={() => {}} />);
    expect(screen.getByText('Materials')).toBeTruthy();
    expect(screen.getByText('Q3')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: /Switch Row\/Column/ }));
    expect(run).toHaveBeenCalledWith('ChartSwitchRowColumn');
    const input = screen.getByLabelText('Chart data range');
    await userEvent.clear(input);
    await userEvent.type(input, 'nonsense');
    expect((screen.getByRole('button', { name: 'OK' }) as HTMLButtonElement).disabled).toBe(true);
    await userEvent.clear(input);
    await userEvent.type(input, 'Sheet1!$A$1:$C$9');
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(run).toHaveBeenLastCalledWith({ ChartSelectData: { range: 'Sheet1!$A$1:$C$9' } });
  });

  it('accepts the references Excel writes', () => {
    for (const ok of ['Sheet1!A1', "'Q1 «sales»'!$A$1:$D$300", "'It''s'!B2:C3"])
      expect(RANGE_PATTERN.test(ok)).toBe(true);
    for (const bad of ['A1:B2', 'Sheet1!', 'Sheet1!A1:', '(Sheet1!A1,Sheet1!B2)'])
      expect(RANGE_PATTERN.test(bad)).toBe(false);
  });
});

describe('Add Chart Element', () => {
  it('toggles elements and places the legend', async () => {
    const { h, run } = host({ legend: 'Bottom', elements: ['MajorGridlines'] });
    render(<ChartElementsMenu host={h} />);
    await userEvent.click(screen.getByRole('button', { name: /Add Chart Element/ }));
    await userEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Chart Title' }));
    expect(run).toHaveBeenCalledWith({ ChartSetElement: { element: 'ChartTitle', on: false } });
    await userEvent.click(screen.getByRole('button', { name: /Add Chart Element/ }));
    await userEvent.click(screen.getByRole('menuitem', { name: /Legend/ }));
    await userEvent.click(await screen.findByRole('menuitemradio', { name: 'Top' }));
    expect(run).toHaveBeenLastCalledWith({ ChartSetLegend: { position: 'Top' } });
  });
});

describe('Format Chart pane', () => {
  it('formats a series and resets an axis bound to automatic', async () => {
    const { h, run } = host();
    render(<FormatChartPane host={h} />);
    await userEvent.click(screen.getByRole('button', { name: 'Fill: Ember' }));
    expect(run).toHaveBeenCalledWith({
      ChartFormatSeries: { series: 0, fill: 'E05A27', line: null, line_width: null, secondary: null },
    });
    await userEvent.click(screen.getByRole('checkbox', { name: 'Plot on secondary axis' }));
    expect(run).toHaveBeenLastCalledWith({
      ChartFormatSeries: { series: 0, fill: null, line: null, line_width: null, secondary: true },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Reset maximum to automatic' }));
    expect(run).toHaveBeenLastCalledWith({
      ChartFormatAxis: {
        axis: 'PrimaryValue',
        min: null,
        max: 'Auto',
        major_unit: null,
        log_base: null,
        reversed: null,
        number_format: null,
      },
    });
    await userEvent.click(screen.getByRole('checkbox', { name: /Logarithmic/ }));
    expect(run).toHaveBeenLastCalledWith(
      expect.objectContaining({ ChartFormatAxis: expect.objectContaining({ log_base: { Fixed: 10 } }) }),
    );
    const title = screen.getByLabelText('Chart title');
    await userEvent.clear(title);
    await userEvent.type(title, 'Spend{Enter}');
    expect(run).toHaveBeenCalledWith({ ChartSetTitle: { text: 'Spend' } });
  });
});

describe('Format Chart pane values', () => {
  it('shows the axis bounds the core reports, and follows the selected axis', async () => {
    const { h } = host();
    render(<FormatChartPane host={h} />);
    expect(screen.getByRole('spinbutton', { name: 'Minimum' })).toHaveValue('0');
    expect(screen.getByRole('spinbutton', { name: 'Maximum' })).toHaveValue('1400');
    expect(screen.getByRole('spinbutton', { name: 'Major unit' })).toHaveValue('200');
    // Automatic bounds say so; a fixed one offers Reset.
    expect(screen.getAllByText('Auto')).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Reset maximum to automatic' })).toBeTruthy();
    await userEvent.click(screen.getByRole('combobox', { name: 'Axis' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Horizontal (Category) Axis' }));
    expect(screen.queryByRole('spinbutton', { name: 'Minimum' })).toBeNull();
    expect(screen.getByRole('checkbox', { name: 'Categories in reverse order' })).toBeTruthy();
  });

  it('labels the colour rows and marks the current colour', async () => {
    const { h } = host();
    render(<FormatChartPane host={h} />);
    expect(screen.getByText('Fill')).toBeTruthy();
    expect(screen.getByText('Line')).toBeTruthy();
    // Materials: automatic fill.
    expect(screen.getByRole('button', { name: 'Fill: Automatic' })).toHaveAttribute('aria-pressed', 'true');
    await userEvent.click(screen.getByRole('combobox', { name: 'Series' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Firing' }));
    expect(screen.getByRole('button', { name: 'Fill: Ember' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Fill: Automatic' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('shows no axis section for a chart without axes', () => {
    const { h } = host({ axes: [] });
    render(<FormatChartPane host={h} />);
    expect(screen.queryByRole('heading', { name: 'Axis Options' })).toBeNull();
  });
});

describe('chart contextual tabs', () => {
  it('lay each Chart Design control out in its own full-height column', async () => {
    const { h } = host();
    const tabs = chartTabs(h, { sheets: ['Budget'], isChartSelected: () => true });
    for (const t of tabs)
      for (const g of t.groups)
        for (const i of g.items) expect('size' in i ? i.size : undefined).toBe('large');
    const harness = createHarness('grid');
    const { container } = renderWithCommands(
      <Ribbon
        spec={{ app: 'grid', tabs }}
        layout="classic"
        defaultTab="chart-design"
        onFileClick={() => {}}
      />,
      harness,
      'grid',
    );
    const design = container.querySelector('[data-ribbon-group="chart-layouts"]');
    expect(design?.querySelectorAll('.kiln-ribbon__col--single')).toHaveLength(2);
    expect(design?.querySelectorAll('.kiln-ribbon__col--stack')).toHaveLength(0);
  });

  it('mark the current colour set from the core state', async () => {
    const { h } = host({
      color_set: 2,
      color_sets: Array.from({ length: 8 }, (_, i) => Array.from({ length: 6 }, () => `0${i}0000`)),
    });
    render(<ChangeColorsMenu host={h} />);
    await userEvent.click(screen.getByRole('button', { name: /Change Colors/ }));
    expect(screen.getByRole('menuitemradio', { name: 'Monochromatic: accent 2' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
  });

  it('are valid ribbon tabs in the Chart Tools set', () => {
    const { h } = host();
    const tabs = chartTabs(h, { sheets: ['Budget'], isChartSelected: () => true });
    expect(tabs.map((t) => t.label)).toEqual(['Chart Design', 'Format']);
    for (const t of tabs) expect(t.contextual?.set).toBe('chart-tools');
    const issues = validateRibbonSpec({ app: 'grid', tabs }, createCommandRegistry({ app: 'grid' }));
    expect(issues).toEqual([]);
    const ids = tabs.flatMap((t) => t.groups.flatMap((g) => g.items.map((i) => ('id' in i ? i.id : ''))));
    expect(ids).toContain('chart.move');
    expect(chartTabs(h)[0]?.groups.some((g) => g.id === 'chart-location')).toBe(false);
  });
});
