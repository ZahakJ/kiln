/**
 * Conditional formatting dialogs: the quick rules of Highlight Cells Rules and Top/Bottom Rules
 * (a value and a preset format, as Excel's small dialogs), New Formatting Rule and Edit Rule
 * (every rule type with its format), and the Conditional Formatting Rules Manager (current
 * selection or this sheet; order, Stop If True, Applies To, new, edit, duplicate and delete).
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type {
  GridCfRule,
  GridCfRuleKind,
  GridCfStyle,
  GridCfValue,
  GridCfValueKind,
  GridCompare,
  GridDatePeriod,
  GridTextRule,
} from '@kiln/protocol';
import { Button, Select } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import { ColorButton, hexOfChoice } from '../../dialogs/ColorButton.tsx';
import type { GridFeatures } from '../model.ts';
import { Check, Choice, FeatureFrame, TextField } from '../ui.tsx';

/** Excel's preset highlight formats. */
export const PRESET_STYLES: readonly [string, GridCfStyle][] = [
  ['Light Red Fill with Dark Red Text', style({ fill: 'FFC7CE', font_color: '9C0006' })],
  ['Yellow Fill with Dark Yellow Text', style({ fill: 'FFEB9C', font_color: '9C5700' })],
  ['Green Fill with Dark Green Text', style({ fill: 'C6EFCE', font_color: '006100' })],
  ['Light Red Fill', style({ fill: 'FFC7CE' })],
  ['Red Text', style({ font_color: '9C0006' })],
  ['Red Border', style({ border: '9C0006' })],
];

function style(p: Partial<GridCfStyle>): GridCfStyle {
  return {
    fill: null,
    font_color: null,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    border: null,
    number_format: null,
    ...p,
  };
}

const COMPARE: readonly (readonly [GridCompare, string])[] = [
  ['Between', 'between'],
  ['NotBetween', msg('not between')],
  ['Equal', msg('equal to')],
  ['NotEqual', msg('not equal to')],
  ['GreaterThan', msg('greater than')],
  ['LessThan', msg('less than')],
  ['GreaterThanOrEqual', msg('greater than or equal to')],
  ['LessThanOrEqual', msg('less than or equal to')],
];

const PERIODS: readonly (readonly [GridDatePeriod, string])[] = [
  ['Yesterday', msg('Yesterday')],
  ['Today', msg('Today')],
  ['Tomorrow', msg('Tomorrow')],
  ['Last7Days', msg('In the last 7 days')],
  ['LastWeek', msg('Last week')],
  ['ThisWeek', msg('This week')],
  ['NextWeek', msg('Next week')],
  ['LastMonth', msg('Last month')],
  ['ThisMonth', msg('This month')],
  ['NextMonth', msg('Next month')],
];

const TEXT_RULES: readonly (readonly [GridTextRule, string])[] = [
  ['Contains', 'containing'],
  ['NotContains', msg('not containing')],
  ['BeginsWith', msg('beginning with')],
  ['EndsWith', msg('ending with')],
];

const VALUE_KINDS: readonly (readonly [GridCfValueKind, string])[] = [
  ['Min', msg('Lowest Value')],
  ['Max', msg('Highest Value')],
  ['Number', msg('Number')],
  ['Percent', msg('Percent')],
  ['Percentile', msg('Percentile')],
  ['Formula', msg('Formula')],
  ['Automatic', msg('Automatic')],
];

const ICON_SETS: readonly (readonly [string, string])[] = [
  ['3Arrows', msg('3 Arrows (Coloured)')],
  ['3ArrowsGray', msg('3 Arrows (Grey)')],
  ['4Arrows', msg('4 Arrows (Coloured)')],
  ['5Arrows', msg('5 Arrows (Coloured)')],
  ['3TrafficLights1', msg('3 Traffic Lights (Unrimmed)')],
  ['3TrafficLights2', msg('3 Traffic Lights (Rimmed)')],
  ['3Signs', msg('3 Signs')],
  ['4TrafficLights', msg('4 Traffic Lights')],
  ['4RedToBlack', msg('Red To Black')],
  ['3Symbols', msg('3 Symbols (Circled)')],
  ['3Symbols2', msg('3 Symbols (Uncircled)')],
  ['3Flags', msg('3 Flags')],
  ['3Stars', msg('3 Stars')],
  ['4Rating', msg('4 Ratings')],
  ['5Quarters', msg('5 Quarters')],
  ['5Rating', msg('5 Ratings')],
  ['5Boxes', msg('5 Boxes')],
  ['3Triangles', msg('3 Triangles')],
];

export const cfValue = (kind: GridCfValueKind, value = ''): GridCfValue => ({ kind, value, gte: true });

/** The default thresholds of an icon set with `n` icons. */
export function iconThresholds(set: string): GridCfValue[] {
  const n = Number(set[0]) || 3;
  return Array.from({ length: n - 1 }, (_, i) => cfValue('Percent', String(Math.round(((i + 1) * 100) / n))));
}

/** One line describing a rule (Manage Rules). */
export function describe(k: GridCfRuleKind): string {
  if ('CellValue' in k) {
    const op = tx(COMPARE.find(([o]) => o === k.CellValue.op)?.[1] ?? '');
    return t('Cell Value {op} {values}', { op, values: k.CellValue.values.join(t(' and ')) });
  }
  if ('Text' in k)
    return t('Cell Value {rule} "{text}"', {
      rule: tx(TEXT_RULES.find(([r]) => r === k.Text.rule)?.[1] ?? ''),
      text: k.Text.text,
    });
  if ('DateOccurring' in k)
    return t('Dates Occurring: {period}', {
      period: tx(PERIODS.find(([p]) => p === k.DateOccurring.period)?.[1] ?? ''),
    });
  if ('Duplicates' in k) return k.Duplicates.unique ? t('Unique Values') : t('Duplicate Values');
  if ('TopBottom' in k) {
    const { bottom, rank, percent } = k.TopBottom;
    if (percent) return bottom ? t('Bottom {rank}%', { rank }) : t('Top {rank}%', { rank });
    return bottom ? t('Bottom {rank}', { rank }) : t('Top {rank}', { rank });
  }
  if ('Average' in k) return k.Average.below ? t('Below Average') : t('Above Average');
  if ('Blanks' in k) return k.Blanks.not ? t('No Blanks') : t('Blanks');
  if ('Errors' in k) return k.Errors.not ? t('No Errors') : t('Errors');
  if ('Formula' in k) return t('Formula: {formula}', { formula: k.Formula.formula });
  if ('ColorScale' in k) return t('grid.cf.colourScale', { count: k.ColorScale.colors.length });
  if ('DataBar' in k) return t('Data Bar');
  return t('Icon Set: {set}', {
    set: tx(ICON_SETS.find(([s]) => s === k.IconSet.set)?.[1] ?? k.IconSet.set),
  });
}

/** The quick rules of the Highlight Cells and Top/Bottom menus. */
export type CfPreset =
  | 'greater'
  | 'less'
  | 'between'
  | 'equal'
  | 'text'
  | 'date'
  | 'duplicate'
  | 'top10'
  | 'top10pct'
  | 'bottom10'
  | 'bottom10pct'
  | 'above'
  | 'below';

const QUICK: Record<CfPreset, { title: string; prompt: string }> = {
  greater: { title: msg('Greater Than'), prompt: msg('Format cells that are GREATER THAN:') },
  less: { title: msg('Less Than'), prompt: msg('Format cells that are LESS THAN:') },
  between: { title: msg('Between'), prompt: msg('Format cells that are BETWEEN:') },
  equal: { title: msg('Equal To'), prompt: msg('Format cells that are EQUAL TO:') },
  text: { title: msg('Text That Contains'), prompt: msg('Format cells that contain the text:') },
  date: { title: msg('A Date Occurring'), prompt: msg('Format cells that contain a date occurring:') },
  duplicate: { title: msg('Duplicate Values'), prompt: msg('Format cells that contain:') },
  top10: { title: msg('Top 10 Items'), prompt: msg('Format cells that rank in the TOP:') },
  top10pct: { title: msg('Top 10%'), prompt: msg('Format cells that rank in the TOP:') },
  bottom10: { title: msg('Bottom 10 Items'), prompt: msg('Format cells that rank in the BOTTOM:') },
  bottom10pct: { title: msg('Bottom 10%'), prompt: msg('Format cells that rank in the BOTTOM:') },
  above: { title: msg('Above Average'), prompt: msg('Format cells that are ABOVE AVERAGE:') },
  below: { title: msg('Below Average'), prompt: msg('Format cells that are BELOW AVERAGE:') },
};

function StylePicker({ value, onChange }: { value: GridCfStyle; onChange: (s: GridCfStyle) => void }) {
  const preset = PRESET_STYLES.findIndex(([, s]) => JSON.stringify(s) === JSON.stringify(value));
  const [custom, setCustom] = useState(preset < 0);
  return (
    <div className="kiln-gfx__style">
      <Choice
        label={t('with')}
        value={custom ? 'custom' : String(Math.max(0, preset))}
        width={280}
        options={[
          ...PRESET_STYLES.map(([n], i) => [String(i), n] as const),
          ['custom', t('Custom Format…')] as const,
        ]}
        onChange={(v) => {
          if (v === 'custom') {
            setCustom(true);
            return;
          }
          setCustom(false);
          const s = PRESET_STYLES[Number(v)];
          if (s) onChange(s[1]);
        }}
      />
      {custom ? <StyleEditor value={value} onChange={onChange} /> : null}
      <div className="kiln-gfx__preview" style={previewCss(value)}>
        {/* i18n-ignore: the font sample */}
        AaBbCcYyZz
      </div>
    </div>
  );
}

const GREEN = '#63BE7B';
const AMBER = '#FFC000';
const RED = '#F8696B';

/** The glyphs and colours of an icon set, lowest icon first (enough to recognise it). */
function iconGlyphs(set: string, reverse: boolean): { g: string; c: string }[] {
  const n = Number(set.match(/^\d/)?.[0] ?? 3);
  const pick = <T,>(three: T[], four: T[], five: T[]) => (n >= 5 ? five : n === 4 ? four : three);
  const traffic = pick(
    [RED, AMBER, GREEN],
    ['#404040', RED, AMBER, GREEN],
    ['#404040', RED, AMBER, '#9BBB59', GREEN],
  );
  let out: { g: string; c: string }[];
  if (/Arrows/.test(set)) {
    const g = pick(['↓', '→', '↑'], ['↓', '↘', '↗', '↑'], ['↓', '↘', '→', '↗', '↑']);
    out = g.map((x, i) => ({
      g: x,
      c: /Gray/.test(set) ? '#7F7F7F' : (traffic[i + (traffic.length - g.length)] ?? GREEN),
    }));
  } else if (/Triangles/.test(set)) {
    out = [
      { g: '▼', c: RED },
      { g: '▬', c: AMBER },
      { g: '▲', c: GREEN },
    ];
  } else if (/Stars/.test(set)) {
    out = ['☆', '⯪', '★'].map((g) => ({ g, c: AMBER }));
  } else if (/Flags/.test(set)) {
    out = [RED, AMBER, GREEN].map((c) => ({ g: '⚑', c }));
  } else if (/Symbols/.test(set)) {
    out = [
      { g: '✖', c: RED },
      { g: '!', c: AMBER },
      { g: '✔', c: GREEN },
    ];
  } else if (/Rating|Boxes/.test(set)) {
    out = pick(['▂', '▄', '█'], ['▂', '▄', '▆', '█'], ['▁', '▂', '▄', '▆', '█']).map((g) => ({
      g,
      c: '#4472C4',
    }));
  } else if (/Quarters/.test(set)) {
    out = ['○', '◔', '◑', '◕', '●'].map((g) => ({ g, c: '#404040' }));
  } else {
    out = traffic.map((c) => ({ g: '●', c }));
  }
  return reverse ? [...out].reverse() : out;
}

/** What a rule looks like in the Rules Manager: its format, colour scale, data bar or icons. */
function RulePreview({ rule }: { rule: GridCfRule }) {
  const k = rule.kind;
  if ('ColorScale' in k) {
    const stops = k.ColorScale.colors.map((c) => `#${c}`).join(', ');
    return (
      <span
        className="kiln-gfx__swatch kiln-gfx__swatch--rule"
        data-testid="grid-cf-preview"
        style={{ background: `linear-gradient(90deg, ${stops})` }}
      />
    );
  }
  if ('DataBar' in k) {
    const c = `#${k.DataBar.color}`;
    return (
      <span className="kiln-gfx__swatch kiln-gfx__swatch--rule" data-testid="grid-cf-preview">
        <span
          className="kiln-gfx__swatch-bar"
          style={{
            background: k.DataBar.gradient ? `linear-gradient(90deg, ${c}, ${c}22)` : c,
            borderColor: c,
          }}
        />
      </span>
    );
  }
  if ('IconSet' in k) {
    return (
      <span className="kiln-gfx__swatch kiln-gfx__swatch--rule" data-testid="grid-cf-preview">
        {iconGlyphs(k.IconSet.set, k.IconSet.reverse).map((x, i) => (
          <span key={i} style={{ color: x.c }}>
            {x.g}
          </span>
        ))}
      </span>
    );
  }
  return (
    <span className="kiln-gfx__swatch" style={previewCss(rule.style)} data-testid="grid-cf-preview">
      {/* i18n-ignore: the font sample */}
      AaBbCc
    </span>
  );
}

export function previewCss(s: GridCfStyle | null | undefined): React.CSSProperties {
  if (!s) return {};
  return {
    background: s.fill ? `#${s.fill}` : undefined,
    color: s.font_color ? `#${s.font_color}` : undefined,
    fontWeight: s.bold ? 700 : undefined,
    fontStyle: s.italic ? 'italic' : undefined,
    textDecoration:
      [s.underline ? 'underline' : '', s.strike ? 'line-through' : ''].join(' ').trim() || undefined,
    outline: s.border ? `1px solid #${s.border}` : undefined,
  };
}

function StyleEditor({ value, onChange }: { value: GridCfStyle; onChange: (s: GridCfStyle) => void }) {
  const set = (p: Partial<GridCfStyle>) => onChange({ ...value, ...p });
  return (
    <div className="kiln-gfx__grid2">
      <ColorButton
        label={t('Fill')}
        value={value.fill}
        allowNone
        onChange={(c) => set({ fill: hexOfChoice(c) })}
      />
      <ColorButton
        label={t('Font colour')}
        value={value.font_color}
        allowAuto
        onChange={(c) => set({ font_color: hexOfChoice(c) })}
      />
      <ColorButton
        label={t('Border')}
        value={value.border}
        allowNone
        onChange={(c) => set({ border: hexOfChoice(c) })}
      />
      <TextField
        label={t('Number format')}
        value={value.number_format ?? ''}
        placeholder={t('General')}
        onChange={(v) => set({ number_format: v || null })}
      />
      <div className="kiln-gfx__row">
        <Check label={t('Bold')} checked={value.bold} onChange={(bold) => set({ bold })} />
        <Check label={t('Italic')} checked={value.italic} onChange={(italic) => set({ italic })} />
        <Check
          label={t('Underline')}
          checked={value.underline}
          onChange={(underline) => set({ underline })}
        />
        <Check label={t('Strikethrough')} checked={value.strike} onChange={(strike) => set({ strike })} />
      </div>
    </div>
  );
}

/** Highlight Cells Rules and Top/Bottom Rules: one value and a format. */
export function QuickRuleDialog({ features, preset }: { features: GridFeatures; preset: CfPreset }) {
  const q = QUICK[preset];
  const [a, setA] = useState(preset.startsWith('top') || preset.startsWith('bottom') ? '10' : '');
  const [b, setB] = useState('');
  const [period, setPeriod] = useState<GridDatePeriod>('Yesterday');
  const [unique, setUnique] = useState(false);
  const [fmt, setFmt] = useState<GridCfStyle>(PRESET_STYLES[0]?.[1] ?? style({}));
  const kind = (): GridCfRuleKind => {
    switch (preset) {
      case 'greater':
        return { CellValue: { op: 'GreaterThan', values: [a] } };
      case 'less':
        return { CellValue: { op: 'LessThan', values: [a] } };
      case 'between':
        return { CellValue: { op: 'Between', values: [a, b] } };
      case 'equal':
        return { CellValue: { op: 'Equal', values: [a] } };
      case 'text':
        return { Text: { rule: 'Contains', text: a } };
      case 'date':
        return { DateOccurring: { period } };
      case 'duplicate':
        return { Duplicates: { unique } };
      case 'top10':
      case 'top10pct':
      case 'bottom10':
      case 'bottom10pct':
        return {
          TopBottom: {
            bottom: preset.startsWith('bottom'),
            percent: preset.endsWith('pct'),
            rank: Number(a) || 0,
          },
        };
      case 'above':
        return { Average: { below: false, equal: false, std_dev: 0 } };
      case 'below':
        return { Average: { below: true, equal: false, std_dev: 0 } };
    }
  };
  const needsValue = !['date', 'duplicate', 'above', 'below'].includes(preset);
  return (
    <FeatureFrame
      features={features}
      title={tx(q.title)}
      size="md"
      testId="grid-cf-quick"
      okDisabled={needsValue && !a.trim()}
      onOk={() =>
        features.tryExec({
          AddConditionalFormat: {
            rule: { id: 0, applies_to: '', kind: kind(), style: fmt, stop_if_true: false },
          },
        })
      }
    >
      <p className="kiln-gfx__prompt">{tx(q.prompt)}</p>
      <div className="kiln-gfx__row">
        {preset === 'date' ? (
          <Choice
            label={t('Period')}
            value={period}
            options={txPairs(PERIODS)}
            onChange={setPeriod}
            width={200}
          />
        ) : preset === 'duplicate' ? (
          <Choice
            label={t('Values')}
            value={unique ? 'unique' : 'dup'}
            options={[
              ['dup', t('Duplicate')],
              ['unique', t('Unique')],
            ]}
            onChange={(v) => setUnique(v === 'unique')}
            width={140}
          />
        ) : needsValue ? (
          <>
            <TextField
              label={
                preset.endsWith('pct')
                  ? t('Percent')
                  : preset.startsWith('top') || preset.startsWith('bottom')
                    ? t('Items')
                    : t('Value')
              }
              value={a}
              onChange={setA}
              autoFocus
              testId="grid-cf-value"
            />
            {preset === 'between' ? <TextField label={t('and')} value={b} onChange={setB} /> : null}
          </>
        ) : null}
      </div>
      <StylePicker value={fmt} onChange={setFmt} />
    </FeatureFrame>
  );
}

type RuleType = 'scale' | 'contain' | 'rank' | 'average' | 'unique' | 'formula';

const RULE_TYPES: readonly (readonly [RuleType, string])[] = [
  ['scale', msg('Format all cells based on their values')],
  ['contain', msg('Format only cells that contain')],
  ['rank', msg('Format only top or bottom ranked values')],
  ['average', msg('Format only values that are above or below average')],
  ['unique', msg('Format only unique or duplicate values')],
  ['formula', msg('Use a formula to determine which cells to format')],
];

function typeOf(k: GridCfRuleKind): RuleType {
  if ('ColorScale' in k || 'DataBar' in k || 'IconSet' in k) return 'scale';
  if ('TopBottom' in k) return 'rank';
  if ('Average' in k) return 'average';
  if ('Duplicates' in k) return 'unique';
  if ('Formula' in k) return 'formula';
  return 'contain';
}

const DEFAULT_KIND: Record<RuleType, GridCfRuleKind> = {
  scale: {
    ColorScale: { points: [cfValue('Min'), cfValue('Max')], colors: ['F8696B', '63BE7B'] },
  },
  contain: { CellValue: { op: 'Between', values: ['', ''] } },
  rank: { TopBottom: { bottom: false, percent: false, rank: 10 } },
  average: { Average: { below: false, equal: false, std_dev: 0 } },
  unique: { Duplicates: { unique: false } },
  formula: { Formula: { formula: '' } },
};

function ValueRow({
  label,
  value,
  onChange,
  noMinMax,
}: {
  label: string;
  value: GridCfValue;
  onChange: (v: GridCfValue) => void;
  noMinMax?: boolean;
}) {
  const kinds = VALUE_KINDS.filter(([k]) => !(noMinMax && (k === 'Min' || k === 'Max' || k === 'Automatic')));
  return (
    <div className="kiln-gfx__row">
      <Choice
        label={t('{label} type', { label })}
        value={value.kind}
        options={txPairs(kinds)}
        width={150}
        onChange={(kind) => onChange({ ...value, kind })}
      />
      {value.kind === 'Min' || value.kind === 'Max' || value.kind === 'Automatic' ? null : (
        <TextField
          label={t('{label} value', { label })}
          value={value.value}
          onChange={(v) => onChange({ ...value, value: v })}
        />
      )}
    </div>
  );
}

/** The body of a rule of any type. */
function RuleEditor({ kind, onKind }: { kind: GridCfRuleKind; onKind: (k: GridCfRuleKind) => void }) {
  if ('ColorScale' in kind || 'DataBar' in kind || 'IconSet' in kind) {
    const style =
      'ColorScale' in kind
        ? kind.ColorScale.points.length === 3
          ? '3'
          : '2'
        : 'DataBar' in kind
          ? 'bar'
          : 'icons';
    return (
      <div className="kiln-gfx__col">
        <Choice
          label={t('Format style')}
          value={style}
          width={200}
          options={[
            ['2', t('2-Color Scale')],
            ['3', t('3-Color Scale')],
            ['bar', t('Data Bar')],
            ['icons', t('Icon Sets')],
          ]}
          onChange={(v) => {
            if (v === '2')
              onKind({
                ColorScale: { points: [cfValue('Min'), cfValue('Max')], colors: ['F8696B', '63BE7B'] },
              });
            else if (v === '3')
              onKind({
                ColorScale: {
                  points: [cfValue('Min'), cfValue('Percentile', '50'), cfValue('Max')],
                  colors: ['F8696B', 'FFEB84', '63BE7B'],
                },
              });
            else if (v === 'bar')
              onKind({
                DataBar: {
                  min: cfValue('Automatic'),
                  max: cfValue('Automatic'),
                  color: '638EC6',
                  gradient: true,
                  show_value: true,
                  negative_color: 'FF0000',
                  border_color: null,
                },
              });
            else
              onKind({
                IconSet: {
                  set: '3TrafficLights1',
                  thresholds: iconThresholds('3TrafficLights1'),
                  reverse: false,
                  show_value: true,
                },
              });
          }}
        />
        {'ColorScale' in kind
          ? kind.ColorScale.points.map((p, i) => (
              <div className="kiln-gfx__row" key={`${i}-${kind.ColorScale.points.length}`}>
                <ValueRow
                  label={
                    i === 0
                      ? t('Minimum')
                      : i === kind.ColorScale.points.length - 1
                        ? t('Maximum')
                        : t('Midpoint')
                  }
                  value={p}
                  onChange={(v) => {
                    const points = kind.ColorScale.points.map((x, j) => (j === i ? v : x));
                    onKind({ ColorScale: { ...kind.ColorScale, points } });
                  }}
                />
                <ColorButton
                  label={t('Colour')}
                  value={kind.ColorScale.colors[i] ?? null}
                  onChange={(c) => {
                    const colors = kind.ColorScale.colors.map((x, j) =>
                      j === i ? (hexOfChoice(c) ?? x) : x,
                    );
                    onKind({ ColorScale: { ...kind.ColorScale, colors } });
                  }}
                />
              </div>
            ))
          : null}
        {'DataBar' in kind ? (
          <>
            <ValueRow
              label={t('Minimum')}
              value={kind.DataBar.min}
              onChange={(min) => onKind({ DataBar: { ...kind.DataBar, min } })}
            />
            <ValueRow
              label={t('Maximum')}
              value={kind.DataBar.max}
              onChange={(max) => onKind({ DataBar: { ...kind.DataBar, max } })}
            />
            <div className="kiln-gfx__row">
              <ColorButton
                label={t('Bar colour')}
                value={kind.DataBar.color}
                onChange={(c) =>
                  onKind({ DataBar: { ...kind.DataBar, color: hexOfChoice(c) ?? kind.DataBar.color } })
                }
              />
              <ColorButton
                label={t('Negative bar')}
                value={kind.DataBar.negative_color}
                onChange={(c) => onKind({ DataBar: { ...kind.DataBar, negative_color: hexOfChoice(c) } })}
              />
              <Choice
                label={t('Fill')}
                value={kind.DataBar.gradient ? 'g' : 's'}
                width={140}
                options={[
                  ['g', t('Gradient Fill')],
                  ['s', t('Solid Fill')],
                ]}
                onChange={(v) => onKind({ DataBar: { ...kind.DataBar, gradient: v === 'g' } })}
              />
            </div>
            <Check
              label={t('Show Bar Only')}
              checked={!kind.DataBar.show_value}
              onChange={(v) => onKind({ DataBar: { ...kind.DataBar, show_value: !v } })}
            />
          </>
        ) : null}
        {'IconSet' in kind ? (
          <>
            <div className="kiln-gfx__row">
              <Choice
                label={t('Icon style')}
                value={kind.IconSet.set}
                width={240}
                options={txPairs(ICON_SETS)}
                onChange={(set) =>
                  onKind({ IconSet: { ...kind.IconSet, set, thresholds: iconThresholds(set) } })
                }
              />
              <Check
                label={t('Reverse Icon Order')}
                checked={kind.IconSet.reverse}
                onChange={(reverse) => onKind({ IconSet: { ...kind.IconSet, reverse } })}
              />
              <Check
                label={t('Show Icon Only')}
                checked={!kind.IconSet.show_value}
                onChange={(v) => onKind({ IconSet: { ...kind.IconSet, show_value: !v } })}
              />
            </div>
            {kind.IconSet.thresholds.map((threshold, i) => (
              <ValueRow
                key={`${kind.IconSet.set}-${i}`}
                label={t('Icon {value} when value >=', { value: i + 2 })}
                value={threshold}
                noMinMax
                onChange={(v) => {
                  const thresholds = kind.IconSet.thresholds.map((x, j) => (j === i ? v : x));
                  onKind({ IconSet: { ...kind.IconSet, thresholds } });
                }}
              />
            ))}
          </>
        ) : null}
      </div>
    );
  }
  if ('TopBottom' in kind) {
    const tb = kind.TopBottom;
    return (
      <div className="kiln-gfx__row">
        <Choice
          label={t('Format values that rank in the')}
          value={tb.bottom ? 'b' : 't'}
          width={120}
          options={[
            ['t', t('Top')],
            ['b', t('Bottom')],
          ]}
          onChange={(v) => onKind({ TopBottom: { ...tb, bottom: v === 'b' } })}
        />
        <TextField
          label={t('Rank')}
          value={String(tb.rank)}
          onChange={(v) => onKind({ TopBottom: { ...tb, rank: Number(v) || 0 } })}
        />
        <Check
          label={t('% of the selected range')}
          checked={tb.percent}
          onChange={(percent) => onKind({ TopBottom: { ...tb, percent } })}
        />
      </div>
    );
  }
  if ('Average' in kind) {
    const a = kind.Average;
    const v = `${a.below ? 'below' : 'above'}${a.equal ? '-eq' : ''}${a.std_dev ? `-${a.std_dev}` : ''}`;
    return (
      <Choice
        label={t('Format values that are')}
        value={v}
        width={260}
        options={[
          ['above', 'above'],
          ['below', 'below'],
          ['above-eq', t('equal or above')],
          ['below-eq', t('equal or below')],
          ['above-1', t('1 std dev above')],
          ['below-1', t('1 std dev below')],
          ['above-2', t('2 std dev above')],
          ['below-2', t('2 std dev below')],
          ['above-3', t('3 std dev above')],
          ['below-3', t('3 std dev below')],
        ]}
        onChange={(x) =>
          onKind({
            Average: {
              below: x.startsWith('below'),
              equal: x.includes('-eq'),
              std_dev: Number(x.match(/-(\d)$/)?.[1] ?? 0),
            },
          })
        }
      />
    );
  }
  if ('Duplicates' in kind) {
    return (
      <Choice
        label={t('Format all')}
        value={kind.Duplicates.unique ? 'u' : 'd'}
        width={140}
        options={[
          ['d', 'duplicate'],
          ['u', 'unique'],
        ]}
        onChange={(v) => onKind({ Duplicates: { unique: v === 'u' } })}
      />
    );
  }
  if ('Formula' in kind) {
    return (
      <TextField
        label={t('Format values where this formula is true')}
        value={kind.Formula.formula}
        placeholder={t('=$B2>100')}
        wide
        autoFocus
        testId="grid-cf-formula"
        onChange={(formula) => onKind({ Formula: { formula } })}
      />
    );
  }
  // Format only cells that contain…
  const what =
    'CellValue' in kind
      ? 'value'
      : 'Text' in kind
        ? 'text'
        : 'DateOccurring' in kind
          ? 'date'
          : 'Blanks' in kind
            ? kind.Blanks.not
              ? 'noblanks'
              : 'blanks'
            : 'Errors' in kind
              ? kind.Errors.not
                ? 'noerrors'
                : 'errors'
              : 'value';
  return (
    <div className="kiln-gfx__row">
      <Choice
        label={t('Format only cells with')}
        value={what}
        width={160}
        options={[
          ['value', t('Cell Value')],
          ['text', t('Specific Text')],
          ['date', t('Dates Occurring')],
          ['blanks', t('Blanks')],
          ['noblanks', t('No Blanks')],
          ['errors', t('Errors')],
          ['noerrors', t('No Errors')],
        ]}
        onChange={(v) => {
          if (v === 'value') onKind({ CellValue: { op: 'Between', values: ['', ''] } });
          else if (v === 'text') onKind({ Text: { rule: 'Contains', text: '' } });
          else if (v === 'date') onKind({ DateOccurring: { period: 'Yesterday' } });
          else if (v === 'blanks' || v === 'noblanks') onKind({ Blanks: { not: v === 'noblanks' } });
          else onKind({ Errors: { not: v === 'noerrors' } });
        }}
      />
      {'CellValue' in kind ? (
        <>
          <Choice
            label={t('Operator')}
            value={kind.CellValue.op}
            options={txPairs(COMPARE)}
            width={180}
            onChange={(op) => onKind({ CellValue: { op, values: kind.CellValue.values } })}
          />
          <TextField
            label={t('Value')}
            value={kind.CellValue.values[0] ?? ''}
            onChange={(v) =>
              onKind({ CellValue: { ...kind.CellValue, values: [v, kind.CellValue.values[1] ?? ''] } })
            }
          />
          {kind.CellValue.op === 'Between' || kind.CellValue.op === 'NotBetween' ? (
            <TextField
              label={t('and')}
              value={kind.CellValue.values[1] ?? ''}
              onChange={(v) =>
                onKind({ CellValue: { ...kind.CellValue, values: [kind.CellValue.values[0] ?? '', v] } })
              }
            />
          ) : null}
        </>
      ) : null}
      {'Text' in kind ? (
        <>
          <Choice
            label={t('Rule')}
            value={kind.Text.rule}
            options={txPairs(TEXT_RULES)}
            width={150}
            onChange={(rule) => onKind({ Text: { ...kind.Text, rule } })}
          />
          <TextField
            label={t('Text')}
            value={kind.Text.text}
            onChange={(text) => onKind({ Text: { ...kind.Text, text } })}
          />
        </>
      ) : null}
      {'DateOccurring' in kind ? (
        <Choice
          label={t('Period')}
          value={kind.DateOccurring.period}
          options={txPairs(PERIODS)}
          width={180}
          onChange={(period) => onKind({ DateOccurring: { period } })}
        />
      ) : null}
    </div>
  );
}

/** New Formatting Rule (`rule` null) or Edit Formatting Rule; `onDone` receives the rule. */
export function RuleDialog({
  features,
  rule,
  onDone,
}: {
  features: GridFeatures;
  rule: GridCfRule | null;
  onDone?: (r: GridCfRule) => void;
}) {
  const [kind, setKind] = useState<GridCfRuleKind>(rule?.kind ?? DEFAULT_KIND.scale);
  const [fmt, setFmt] = useState<GridCfStyle>(rule?.style ?? PRESET_STYLES[0]?.[1] ?? style({}));
  const type = typeOf(kind);
  const highlight = type !== 'scale';
  const result = (): GridCfRule => ({
    id: rule?.id ?? 0,
    applies_to: rule?.applies_to ?? '',
    kind,
    style: highlight ? fmt : null,
    stop_if_true: rule?.stop_if_true ?? false,
  });
  return (
    <FeatureFrame
      features={features}
      title={rule ? t('Edit Formatting Rule') : t('New Formatting Rule')}
      size="lg"
      testId="grid-cf-rule"
      onOk={async () => {
        if (onDone) {
          onDone(result());
          return;
        }
        return features.tryExec({ AddConditionalFormat: { rule: result() } });
      }}
    >
      <fieldset className="kiln-griddlg__radios" aria-label={t('Select a Rule Type')}>
        <legend>{t('Select a Rule Type')}</legend>
        {RULE_TYPES.map(([t, label]) => (
          <label key={t} className="kiln-griddlg__check">
            <input
              type="radio"
              name="cf-type"
              checked={type === t}
              onChange={() => setKind(DEFAULT_KIND[t])}
            />
            {tx(label)}
          </label>
        ))}
      </fieldset>
      <div className="kiln-gfx__section">
        <div className="kiln-griddlg__label">{t('Edit the Rule Description')}</div>
        <RuleEditor kind={kind} onKind={setKind} />
      </div>
      {highlight ? <StylePicker value={fmt} onChange={setFmt} /> : null}
    </FeatureFrame>
  );
}

/** The Conditional Formatting Rules Manager. */
export function ManageRulesDialog({ features }: { features: GridFeatures }) {
  const [sheet, setSheet] = useState(false);
  const [rules, setRules] = useState<GridCfRule[] | null>(null);
  const [selected, setSelected] = useState(0);
  const [editing, setEditing] = useState<{ index: number; rule: GridCfRule | null } | null>(null);
  useEffect(() => {
    setRules(null);
    void features.ask({ ConditionalFormats: { sheet } }).then((r) => {
      if (r && 'Rules' in r) setRules(r.Rules);
    });
  }, [features, sheet]);
  const list = rules ?? [];
  const move = (d: number) => {
    const j = selected + d;
    if (j < 0 || j >= list.length) return;
    const next = [...list];
    [next[selected], next[j]] = [next[j] as GridCfRule, next[selected] as GridCfRule];
    setRules(next);
    setSelected(j);
  };
  if (editing)
    return (
      <EmbeddedRuleDialog
        features={features}
        rule={editing.rule}
        onCancel={() => setEditing(null)}
        onDone={(r) => {
          const next = [...list];
          if (editing.index < 0) next.unshift(r);
          else next[editing.index] = r;
          setRules(next);
          setEditing(null);
        }}
      />
    );
  return (
    <FeatureFrame
      features={features}
      title={t('Conditional Formatting Rules Manager')}
      size="lg"
      testId="grid-cf-manage"
      okDisabled={!rules}
      onOk={() => features.tryExec({ SetConditionalFormats: { rules: list, sheet } })}
    >
      <div className="kiln-gfx__toolbar">
        <Select
          aria-label={t('Show formatting rules for')}
          value={sheet ? 'sheet' : 'sel'}
          width={200}
          options={[
            { value: 'sel', label: t('Current Selection') },
            { value: 'sheet', label: t('This Worksheet') },
          ]}
          onValueChange={(v) => setSheet(v === 'sheet')}
        />
        <Button size="sm" onClick={() => setEditing({ index: -1, rule: null })}>
          {t('New Rule…')}
        </Button>
        <Button
          size="sm"
          disabled={!list[selected]}
          onClick={() => setEditing({ index: selected, rule: list[selected] ?? null })}
        >
          {t('Edit Rule…')}
        </Button>
        <Button
          size="sm"
          disabled={!list[selected]}
          onClick={() => {
            setRules(list.filter((_, i) => i !== selected));
            setSelected((s) => Math.max(0, s - 1));
          }}
        >
          {t('Delete Rule')}
        </Button>
        <Button
          size="sm"
          disabled={!list[selected]}
          onClick={() => {
            const r = list[selected];
            if (r) setRules([...list.slice(0, selected + 1), { ...r, id: 0 }, ...list.slice(selected + 1)]);
          }}
        >
          {t('Duplicate Rule')}
        </Button>
        <Button size="sm" aria-label={t('Move Up')} disabled={selected <= 0} onClick={() => move(-1)}>
          ▲
        </Button>
        <Button
          size="sm"
          aria-label={t('Move Down')}
          disabled={selected >= list.length - 1}
          onClick={() => move(1)}
        >
          ▼
        </Button>
      </div>
      <div className="kiln-gfx__table kiln-gfx__rules">
        <div className="kiln-gfx__thead">
          <span>{t('Rule (applied in order shown)')}</span>
          <span>{t('Format')}</span>
          <span>{t('Applies to')}</span>
          <span>{t('Stop If True')}</span>
        </div>
        {list.length === 0 ? (
          <p className="kiln-gfx__hint">{rules ? t('No rules.') : t('Loading…')}</p>
        ) : null}
        {list.map((r, i) => (
          // biome-ignore lint/a11y/noStaticElementInteractions: clicking a rule row selects it; its inputs take the keys.
          <div
            key={`${r.id}-${i}`}
            tabIndex={-1}
            className={i === selected ? 'kiln-gfx__level kiln-gfx__level--on' : 'kiln-gfx__level'}
            onClick={() => setSelected(i)}
            onKeyDown={() => undefined}
            data-testid="grid-cf-row"
          >
            <span className="kiln-gfx__cell">{describe(r.kind)}</span>
            <span className="kiln-gfx__cell">
              <RulePreview rule={r} />
            </span>
            <input
              className="kiln-griddlg__input"
              aria-label={t('Applies to')}
              value={r.applies_to}
              onChange={(e) =>
                setRules(list.map((x, j) => (j === i ? { ...x, applies_to: e.target.value } : x)))
              }
            />
            <input
              type="checkbox"
              aria-label={t('Stop If True')}
              checked={r.stop_if_true}
              onChange={(e) =>
                setRules(list.map((x, j) => (j === i ? { ...x, stop_if_true: e.target.checked } : x)))
              }
            />
          </div>
        ))}
      </div>
    </FeatureFrame>
  );
}

/** The rule editor inside the manager (its own OK returns to the list). */
function EmbeddedRuleDialog({
  features,
  rule,
  onDone,
  onCancel,
}: {
  features: GridFeatures;
  rule: GridCfRule | null;
  onDone: (r: GridCfRule) => void;
  onCancel: () => void;
}) {
  // A wrapper whose close returns to the manager rather than the sheet.
  const proxy = Object.create(features) as GridFeatures;
  proxy.closeDialog = onCancel;
  return <RuleDialog features={proxy} rule={rule} onDone={onDone} />;
}
