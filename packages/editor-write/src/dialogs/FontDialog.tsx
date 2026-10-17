/** The Font dialog (Ctrl+D, Ctrl+Shift+F, Home ▸ Font ↘): Font and Advanced tabs. */
import type { FontFormat, UnderlineKind } from '@kiln/protocol';
import { Combobox, NumberInput, Select, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import type { WriteEditorModel } from '../model.ts';
import { Check, Field, Grid, Section, WriteDialogFrame } from './common.tsx';

const FAMILIES = [
  'Figtree',
  'Fraunces',
  'Carlito',
  'Caladea',
  'Calibri',
  'Cambria',
  'Arial',
  'Georgia',
  'Times New Roman',
  'Courier New',
];
const SIZES = ['8', '9', '10', '10.5', '11', '12', '14', '16', '18', '20', '24', '28', '36', '48', '72'];
const UNDERLINES: readonly [string, string][] = [
  ['none', '(none)'],
  ['Single', 'Single'],
  ['Words', 'Words only'],
  ['Double', 'Double'],
  ['Thick', 'Thick'],
  ['Dotted', 'Dotted'],
  ['Dash', 'Dashed'],
  ['DotDash', 'Dot-dash'],
  ['Wave', 'Wave'],
];

export function FontDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const f = model.state.format;
  const [family, setFamily] = useState(f?.font ?? 'Carlito');
  const [size, setSize] = useState(String(f?.size ?? 11));
  // Radix Select treats '' as "no value": Regular is 'r'.
  const [style, setStyle] = useState(`${f?.bold ? 'b' : ''}${f?.italic ? 'i' : ''}` || 'r');
  const [underline, setUnderline] = useState<string>(f?.underline ?? 'none');
  const [color, setColor] = useState(f?.color ? `#${f.color}` : '#000000');
  const [autoColor, setAutoColor] = useState(!f?.color);
  const [fx, setFx] = useState({
    strike: f?.strike ?? false,
    double_strike: f?.double_strike ?? false,
    superscript: f?.superscript ?? false,
    subscript: f?.subscript ?? false,
    small_caps: f?.small_caps ?? false,
    all_caps: f?.all_caps ?? false,
    hidden: f?.hidden ?? false,
  });
  const [spacing, setSpacing] = useState(f?.spacing ?? 0);
  const [scale, setScale] = useState(100);
  const [position, setPosition] = useState(0);
  const [kern, setKern] = useState(false);
  // Word previews the selected text (its first line), or the font's name when nothing is selected.
  const [sample, setSample] = useState('');
  useEffect(() => {
    let live = true;
    model.client.query(model.doc, 'SelectedText').then(
      (r) => {
        if (!live || typeof r !== 'object' || r === null || !('Text' in r)) return;
        const line =
          String(r.Text)
            .split(/[\r\n\v\u2028\u2029]/, 1)[0]
            ?.trim() ?? '';
        setSample(Array.from(line).slice(0, 60).join(''));
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [model]);
  const sizeNum = Number(size);
  const valid = Number.isFinite(sizeNum) && sizeNum >= 1 && sizeNum <= 1638 && family.trim() !== '';

  const ok = async () => {
    const format: FontFormat = {
      family: family.trim(),
      size: sizeNum,
      bold: style.includes('b'),
      italic: style.includes('i'),
      underline: underline === 'none' ? null : (underline as UnderlineKind),
      underline_none: underline === 'none',
      underline_color: null,
      color: autoColor ? 'auto' : color.replace('#', '').toUpperCase(),
      strike: fx.strike,
      double_strike: fx.double_strike,
      superscript: fx.superscript,
      subscript: fx.subscript,
      small_caps: fx.small_caps,
      all_caps: fx.all_caps,
      hidden: fx.hidden,
      spacing,
      scale,
      position,
      kerning: kern ? sizeNum : 0,
    };
    return !(await model.run('write.format.font', { format }));
  };
  const flag = (k: keyof typeof fx) => (v: boolean) =>
    setFx((p) => ({
      ...p,
      [k]: v,
      ...(k === 'superscript' && v ? { subscript: false } : {}),
      ...(k === 'subscript' && v ? { superscript: false } : {}),
      ...(k === 'strike' && v ? { double_strike: false } : {}),
      ...(k === 'double_strike' && v ? { strike: false } : {}),
    }));
  const previewStyle = {
    fontFamily: `"${family}", var(--kiln-font-doc)`,
    // At its size on a 100 % page (points to CSS pixels), within what the box can show.
    fontSize: `${Math.min(48, Math.max(10, (sizeNum || 11) * (4 / 3)))}px`,
    fontWeight: style.includes('b') ? 700 : 400,
    fontStyle: style.includes('i') ? 'italic' : 'normal',
    textDecoration:
      `${underline !== 'none' ? 'underline' : ''} ${fx.strike || fx.double_strike ? 'line-through' : ''}`.trim() ||
      'none',
    textTransform: fx.all_caps ? ('uppercase' as const) : ('none' as const),
    fontVariant: fx.small_caps ? 'small-caps' : 'normal',
    // Automatic is the page's ink (dark on the light page, light on the dark page), like the
    // document itself; an explicit colour is shown as chosen.
    color: autoColor ? 'var(--kiln-page-ink)' : color,
    letterSpacing: `${spacing}pt`,
  };

  return (
    <WriteDialogFrame
      model={model}
      title="Font"
      size="lg"
      onOk={ok}
      okDisabled={!valid}
      testId="write-font-dialog"
    >
      <Tabs defaultValue={tab ?? 'font'}>
        <TabsList aria-label="Font settings">
          <TabsTrigger value="font">Font</TabsTrigger>
          <TabsTrigger value="advanced">Advanced</TabsTrigger>
        </TabsList>
        <TabsContent value="font">
          <Grid cols={3}>
            <Field label="Font">
              {() => (
                <Combobox
                  aria-label="Font"
                  value={family}
                  onValueChange={setFamily}
                  options={FAMILIES.map((v) => ({ value: v, fontFamily: v }))}
                  allowCustomValue
                />
              )}
            </Field>
            <Field label="Font style">
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={style}
                  onValueChange={setStyle}
                  options={[
                    { value: 'r', label: 'Regular' },
                    { value: 'i', label: 'Italic' },
                    { value: 'b', label: 'Bold' },
                    { value: 'bi', label: 'Bold Italic' },
                  ]}
                />
              )}
            </Field>
            <Field label="Size">
              {() => (
                <Combobox
                  aria-label="Size"
                  value={size}
                  onValueChange={setSize}
                  options={SIZES.map((v) => ({ value: v }))}
                  allowCustomValue
                />
              )}
            </Field>
            <Field label="Font color">
              {(id) => (
                <div className="kiln-write-inline">
                  <input
                    id={id}
                    type="color"
                    value={color}
                    disabled={autoColor}
                    onChange={(e) => setColor(e.target.value)}
                    aria-label="Font color"
                  />
                  <Check label="Automatic" checked={autoColor} onChange={setAutoColor} />
                </div>
              )}
            </Field>
            <Field label="Underline style">
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={underline}
                  onValueChange={setUnderline}
                  options={UNDERLINES.map(([value, label]) => ({ value, label }))}
                />
              )}
            </Field>
          </Grid>
          <Section title="Effects">
            <Grid cols={2}>
              <Check label="Strikethrough" checked={fx.strike} onChange={flag('strike')} />
              <Check label="Small caps" checked={fx.small_caps} onChange={flag('small_caps')} />
              <Check
                label="Double strikethrough"
                checked={fx.double_strike}
                onChange={flag('double_strike')}
              />
              <Check label="All caps" checked={fx.all_caps} onChange={flag('all_caps')} />
              <Check label="Superscript" checked={fx.superscript} onChange={flag('superscript')} />
              <Check label="Hidden" checked={fx.hidden} onChange={flag('hidden')} />
              <Check label="Subscript" checked={fx.subscript} onChange={flag('subscript')} />
            </Grid>
          </Section>
        </TabsContent>
        <TabsContent value="advanced">
          <Section title="Character spacing">
            <Grid cols={3}>
              <Field label="Scale">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={scale}
                    onValueChange={setScale}
                    unit="%"
                    min={1}
                    max={600}
                    step={10}
                  />
                )}
              </Field>
              <Field label="Spacing" hint="Negative is condensed">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={spacing}
                    onValueChange={setSpacing}
                    unit="pt"
                    min={-1584}
                    max={1584}
                    step={0.5}
                  />
                )}
              </Field>
              <Field label="Position" hint="Raised is positive">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={position}
                    onValueChange={setPosition}
                    unit="pt"
                    min={-1584}
                    max={1584}
                    step={0.5}
                  />
                )}
              </Field>
            </Grid>
            <Check label="Kerning for fonts of this size and above" checked={kern} onChange={setKern} />
          </Section>
        </TabsContent>
      </Tabs>
      <Section title="Preview">
        <div className="kiln-write-preview" data-testid="write-font-preview">
          <span style={previewStyle}>{sample || family || 'Font'}</span>
        </div>
      </Section>
    </WriteDialogFrame>
  );
}
