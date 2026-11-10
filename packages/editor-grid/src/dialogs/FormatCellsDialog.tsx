/**
 * Format Cells (Ctrl+1): Number, Alignment, Font, Border, Fill and Protection, filled
 * from the core's `Query::Grid(FormatCells)` for the selection. OK sends one `grid.format.apply`
 * with the members the user changed; the Number tab's Sample is the core's rendering of the active
 * cell in the chosen format.
 */
import { localizeNumberText, msg, t, tx, txOptions } from '@kiln/i18n';
import type {
  GridBorderSide,
  GridFormat,
  GridFormatCells,
  GridHAlign,
  GridLineStyle,
  GridVAlign,
} from '@kiln/protocol';
import {
  Button,
  type ColorChoice,
  Dialog,
  DialogContent,
  NumberInput,
  Select,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@kiln/ui-kit';
import { type KeyboardEvent, useEffect, useMemo, useState } from 'react';

/** The members of a format the dialog changes. */
type Patch = Partial<GridFormat>;

import type { GridEditorModel } from '../model.ts';
import { ColorButton } from './ColorButton.tsx';

/** Number format categories and the codes each offers (en-US, as Excel's dialog). */
/**
 * The arrows, Home and End in a list of options: the next item (focused once it is drawn), or
 * null for another key.
 */
function arrowed(e: KeyboardEvent<HTMLElement>, items: readonly string[], current: string): string | null {
  const at = Math.max(0, items.indexOf(current));
  const to =
    e.key === 'ArrowDown'
      ? Math.min(items.length - 1, at + 1)
      : e.key === 'ArrowUp'
        ? Math.max(0, at - 1)
        : e.key === 'Home'
          ? 0
          : e.key === 'End'
            ? items.length - 1
            : null;
  if (to === null) return null;
  e.preventDefault();
  const list = e.currentTarget;
  requestAnimationFrame(() => list.querySelectorAll<HTMLElement>('[role="option"]')[to]?.focus());
  return items[to] ?? null;
}

/** The categories' names in the list (the keys of `CATEGORIES`). */
const CATEGORY_NAMES: Readonly<Record<string, string>> = {
  General: msg('General'),
  Number: msg('Number'),
  Currency: msg('Currency'),
  Accounting: msg('Accounting'),
  Date: msg('Date'),
  Time: msg('Time'),
  Percentage: msg('Percentage'),
  Fraction: msg('Fraction'),
  Scientific: msg('Scientific'),
  Text: msg('Text'),
  Special: msg('Special'),
  Custom: msg('Custom'),
};

export const CATEGORIES: Readonly<Record<string, readonly string[]>> = {
  General: ['General'],
  Number: ['0', '0.00', '#,##0', '#,##0.00', '#,##0.00_);(#,##0.00)', '#,##0.00_);[Red](#,##0.00)'],
  Currency: [
    '"$"#,##0.00',
    '"$"#,##0.00_);("$"#,##0.00)',
    '"$"#,##0.00_);[Red]("$"#,##0.00)',
    '"£"#,##0.00',
    '"€"#,##0.00',
  ],
  Accounting: [
    '_("$"* #,##0.00_);_("$"* \\(#,##0.00\\);_("$"* "-"??_);_(@_)',
    '_("£"* #,##0.00_);_("£"* \\(#,##0.00\\);_("£"* "-"??_);_(@_)',
    '_(* #,##0.00_);_(* \\(#,##0.00\\);_(* "-"??_);_(@_)',
  ],
  Date: ['m/d/yyyy', 'dddd, mmmm d, yyyy', 'd-mmm-yy', 'd-mmm', 'mmm-yy', 'yyyy-mm-dd', 'm/d/yyyy h:mm'],
  Time: ['h:mm:ss AM/PM', 'h:mm AM/PM', 'h:mm', 'h:mm:ss', '[h]:mm:ss', 'mm:ss.0'],
  Percentage: ['0%', '0.00%'],
  Fraction: ['# ?/?', '# ??/??', '# ?/4', '# ?/8', '# ??/16'],
  Scientific: ['0.00E+00', '##0.0E+0'],
  Text: ['@'],
  Special: ['00000', '00000-0000', '[<=9999999]###-####;(###) ###-####', '000-00-0000'],
  // Excel's built-in codes, as its Custom list shows them; the workbook's own follow.
  Custom: [
    'General',
    '0',
    '0.00',
    '#,##0',
    '#,##0.00',
    '#,##0_);(#,##0)',
    '#,##0_);[Red](#,##0)',
    '#,##0.00_);(#,##0.00)',
    '#,##0.00_);[Red](#,##0.00)',
    '"$"#,##0_);("$"#,##0)',
    '"$"#,##0_);[Red]("$"#,##0)',
    '"$"#,##0.00_);("$"#,##0.00)',
    '"$"#,##0.00_);[Red]("$"#,##0.00)',
    '0%',
    '0.00%',
    '0.00E+00',
    '##0.0E+0',
    '# ?/?',
    '# ??/??',
    'm/d/yyyy',
    'd-mmm-yy',
    'd-mmm',
    'mmm-yy',
    'h:mm AM/PM',
    'h:mm:ss AM/PM',
    'h:mm',
    'h:mm:ss',
    'm/d/yyyy h:mm',
    'mm:ss',
    'mm:ss.0',
    '@',
    '[h]:mm:ss',
    '_("$"* #,##0_);_("$"* \\(#,##0\\);_("$"* "-"_);_(@_)',
    '_(* #,##0_);_(* \\(#,##0\\);_(* "-"_);_(@_)',
    '_("$"* #,##0.00_);_("$"* \\(#,##0.00\\);_("$"* "-"??_);_(@_)',
    '_(* #,##0.00_);_(* \\(#,##0.00\\);_(* "-"??_);_(@_)',
    '[Blue]#,##0;[Red]-#,##0;[Green]"zero"',
  ],
};

const H_ALIGNS: readonly { value: GridHAlign; label: string }[] = [
  { value: 'General', label: msg('General') },
  { value: 'Left', label: msg('Left (Indent)') },
  { value: 'Center', label: msg('Center') },
  { value: 'Right', label: msg('Right (Indent)') },
  { value: 'Fill', label: msg('Fill') },
  { value: 'Justify', label: msg('Justify') },
  { value: 'CenterAcross', label: msg('Center Across Selection') },
  { value: 'Distributed', label: msg('Distributed (Indent)') },
];

const V_ALIGNS: readonly { value: GridVAlign; label: string }[] = [
  { value: 'Top', label: msg('Top') },
  { value: 'Center', label: msg('Center') },
  { value: 'Bottom', label: msg('Bottom') },
  { value: 'Justify', label: msg('Justify') },
  { value: 'Distributed', label: msg('Distributed') },
];

const LINES: readonly GridLineStyle[] = [
  'None',
  'Hair',
  'Dotted',
  'DashDotDot',
  'DashDot',
  'Dashed',
  'Thin',
  'MediumDashDotDot',
  'SlantDashDot',
  'MediumDashDot',
  'MediumDashed',
  'Medium',
  'Thick',
  'Double',
];

const UNDERLINES = [
  { value: 'none', label: msg('None') },
  { value: 'single', label: msg('Single') },
  { value: 'double', label: msg('Double') },
  { value: 'singleAccounting', label: msg('Single Accounting') },
  { value: 'doubleAccounting', label: msg('Double Accounting') },
];

const PATTERNS = [
  'none',
  'solid',
  'gray125',
  'gray0625',
  'lightGray',
  'mediumGray',
  'darkGray',
  'lightHorizontal',
  'lightVertical',
  'lightDown',
  'lightUp',
  'lightGrid',
  'lightTrellis',
  'darkHorizontal',
  'darkVertical',
  'darkDown',
  'darkUp',
  'darkGrid',
  'darkTrellis',
];

/** The dash pattern a line style is drawn with in the dialog's previews (SVG `stroke-dasharray`). */
export function dashOf(style: GridLineStyle): { width: number; dash?: string; double?: boolean } {
  switch (style) {
    case 'None':
      return { width: 0 };
    case 'Hair':
      return { width: 1, dash: '1 1' };
    case 'Dotted':
      return { width: 1, dash: '1 2' };
    case 'Dashed':
      return { width: 1, dash: '3 1' };
    case 'DashDot':
      return { width: 1, dash: '9 3 3 3' };
    case 'DashDotDot':
      return { width: 1, dash: '9 3 3 3 3 3' };
    case 'Thin':
      return { width: 1 };
    case 'Medium':
      return { width: 2 };
    case 'MediumDashed':
      return { width: 2, dash: '9 3' };
    case 'MediumDashDot':
      return { width: 2, dash: '9 3 3 3' };
    case 'MediumDashDotDot':
      return { width: 2, dash: '9 3 3 3 3 3' };
    case 'SlantDashDot':
      return { width: 2, dash: '11 1 5 1' };
    case 'Thick':
      return { width: 3 };
    case 'Double':
      return { width: 1, double: true };
  }
}

function LineSwatch({ style, color = 'currentColor' }: { style: GridLineStyle; color?: string }) {
  const d = dashOf(style);
  if (style === 'None') return <span className="kiln-fc__line-none">{t('None')}</span>;
  return (
    <svg width="56" height="10" aria-hidden="true">
      {d.double ? (
        <>
          <line x1="2" y1="3" x2="54" y2="3" stroke={color} strokeWidth="1" />
          <line x1="2" y1="7" x2="54" y2="7" stroke={color} strokeWidth="1" />
        </>
      ) : (
        <line x1="2" y1="5" x2="54" y2="5" stroke={color} strokeWidth={d.width} strokeDasharray={d.dash} />
      )}
    </svg>
  );
}

type Side = 'top' | 'bottom' | 'left' | 'right' | 'insideH' | 'insideV' | 'diagUp' | 'diagDown';

const hexOf = (c: ColorChoice): string | null =>
  c.type === 'rgb' || c.type === 'theme' ? c.hex.replace('#', '').toUpperCase() : null;

export function FormatCellsDialog({
  model,
  tab,
  onClose,
}: {
  model: GridEditorModel;
  tab?: string;
  onClose: () => void;
}) {
  const [initial, setInitial] = useState<GridFormatCells | null>(null);
  const [f, setF] = useState<Patch>({});
  const [active, setActive] = useState(tab ?? 'number');
  const [category, setCategory] = useState('General');
  const [code, setCode] = useState('General');
  const [sample, setSample] = useState('');
  const [line, setLine] = useState<GridLineStyle>('Thin');
  const [lineColor, setLineColor] = useState<string | null>(null);
  const [sides, setSides] = useState<Partial<Record<Side, GridBorderSide>>>({});
  useEffect(() => {
    let live = true;
    void model.formatCells().then((r) => {
      if (!live || !r) return;
      setInitial(r);
      setCategory(r.category);
      setCode(r.number_format);
      setSample(r.sample);
      const s: Partial<Record<Side, GridBorderSide>> = {};
      if (r.border_top) s.top = r.border_top;
      if (r.border_bottom) s.bottom = r.border_bottom;
      if (r.border_left) s.left = r.border_left;
      if (r.border_right) s.right = r.border_right;
      if (r.border_diagonal_up) s.diagUp = r.border_diagonal_up;
      if (r.border_diagonal_down) s.diagDown = r.border_diagonal_down;
      setSides(s);
    });
    return () => {
      live = false;
    };
  }, [model]);
  // The sample follows the code.
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      void model.preview(code).then((s) => {
        if (live) setSample(s);
      });
    }, 80);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [model, code]);
  const set = (patch: Patch) => setF((x) => ({ ...x, ...patch }));
  const fmt = initial?.format;
  const value = <K extends keyof GridFormat>(
    k: K,
    fallback: NonNullable<GridFormat[K]>,
  ): NonNullable<GridFormat[K]> => (f[k] ?? fallback) as NonNullable<GridFormat[K]>;
  const toggleSide = (side: Side) =>
    setSides((s) => {
      const cur = s[side];
      const next = { ...s };
      if (cur && cur.style === line && cur.color === lineColor) delete next[side];
      else next[side] = { style: line, color: lineColor };
      return next;
    });
  const borderPatch = useMemo(() => {
    if (!initial) return {};
    const out: Patch = {};
    const none: GridBorderSide = { style: 'None', color: null };
    const was: Partial<Record<Side, GridBorderSide | null>> = {
      top: initial.border_top,
      bottom: initial.border_bottom,
      left: initial.border_left,
      right: initial.border_right,
      diagUp: initial.border_diagonal_up,
      diagDown: initial.border_diagonal_down,
    };
    const keys: Record<Side, keyof GridFormat> = {
      top: 'border_top',
      bottom: 'border_bottom',
      left: 'border_left',
      right: 'border_right',
      insideH: 'border_inside_h',
      insideV: 'border_inside_v',
      diagUp: 'border_diagonal_up',
      diagDown: 'border_diagonal_down',
    };
    for (const side of Object.keys(keys) as Side[]) {
      const now = sides[side] ?? null;
      const before = was[side] ?? null;
      if (JSON.stringify(now) !== JSON.stringify(before))
        (out as Record<string, unknown>)[keys[side]] = now ?? none;
    }
    return out;
  }, [sides, initial]);
  const apply = () => {
    const format: Patch = { ...f, ...borderPatch };
    if (initial && code !== initial.number_format) format.number_format = code;
    if (Object.keys(format).length > 0) void model.run('grid.format.apply', { format });
    onClose();
  };
  const fonts = initial?.fonts ?? [];
  const own = initial?.custom_codes ?? [];
  const codes =
    category === 'Custom'
      ? [...(CATEGORIES.Custom ?? []), ...own.filter((c) => !(CATEGORIES.Custom ?? []).includes(c))]
      : (CATEGORIES[category] ?? []);
  const lineColorCss = lineColor ? `#${lineColor}` : 'currentColor';
  const sideStyle = (s: Side) => {
    const b = sides[s];
    return b ? <LineSwatch style={b.style} color={b.color ? `#${b.color}` : 'currentColor'} /> : null;
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : onClose())}>
      <DialogContent
        title={t('Format Cells')}
        size="lg"
        className="kiln-fc"
        data-kiln-shortcuts="off"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          model.surface?.focus();
        }}
        footer={
          <>
            <Button variant="default" onClick={onClose}>
              {t('Cancel')}
            </Button>
            <Button variant="primary" onClick={apply} disabled={!initial}>
              {t('OK')}
            </Button>
          </>
        }
      >
        <Tabs value={active} onValueChange={setActive}>
          <TabsList aria-label={t('Format Cells')}>
            <TabsTrigger value="number">{t('Number')}</TabsTrigger>
            <TabsTrigger value="alignment">{t('Alignment')}</TabsTrigger>
            <TabsTrigger value="font">{t('Font')}</TabsTrigger>
            <TabsTrigger value="border">{t('Border')}</TabsTrigger>
            <TabsTrigger value="fill">{t('Fill')}</TabsTrigger>
            <TabsTrigger value="protection">{t('Protection')}</TabsTrigger>
          </TabsList>
          <TabsContent value="number" className="kiln-fc__tab kiln-fc__number">
            <div className="kiln-fc__col">
              <div className="kiln-fc__label" id="fc-category">
                {t('Category')}
              </div>
              <div
                className="kiln-fc__list"
                role="listbox"
                aria-labelledby="fc-category"
                onKeyDown={(e) => {
                  const next = arrowed(e, Object.keys(CATEGORIES), category);
                  if (next === null) return;
                  setCategory(next);
                  const first = CATEGORIES[next]?.[0];
                  if (first && next !== 'Custom') setCode(first);
                }}
              >
                {Object.keys(CATEGORIES).map((c) => (
                  <div
                    key={c}
                    role="option"
                    aria-selected={c === category}
                    tabIndex={c === category ? 0 : -1}
                    className={c === category ? 'kiln-fc__option kiln-fc__option--on' : 'kiln-fc__option'}
                    onClick={() => {
                      setCategory(c);
                      const first = CATEGORIES[c]?.[0];
                      if (first && c !== 'Custom') setCode(first);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') setCategory(c);
                    }}
                  >
                    {tx(CATEGORY_NAMES[c] ?? c)}
                  </div>
                ))}
              </div>
            </div>
            <div className="kiln-fc__col kiln-fc__grow">
              <div className="kiln-fc__sample" aria-live="polite">
                <span className="kiln-fc__label">{t('Sample')}</span>
                <output>{sample ? localizeNumberText(sample) : ' '}</output>
              </div>
              <label className="kiln-fc__label" htmlFor="fc-code">
                {t('Type')}
              </label>
              <input
                id="fc-code"
                className="kiln-fc__input"
                value={code}
                spellCheck={false}
                onChange={(e) => setCode(e.target.value)}
              />
              <div
                className="kiln-fc__list kiln-fc__codes"
                role="listbox"
                aria-label={t('Format codes')}
                onKeyDown={(e) => {
                  const next = arrowed(e, codes, code);
                  if (next !== null) setCode(next);
                }}
              >
                {codes.map((c, i) => (
                  <div
                    key={c}
                    role="option"
                    aria-selected={c === code}
                    className={c === code ? 'kiln-fc__option kiln-fc__option--on' : 'kiln-fc__option'}
                    onClick={() => setCode(c)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') setCode(c);
                    }}
                    // One stop in the Tab order: the chosen code (or the first), the arrows move.
                    tabIndex={c === code || (i === 0 && !codes.includes(code)) ? 0 : -1}
                  >
                    {c === 'General' ? t('General') : c}
                  </div>
                ))}
              </div>
              {category === 'Custom' ? (
                <div className="kiln-fc__row">
                  <Button
                    disabled={!own.includes(code)}
                    onClick={async () => {
                      // Delete: cells in this format return to General (built-in codes stay).
                      if (!(await model.run('grid.number.delete_format', { code }))) return;
                      const r = await model.formatCells();
                      if (r) setInitial(r);
                      setCode('General');
                    }}
                  >
                    {t('Delete')}
                  </Button>
                </div>
              ) : null}
            </div>
          </TabsContent>
          <TabsContent value="alignment" className="kiln-fc__tab">
            <fieldset className="kiln-fc__group">
              <legend>{t('Text alignment')}</legend>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">{t('Horizontal')}</span>
                <Select
                  aria-label={t('Horizontal')}
                  value={value('h_align', fmt?.h_align ?? 'General')}
                  onValueChange={(v) => set({ h_align: v as GridHAlign })}
                  options={H_ALIGNS.map((a) => ({ value: a.value, label: tx(a.label) }))}
                  width={220}
                />
              </span>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">{t('Indent')}</span>
                <NumberInput
                  aria-label={t('Indent')}
                  value={value('indent', fmt?.indent ?? 0)}
                  min={0}
                  max={250}
                  precision={0}
                  unit=""
                  onValueChange={(v) => set({ indent: Math.round(v) })}
                  width={80}
                />
              </span>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">{t('Vertical')}</span>
                <Select
                  aria-label={t('Vertical')}
                  value={value('v_align', fmt?.v_align ?? 'Bottom')}
                  onValueChange={(v) => set({ v_align: v as GridVAlign })}
                  options={V_ALIGNS.map((a) => ({ value: a.value, label: tx(a.label) }))}
                  width={220}
                />
              </span>
            </fieldset>
            <fieldset className="kiln-fc__group">
              <legend>{t('Text control')}</legend>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('wrap', fmt?.wrap ?? false)}
                  onChange={(e) => set({ wrap: e.target.checked })}
                />
                {t('Wrap text')}
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('shrink', initial?.shrink ?? false)}
                  onChange={(e) => set({ shrink: e.target.checked })}
                />
                {t('Shrink to fit')}
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('merge', fmt?.merged ?? false)}
                  onChange={(e) => set({ merge: e.target.checked })}
                />
                {t('Merge cells')}
              </label>
            </fieldset>
            <fieldset className="kiln-fc__group">
              <legend>{t('Orientation')}</legend>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">{t('Degrees')}</span>
                <NumberInput
                  aria-label={t('Degrees')}
                  value={(() => {
                    const r = value('rotation', fmt?.rotation ?? 0);
                    return r > 90 && r <= 180 ? 90 - r : r === 255 ? 0 : r;
                  })()}
                  min={-90}
                  max={90}
                  precision={0}
                  unit="°"
                  onValueChange={(v) => {
                    const d = Math.round(v);
                    set({ rotation: d < 0 ? 90 - d : d });
                  }}
                  width={90}
                />
              </span>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('rotation', fmt?.rotation ?? 0) === 255}
                  onChange={(e) => set({ rotation: e.target.checked ? 255 : 0 })}
                />
                {t('Vertical text')}
              </label>
            </fieldset>
          </TabsContent>
          <TabsContent value="font" className="kiln-fc__tab kiln-fc__font">
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">{t('Font')}</span>
              <Select
                aria-label={t('Font')}
                value={value('font_family', fmt?.font_family ?? 'Calibri')}
                onValueChange={(v) => set({ font_family: v })}
                options={[...new Set([fmt?.font_family ?? 'Calibri', ...fonts])].map((n) => ({
                  value: n,
                  label: <span style={{ fontFamily: n }}>{n}</span>,
                  textValue: n,
                }))}
                width={220}
              />
            </span>
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">{t('Font style')}</span>
              <Select
                aria-label={t('Font style')}
                value={
                  `${value('bold', fmt?.bold ?? false) ? 'b' : ''}${value('italic', fmt?.italic ?? false) ? 'i' : ''}` ||
                  'r'
                }
                onValueChange={(v) => set({ bold: v.includes('b'), italic: v.includes('i') })}
                options={[
                  { value: 'r', label: t('Regular') },
                  { value: 'i', label: t('Italic') },
                  { value: 'b', label: t('Bold') },
                  { value: 'bi', label: t('Bold Italic') },
                ]}
                width={140}
              />
            </span>
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">{t('Size')}</span>
              <NumberInput
                aria-label={t('Size')}
                value={value('font_size', fmt?.font_size ?? 11)}
                min={1}
                max={409}
                step={1}
                precision={1}
                unit=""
                onValueChange={(v) => set({ font_size: v })}
                width={80}
              />
            </span>
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">{t('Underline')}</span>
              <Select
                aria-label={t('Underline')}
                value={value('underline', initial?.underline ?? 'none')}
                onValueChange={(v) => set({ underline: v })}
                options={txOptions(UNDERLINES)}
                width={180}
              />
            </span>
            <fieldset className="kiln-fc__group">
              <legend>{t('Effects')}</legend>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('strike', fmt?.strike ?? false)}
                  onChange={(e) => set({ strike: e.target.checked })}
                />
                {t('Strikethrough')}
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('superscript', initial?.superscript ?? false)}
                  onChange={(e) =>
                    set({ superscript: e.target.checked, subscript: e.target.checked ? false : f.subscript })
                  }
                />
                {t('Superscript')}
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('subscript', initial?.subscript ?? false)}
                  onChange={(e) =>
                    set({
                      subscript: e.target.checked,
                      superscript: e.target.checked ? false : f.superscript,
                    })
                  }
                />
                {t('Subscript')}
              </label>
            </fieldset>
            <ColorButton
              label={t('Color')}
              allowAuto
              value={f.font_color !== undefined ? f.font_color || null : (fmt?.font_color ?? null)}
              onChange={(c) => set({ font_color: c.type === 'auto' ? '' : (hexOf(c) ?? '') })}
            />
            <div
              className="kiln-fc__preview"
              style={{
                fontFamily: value('font_family', fmt?.font_family ?? 'Calibri'),
                fontWeight: value('bold', fmt?.bold ?? false) ? 700 : 400,
                fontStyle: value('italic', fmt?.italic ?? false) ? 'italic' : 'normal',
                textDecoration:
                  `${value('underline', initial?.underline ?? 'none') !== 'none' ? 'underline' : ''} ${value('strike', fmt?.strike ?? false) ? 'line-through' : ''}`.trim() ||
                  'none',
                color: f.font_color ? `#${f.font_color}` : fmt?.font_color ? `#${fmt.font_color}` : undefined,
              }}
            >
              {/* i18n-ignore: the font sample */}
              AaBbCcYyZz
            </div>
          </TabsContent>
          <TabsContent value="border" className="kiln-fc__tab kiln-fc__border">
            <div className="kiln-fc__col">
              <div className="kiln-fc__label">{t('Line style')}</div>
              <div className="kiln-fc__lines" role="listbox" aria-label={t('Line style')}>
                {LINES.map((l) => (
                  <div
                    key={l}
                    role="option"
                    aria-selected={l === line}
                    aria-label={l}
                    tabIndex={l === line ? 0 : -1}
                    className={l === line ? 'kiln-fc__option kiln-fc__option--on' : 'kiln-fc__option'}
                    onClick={() => setLine(l)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') setLine(l);
                    }}
                  >
                    <LineSwatch style={l} color={lineColorCss} />
                  </div>
                ))}
              </div>
              <ColorButton
                label={t('Color')}
                allowAuto
                value={lineColor}
                onChange={(c) => setLineColor(c.type === 'auto' ? null : hexOf(c))}
              />
            </div>
            <div className="kiln-fc__col kiln-fc__grow">
              <div className="kiln-fc__presets">
                <Button variant="default" onClick={() => setSides({})}>
                  {t('None')}
                </Button>
                <Button
                  variant="default"
                  onClick={() =>
                    setSides((s) => ({
                      ...s,
                      top: { style: line, color: lineColor },
                      bottom: { style: line, color: lineColor },
                      left: { style: line, color: lineColor },
                      right: { style: line, color: lineColor },
                    }))
                  }
                >
                  {t('Outline')}
                </Button>
                <Button
                  variant="default"
                  onClick={() =>
                    setSides((s) => ({
                      ...s,
                      insideH: { style: line, color: lineColor },
                      insideV: { style: line, color: lineColor },
                    }))
                  }
                >
                  {t('Inside')}
                </Button>
              </div>
              <div className="kiln-fc__borderbox">
                {(
                  ['top', 'insideH', 'bottom', 'diagUp', 'left', 'insideV', 'right', 'diagDown'] as Side[]
                ).map((s) => (
                  <button
                    key={s}
                    type="button"
                    className={`kiln-fc__side kiln-fc__side--${s}${sides[s] ? ' kiln-fc__side--on' : ''}`}
                    aria-pressed={!!sides[s]}
                    aria-label={
                      {
                        top: 'Top border',
                        bottom: 'Bottom border',
                        left: 'Left border',
                        right: 'Right border',
                        insideH: 'Inside horizontal border',
                        insideV: 'Inside vertical border',
                        diagUp: 'Diagonal up border',
                        diagDown: 'Diagonal down border',
                      }[s]
                    }
                    onClick={() => toggleSide(s)}
                  >
                    {sideStyle(s)}
                  </button>
                ))}
                <div className="kiln-fc__borderpreview" aria-hidden="true">
                  {t('Text')}
                </div>
              </div>
            </div>
          </TabsContent>
          <TabsContent value="fill" className="kiln-fc__tab kiln-fc__fill">
            <div className="kiln-fc__col">
              <ColorButton
                label={t('Background Color')}
                allowNone
                value={f.fill_color !== undefined ? f.fill_color || null : (fmt?.fill_color ?? null)}
                onChange={(c) => set({ fill_color: c.type === 'none' ? '' : (hexOf(c) ?? '') })}
              />
            </div>
            <div className="kiln-fc__col">
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">{t('Pattern Style')}</span>
                <Select
                  aria-label={t('Pattern Style')}
                  value={value('pattern', initial?.pattern ?? 'none')}
                  onValueChange={(v) => set({ pattern: v })}
                  options={PATTERNS.map((p) => ({
                    value: p,
                    label: p.replace(/([A-Z0-9]+)/g, ' $1').replace(/^./, (x) => x.toUpperCase()),
                  }))}
                  width={180}
                />
              </span>
              <ColorButton
                label={t('Pattern Color')}
                allowAuto
                value={f.pattern_color !== undefined ? f.pattern_color : (initial?.pattern_color ?? null)}
                onChange={(c) => set({ pattern_color: c.type === 'auto' ? null : hexOf(c) })}
              />
              <div
                className="kiln-fc__sample kiln-fc__fillsample"
                style={{
                  background: f.fill_color
                    ? `#${f.fill_color}`
                    : fmt?.fill_color
                      ? `#${fmt.fill_color}`
                      : 'var(--kiln-page)',
                }}
              >
                <span className="kiln-fc__label">{t('Sample')}</span>
              </div>
            </div>
          </TabsContent>
          <TabsContent value="protection" className="kiln-fc__tab">
            <label className="kiln-fc__check">
              <input
                type="checkbox"
                checked={value('locked', initial?.locked ?? true)}
                onChange={(e) => set({ locked: e.target.checked })}
              />
              {t('Locked')}
            </label>
            <label className="kiln-fc__check">
              <input
                type="checkbox"
                checked={value('hidden', initial?.hidden ?? false)}
                onChange={(e) => set({ hidden: e.target.checked })}
              />
              {t('Hidden')}
            </label>
            <p className="kiln-fc__note">
              {t(
                'Locking cells or hiding formulas has no effect until you protect the sheet (Review ▸ Protect Sheet).',
              )}
            </p>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
