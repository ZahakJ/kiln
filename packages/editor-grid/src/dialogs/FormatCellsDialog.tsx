/**
 * Format Cells (Ctrl+1; PLAN.md §1.3): Number, Alignment, Font, Border, Fill and Protection, filled
 * from the core's `Query::Grid(FormatCells)` for the selection. OK sends one `grid.format.apply`
 * with the members the user changed; the Number tab's Sample is the core's rendering of the active
 * cell in the chosen format.
 */
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
import { useEffect, useMemo, useState } from 'react';

/** The members of a format the dialog changes. */
type Patch = Partial<GridFormat>;

import type { GridEditorModel } from '../model.ts';
import { ColorButton } from './ColorButton.tsx';

/** Number format categories and the codes each offers (en-US, as Excel's dialog). */
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
  Custom: [
    'General',
    '0',
    '0.00',
    '#,##0',
    '0%',
    'm/d/yyyy',
    'h:mm',
    '@',
    '[Blue]#,##0;[Red]-#,##0;[Green]"zero"',
  ],
};

const H_ALIGNS: readonly { value: GridHAlign; label: string }[] = [
  { value: 'General', label: 'General' },
  { value: 'Left', label: 'Left (Indent)' },
  { value: 'Center', label: 'Center' },
  { value: 'Right', label: 'Right (Indent)' },
  { value: 'Fill', label: 'Fill' },
  { value: 'Justify', label: 'Justify' },
  { value: 'CenterAcross', label: 'Center Across Selection' },
  { value: 'Distributed', label: 'Distributed (Indent)' },
];

const V_ALIGNS: readonly { value: GridVAlign; label: string }[] = [
  { value: 'Top', label: 'Top' },
  { value: 'Center', label: 'Center' },
  { value: 'Bottom', label: 'Bottom' },
  { value: 'Justify', label: 'Justify' },
  { value: 'Distributed', label: 'Distributed' },
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
  { value: 'none', label: 'None' },
  { value: 'single', label: 'Single' },
  { value: 'double', label: 'Double' },
  { value: 'singleAccounting', label: 'Single Accounting' },
  { value: 'doubleAccounting', label: 'Double Accounting' },
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
  if (style === 'None') return <span className="kiln-fc__line-none">None</span>;
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
  const lineColorCss = lineColor ? `#${lineColor}` : 'currentColor';
  const sideStyle = (s: Side) => {
    const b = sides[s];
    return b ? <LineSwatch style={b.style} color={b.color ? `#${b.color}` : 'currentColor'} /> : null;
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : onClose())}>
      <DialogContent
        title="Format Cells"
        size="lg"
        className="kiln-fc"
        footer={
          <>
            <Button variant="default" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" onClick={apply} disabled={!initial}>
              OK
            </Button>
          </>
        }
      >
        <Tabs value={active} onValueChange={setActive}>
          <TabsList aria-label="Format Cells">
            <TabsTrigger value="number">Number</TabsTrigger>
            <TabsTrigger value="alignment">Alignment</TabsTrigger>
            <TabsTrigger value="font">Font</TabsTrigger>
            <TabsTrigger value="border">Border</TabsTrigger>
            <TabsTrigger value="fill">Fill</TabsTrigger>
            <TabsTrigger value="protection">Protection</TabsTrigger>
          </TabsList>
          <TabsContent value="number" className="kiln-fc__tab kiln-fc__number">
            <div className="kiln-fc__col">
              <div className="kiln-fc__label" id="fc-category">
                Category
              </div>
              <div className="kiln-fc__list" role="listbox" aria-labelledby="fc-category">
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
                    {c}
                  </div>
                ))}
              </div>
            </div>
            <div className="kiln-fc__col kiln-fc__grow">
              <div className="kiln-fc__sample" aria-live="polite">
                <span className="kiln-fc__label">Sample</span>
                <output>{sample || ' '}</output>
              </div>
              <label className="kiln-fc__label" htmlFor="fc-code">
                Type
              </label>
              <input
                id="fc-code"
                className="kiln-fc__input"
                value={code}
                spellCheck={false}
                onChange={(e) => setCode(e.target.value)}
              />
              <div className="kiln-fc__list kiln-fc__codes" role="listbox" aria-label="Format codes">
                {(CATEGORIES[category] ?? []).map((c) => (
                  <div
                    key={c}
                    role="option"
                    aria-selected={c === code}
                    className={c === code ? 'kiln-fc__option kiln-fc__option--on' : 'kiln-fc__option'}
                    onClick={() => setCode(c)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') setCode(c);
                    }}
                    tabIndex={-1}
                  >
                    {c}
                  </div>
                ))}
              </div>
            </div>
          </TabsContent>
          <TabsContent value="alignment" className="kiln-fc__tab">
            <fieldset className="kiln-fc__group">
              <legend>Text alignment</legend>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">Horizontal</span>
                <Select
                  aria-label="Horizontal"
                  value={value('h_align', fmt?.h_align ?? 'General')}
                  onValueChange={(v) => set({ h_align: v as GridHAlign })}
                  options={H_ALIGNS.map((a) => ({ value: a.value, label: a.label }))}
                  width={220}
                />
              </span>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">Indent</span>
                <NumberInput
                  aria-label="Indent"
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
                <span className="kiln-fc__label">Vertical</span>
                <Select
                  aria-label="Vertical"
                  value={value('v_align', fmt?.v_align ?? 'Bottom')}
                  onValueChange={(v) => set({ v_align: v as GridVAlign })}
                  options={V_ALIGNS.map((a) => ({ value: a.value, label: a.label }))}
                  width={220}
                />
              </span>
            </fieldset>
            <fieldset className="kiln-fc__group">
              <legend>Text control</legend>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('wrap', fmt?.wrap ?? false)}
                  onChange={(e) => set({ wrap: e.target.checked })}
                />
                Wrap text
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('shrink', initial?.shrink ?? false)}
                  onChange={(e) => set({ shrink: e.target.checked })}
                />
                Shrink to fit
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('merge', fmt?.merged ?? false)}
                  onChange={(e) => set({ merge: e.target.checked })}
                />
                Merge cells
              </label>
            </fieldset>
            <fieldset className="kiln-fc__group">
              <legend>Orientation</legend>
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">Degrees</span>
                <NumberInput
                  aria-label="Degrees"
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
                Vertical text
              </label>
            </fieldset>
          </TabsContent>
          <TabsContent value="font" className="kiln-fc__tab kiln-fc__font">
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">Font</span>
              <Select
                aria-label="Font"
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
              <span className="kiln-fc__label">Font style</span>
              <Select
                aria-label="Font style"
                value={
                  `${value('bold', fmt?.bold ?? false) ? 'b' : ''}${value('italic', fmt?.italic ?? false) ? 'i' : ''}` ||
                  'r'
                }
                onValueChange={(v) => set({ bold: v.includes('b'), italic: v.includes('i') })}
                options={[
                  { value: 'r', label: 'Regular' },
                  { value: 'i', label: 'Italic' },
                  { value: 'b', label: 'Bold' },
                  { value: 'bi', label: 'Bold Italic' },
                ]}
                width={140}
              />
            </span>
            <span className="kiln-fc__field">
              <span className="kiln-fc__label">Size</span>
              <NumberInput
                aria-label="Size"
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
              <span className="kiln-fc__label">Underline</span>
              <Select
                aria-label="Underline"
                value={value('underline', initial?.underline ?? 'none')}
                onValueChange={(v) => set({ underline: v })}
                options={UNDERLINES}
                width={180}
              />
            </span>
            <fieldset className="kiln-fc__group">
              <legend>Effects</legend>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('strike', fmt?.strike ?? false)}
                  onChange={(e) => set({ strike: e.target.checked })}
                />
                Strikethrough
              </label>
              <label className="kiln-fc__check">
                <input
                  type="checkbox"
                  checked={value('superscript', initial?.superscript ?? false)}
                  onChange={(e) =>
                    set({ superscript: e.target.checked, subscript: e.target.checked ? false : f.subscript })
                  }
                />
                Superscript
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
                Subscript
              </label>
            </fieldset>
            <ColorButton
              label="Color"
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
              AaBbCcYyZz
            </div>
          </TabsContent>
          <TabsContent value="border" className="kiln-fc__tab kiln-fc__border">
            <div className="kiln-fc__col">
              <div className="kiln-fc__label">Line style</div>
              <div className="kiln-fc__lines" role="listbox" aria-label="Line style">
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
                label="Color"
                allowAuto
                value={lineColor}
                onChange={(c) => setLineColor(c.type === 'auto' ? null : hexOf(c))}
              />
            </div>
            <div className="kiln-fc__col kiln-fc__grow">
              <div className="kiln-fc__presets">
                <Button variant="default" onClick={() => setSides({})}>
                  None
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
                  Outline
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
                  Inside
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
                  Text
                </div>
              </div>
            </div>
          </TabsContent>
          <TabsContent value="fill" className="kiln-fc__tab kiln-fc__fill">
            <div className="kiln-fc__col">
              <ColorButton
                label="Background Color"
                allowNone
                value={f.fill_color !== undefined ? f.fill_color || null : (fmt?.fill_color ?? null)}
                onChange={(c) => set({ fill_color: c.type === 'none' ? '' : (hexOf(c) ?? '') })}
              />
            </div>
            <div className="kiln-fc__col">
              <span className="kiln-fc__field">
                <span className="kiln-fc__label">Pattern Style</span>
                <Select
                  aria-label="Pattern Style"
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
                label="Pattern Color"
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
                <span className="kiln-fc__label">Sample</span>
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
              Locked
            </label>
            <label className="kiln-fc__check">
              <input
                type="checkbox"
                checked={value('hidden', initial?.hidden ?? false)}
                onChange={(e) => set({ hidden: e.target.checked })}
              />
              Hidden
            </label>
            <p className="kiln-fc__note">
              Locking cells or hiding formulas has no effect until you protect the sheet (Review ▸ Protect
              Sheet).
            </p>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
