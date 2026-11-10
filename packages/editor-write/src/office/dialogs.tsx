/**
 * Kiln Write's dialogs for the rest of Word's ribbon: Zoom, Table Properties, Sort Text,
 * Set Proofing Language and Picture Corrections. Presentation only: each reads the core's answer
 * and runs one core command with the fields as they are.
 */
import { languageName, msg, t, tx } from '@kiln/i18n';
import type {
  CellVerticalAlign,
  PreferredWidth,
  SortKind,
  TablePropertiesSpec,
  TextAlign,
} from '@kiln/protocol';
import { NumberInput, Select, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import { fitZoom } from '../commands.ts';
import {
  Check,
  cmOf,
  Field,
  Grid,
  LEN_UNIT,
  ptOf,
  Radio,
  Section,
  unitName,
  WriteDialogFrame,
} from '../dialogs/common.tsx';
import type { WriteEditorModel } from '../model.ts';

// ── Zoom ─────────────────────────────────────────────────────────────────────────────────────

type ZoomPreset = '200' | '100' | '75' | 'page_width' | 'one_page' | 'multiple_pages' | 'custom';

/** View ▸ Zoom: Word's presets, page width, whole page, many pages, or any percentage 10–500. */
export function ZoomDialog({ model }: { model: WriteEditorModel }) {
  const current = Math.round((model.state.status?.zoom ?? 1) * 100);
  const [preset, setPreset] = useState<ZoomPreset>(
    current === 200 ? '200' : current === 100 ? '100' : current === 75 ? '75' : 'custom',
  );
  const [percent, setPercent] = useState(current);
  const choose = (p: ZoomPreset) => {
    setPreset(p);
    if (p === '200' || p === '100' || p === '75') setPercent(Number(p));
  };
  const ok = async () => {
    if (preset === 'page_width' || preset === 'one_page' || preset === 'multiple_pages') {
      const r = await model.query('Pages');
      const view = model.state.view;
      const page = r && 'Pages' in r ? (r.Pages.rects[r.Pages.current] ?? r.Pages.rects[0]) : undefined;
      if (!page || !view) return;
      return !(await model.run('suite.view.zoom', { zoom: fitZoom(preset, page, view) }));
    }
    const z = Math.min(500, Math.max(10, Math.round(percent))) / 100;
    return !(await model.run('suite.view.zoom', { zoom: z }));
  };
  return (
    <WriteDialogFrame model={model} title={t('Zoom')} size="sm" onOk={ok} testId="write-zoom-dialog">
      <Radio
        name="zoom-to"
        label={t('Zoom to')}
        value={preset}
        onChange={choose}
        options={[
          ['200', '200%'],
          ['100', '100%'],
          ['75', '75%'],
          ['page_width', t('Page width')],
          ['one_page', t('Whole page')],
          ['multiple_pages', t('Many pages')],
          ['custom', t('Percent')],
        ]}
      />
      <Field label={t('Percent')} hint={t('10% to 500%')}>
        {(id) => (
          <NumberInput
            id={id}
            value={percent}
            onValueChange={(v) => {
              setPercent(v);
              setPreset('custom');
            }}
            unit="%"
            min={10}
            max={500}
            step={10}
            precision={0}
          />
        )}
      </Field>
    </WriteDialogFrame>
  );
}

// ── Table Properties ─────────────────────────────────────────────────────────────────────────

const EMPTY_TABLE: TablePropertiesSpec = {
  width: null,
  alignment: null,
  indent: null,
  wrap_around: null,
  autofit: null,
  cell_margins: null,
  cell_spacing: null,
  row_height: null,
  row_can_break: null,
  row_header: null,
  column_width: null,
  cell_width: null,
  cell_v_align: null,
  cell_wrap: null,
  alt_title: null,
  alt_description: null,
};

type WidthUnit = 'len' | 'pct';

/** A preferred width: on/off, value, points or percent. */
function WidthField({
  label,
  value,
  onChange,
  percentOk = true,
}: {
  label: string;
  value: PreferredWidth;
  onChange: (w: PreferredWidth) => void;
  percentOk?: boolean;
}) {
  const on = value.value > 0;
  const unit: WidthUnit = value.percent ? 'pct' : 'len';
  return (
    <Grid cols={3}>
      <Check
        label={label}
        checked={on}
        onChange={(v) =>
          onChange(v ? { value: value.percent ? 100 : 288, percent: value.percent } : { ...value, value: 0 })
        }
      />
      <Field label={t('Width')}>
        {(id) => (
          <NumberInput
            id={id}
            value={unit === 'pct' ? value.value : cmOf(value.value)}
            onValueChange={(v) => onChange({ value: unit === 'pct' ? v : ptOf(v), percent: unit === 'pct' })}
            unit={unit === 'pct' ? '%' : LEN_UNIT}
            min={0}
            max={unit === 'pct' ? 600 : 55}
            step={unit === 'pct' ? 5 : 0.1}
            disabled={!on}
          />
        )}
      </Field>
      <Field label={t('Measure in')}>
        {(id) => (
          <Select
            labelId={`${id}-label`}
            value={unit}
            disabled={!on || !percentOk}
            onValueChange={(u) => onChange({ value: u === 'pct' ? 100 : 288, percent: u === 'pct' })}
            options={[
              { value: 'len', label: unitName(LEN_UNIT) },
              { value: 'pct', label: t('Percent') },
            ]}
          />
        )}
      </Field>
    </Grid>
  );
}

/** Table Layout ▸ Properties: Table, Row, Column, Cell and Alt Text, as in Word. */
export function TablePropertiesDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const [p, setP] = useState<TablePropertiesSpec | null>(null);
  const [changed, setChanged] = useState<Partial<TablePropertiesSpec>>({});
  useEffect(() => {
    void model.query('TableProperties').then((r) => {
      if (r && 'TableProperties' in r && r.TableProperties) setP(r.TableProperties);
    });
  }, [model]);
  if (!p) {
    return (
      <WriteDialogFrame model={model} title={t('Table Properties')} testId="write-table-properties-dialog">
        <p className="kiln-write-status">{t('Place the cursor in a table first.')}</p>
      </WriteDialogFrame>
    );
  }
  const v = { ...p, ...changed };
  const set = (patch: Partial<TablePropertiesSpec>) => setChanged((c) => ({ ...c, ...patch }));
  const zero: PreferredWidth = { value: 0, percent: false };
  const rowH = v.row_height ?? { points: 0, exact: false };
  const margins = v.cell_margins ?? { top: 0, left: 5.4, bottom: 0, right: 5.4 };
  return (
    <WriteDialogFrame
      model={model}
      title={t('Table Properties')}
      size="lg"
      testId="write-table-properties-dialog"
      onOk={async () =>
        Object.keys(changed).length === 0
          ? false
          : !(await model.run('write.table.properties', { spec: { ...EMPTY_TABLE, ...changed } }))
      }
    >
      <Tabs defaultValue={tab ?? 'table'}>
        <TabsList aria-label={t('Table properties')}>
          <TabsTrigger value="table">{t('Table')}</TabsTrigger>
          <TabsTrigger value="row">{t('Row')}</TabsTrigger>
          <TabsTrigger value="column">{t('Column')}</TabsTrigger>
          <TabsTrigger value="cell">{t('Cell')}</TabsTrigger>
          <TabsTrigger value="alt">{t('Alt Text')}</TabsTrigger>
        </TabsList>
        <TabsContent value="table">
          <Section title={t('Size')}>
            <WidthField
              label={t('Preferred width')}
              value={v.width ?? zero}
              onChange={(width) => set({ width })}
            />
          </Section>
          <Section title={t('Alignment')}>
            <Grid cols={2}>
              <Radio
                name="table-align"
                label={t('Alignment')}
                value={v.alignment === 'Justify' ? 'Left' : (v.alignment ?? 'Left')}
                onChange={(alignment: TextAlign) => set({ alignment })}
                options={[
                  ['Left', t('Left')],
                  ['Center', t('Center')],
                  ['Right', t('Right')],
                ]}
              />
              <Field label={t('Indent from left')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    value={cmOf(v.indent ?? 0)}
                    onValueChange={(x) => set({ indent: ptOf(x) })}
                    unit={LEN_UNIT}
                    min={-20}
                    max={55}
                    step={0.1}
                    disabled={v.alignment !== 'Left' && v.alignment !== null}
                  />
                )}
              </Field>
            </Grid>
          </Section>
          <Section title={t('Text wrapping')}>
            <Radio
              name="table-wrap"
              label={t('Text wrapping')}
              value={v.wrap_around ? 'around' : 'none'}
              onChange={(w) => set({ wrap_around: w === 'around' })}
              options={[
                ['none', t('None')],
                ['around', t('Around')],
              ]}
            />
          </Section>
          <Section title={t('Options')}>
            <Grid cols={2}>
              <Check
                label={t('Automatically resize to fit contents')}
                checked={v.autofit ?? true}
                onChange={(autofit) => set({ autofit })}
              />
              <Field label={t('Spacing between cells')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    value={cmOf(v.cell_spacing ?? 0)}
                    onValueChange={(x) => set({ cell_spacing: ptOf(x) })}
                    unit={LEN_UNIT}
                    min={0}
                    max={5}
                    step={0.05}
                  />
                )}
              </Field>
            </Grid>
            <Grid cols={4}>
              {(['top', 'left', 'bottom', 'right'] as const).map((side) => (
                <Field key={side} label={t('Cell margin {side}', { side })}>
                  {(id) => (
                    <NumberInput
                      id={id}
                      value={cmOf(margins[side])}
                      onValueChange={(x) => set({ cell_margins: { ...margins, [side]: ptOf(x) } })}
                      unit={LEN_UNIT}
                      min={0}
                      max={5}
                      step={0.05}
                    />
                  )}
                </Field>
              ))}
            </Grid>
          </Section>
        </TabsContent>
        <TabsContent value="row">
          <Section title={t('Size')}>
            <Grid cols={3}>
              <Check
                label={t('Specify height')}
                checked={rowH.points > 0}
                onChange={(on) => set({ row_height: { points: on ? 18 : 0, exact: rowH.exact } })}
              />
              <Field label={t('Height')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    value={cmOf(rowH.points)}
                    onValueChange={(x) => set({ row_height: { ...rowH, points: ptOf(x) } })}
                    unit={LEN_UNIT}
                    min={0}
                    max={55}
                    step={0.1}
                    disabled={rowH.points <= 0}
                  />
                )}
              </Field>
              <Field label={t('Row height is')}>
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={rowH.exact ? 'exact' : 'atLeast'}
                    disabled={rowH.points <= 0}
                    onValueChange={(r) => set({ row_height: { ...rowH, exact: r === 'exact' } })}
                    options={[
                      { value: 'atLeast', label: t('At least') },
                      { value: 'exact', label: t('Exactly') },
                    ]}
                  />
                )}
              </Field>
            </Grid>
          </Section>
          <Section title={t('Options')}>
            <Check
              label={t('Allow row to break across pages')}
              checked={v.row_can_break ?? true}
              onChange={(row_can_break) => set({ row_can_break })}
            />
            <Check
              label={t('Repeat as header row at the top of each page')}
              checked={v.row_header ?? false}
              onChange={(row_header) => set({ row_header })}
            />
          </Section>
        </TabsContent>
        <TabsContent value="column">
          <Section title={t('Size')}>
            <WidthField
              label={t('Preferred width')}
              value={v.column_width ?? zero}
              onChange={(column_width) => set({ column_width })}
            />
          </Section>
        </TabsContent>
        <TabsContent value="cell">
          <Section title={t('Size')}>
            <WidthField
              label={t('Preferred width')}
              value={v.cell_width ?? zero}
              onChange={(cell_width) => set({ cell_width })}
            />
          </Section>
          <Section title={t('Vertical alignment')}>
            <Radio
              name="cell-valign"
              label={t('Vertical alignment')}
              value={v.cell_v_align ?? 'Top'}
              onChange={(cell_v_align: CellVerticalAlign) => set({ cell_v_align })}
              options={[
                ['Top', t('Top')],
                ['Center', t('Center')],
                ['Bottom', t('Bottom')],
              ]}
            />
          </Section>
          <Section title={t('Options')}>
            <Check
              label={t('Wrap text')}
              checked={v.cell_wrap ?? true}
              onChange={(cell_wrap) => set({ cell_wrap })}
            />
          </Section>
        </TabsContent>
        <TabsContent value="alt">
          <Field label={t('Title')}>
            {(id) => (
              <input
                id={id}
                className="kiln-write-input"
                value={v.alt_title ?? ''}
                onChange={(e) => set({ alt_title: e.target.value })}
              />
            )}
          </Field>
          <Field label={t('Description')} hint={t('What the table shows, for people using a screen reader')}>
            {(id) => (
              <textarea
                id={id}
                className="kiln-write-input kiln-write-textarea"
                rows={4}
                value={v.alt_description ?? ''}
                onChange={(e) => set({ alt_description: e.target.value })}
              />
            )}
          </Field>
        </TabsContent>
      </Tabs>
    </WriteDialogFrame>
  );
}

// ── Sort ─────────────────────────────────────────────────────────────────────────────────────

/** Home ▸ Sort: paragraphs, or the rows of the table at the cursor by a column. */
export function SortDialog({ model }: { model: WriteEditorModel }) {
  const table = model.state.format?.table ?? null;
  const [column, setColumn] = useState(0);
  const [kind, setKind] = useState<SortKind>('Text');
  const [order, setOrder] = useState<'asc' | 'desc'>('asc');
  const [header, setHeader] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const cols = table?.cols ?? 0;
  return (
    <WriteDialogFrame
      model={model}
      title={t('Sort Text')}
      size="md"
      testId="write-sort-dialog"
      onOk={async () =>
        !(await model.run('write.paragraph.sort_by', {
          kind,
          descending: order === 'desc',
          column,
          header,
          case_sensitive: caseSensitive,
        }))
      }
    >
      <Grid cols={3}>
        <Field label={t('Sort by')}>
          {(id) =>
            table ? (
              <Select
                labelId={`${id}-label`}
                value={String(column)}
                onValueChange={(c) => setColumn(Number(c))}
                options={Array.from({ length: Math.max(1, cols) }, (_, i) => ({
                  value: String(i),
                  label: t('Column {value}', { value: i + 1 }),
                }))}
              />
            ) : (
              <input id={id} className="kiln-write-input" value="Paragraphs" readOnly />
            )
          }
        </Field>
        <Field label={t('Type')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={kind}
              onValueChange={(k) => setKind(k as SortKind)}
              options={[
                { value: 'Text', label: t('Text') },
                { value: 'Number', label: t('Number') },
                { value: 'Date', label: t('Date') },
              ]}
            />
          )}
        </Field>
        <Radio
          name="sort-order"
          label={t('Order')}
          value={order}
          onChange={setOrder}
          options={[
            ['asc', t('Ascending')],
            ['desc', t('Descending')],
          ]}
        />
      </Grid>
      <Section title={t('My list has')}>
        <Radio
          name="sort-header"
          label={t('My list has')}
          value={header ? 'header' : 'none'}
          onChange={(h) => setHeader(h === 'header')}
          options={[
            ['header', t('Header row')],
            ['none', t('No header row')],
          ]}
        />
        <Check label={t('Case sensitive')} checked={caseSensitive} onChange={setCaseSensitive} />
      </Section>
    </WriteDialogFrame>
  );
}

// ── Language ─────────────────────────────────────────────────────────────────────────────────

/** Proofing languages, named in the interface language. */
const LANGUAGES: readonly string[] = [
  'en-US',
  'en-GB',
  'de-DE',
  'fr-FR',
  'es-ES',
  'it-IT',
  'nl-NL',
  'pt-BR',
  'pl-PL',
  'sv-SE',
  'ar-SA',
  'he-IL',
  'ja-JP',
  'zh-CN',
];

/** Review ▸ Language ▸ Set Proofing Language. */
export function LanguageDialog({ model }: { model: WriteEditorModel }) {
  const [lang, setLang] = useState('en-US');
  const [noProof, setNoProof] = useState(false);
  return (
    <WriteDialogFrame
      model={model}
      title={t('Language')}
      size="sm"
      testId="write-language-dialog"
      onOk={async () => !(await model.run('write.review.set_language', { lang, no_proof: noProof }))}
    >
      <Field
        label={t('Mark selected text as')}
        hint={t('Kiln checks spelling in English (United States); text in other languages is left unchecked')}
      >
        {(id) => (
          <Select
            labelId={`${id}-label`}
            value={lang}
            onValueChange={setLang}
            options={LANGUAGES.map((value) => ({ value, label: languageName(value) }))}
          />
        )}
      </Field>
      <Check label={t('Do not check spelling or grammar')} checked={noProof} onChange={setNoProof} />
    </WriteDialogFrame>
  );
}

// ── Picture Corrections ──────────────────────────────────────────────────────────────────────

/** Picture Format ▸ Corrections ▸ Picture Corrections Options. */
export function CorrectionsDialog({ model }: { model: WriteEditorModel }) {
  const [brightness, setBrightness] = useState(0);
  const [contrast, setContrast] = useState(0);
  return (
    <WriteDialogFrame
      model={model}
      title={t('Picture Corrections')}
      size="sm"
      testId="write-corrections-dialog"
      onOk={async () =>
        !(await model.run('write.picture.corrections', {
          brightness: Math.round(brightness),
          contrast: Math.round(contrast),
        }))
      }
    >
      <Grid cols={2}>
        <Field label={t('Brightness')}>
          {(id) => (
            <NumberInput
              id={id}
              value={brightness}
              onValueChange={setBrightness}
              unit="%"
              min={-100}
              max={100}
              step={5}
              precision={0}
            />
          )}
        </Field>
        <Field label={t('Contrast')}>
          {(id) => (
            <NumberInput
              id={id}
              value={contrast}
              onValueChange={setContrast}
              unit="%"
              min={-100}
              max={100}
              step={5}
              precision={0}
            />
          )}
        </Field>
      </Grid>
    </WriteDialogFrame>
  );
}

// ── Equation ─────────────────────────────────────────────────────────────────────────────────

const EQUATION_EXAMPLES: readonly [string, string][] = [
  ['a^2+b^2=c^2', msg('Pythagoras')],
  ['x=(-b\\pm\\sqrt(b^2-4ac))/(2a)', msg('Quadratic formula')],
  ['\\sum_(i=1)^n i=(n(n+1))/2', msg('Sum')],
  ['e^(i\\pi)+1=0', msg('Euler')],
];

/** Insert ▸ Equation: the linear form Word's equation editor accepts, saved as Office Math. */
export function EquationDialog({ model }: { model: WriteEditorModel }) {
  const [linear, setLinear] = useState('');
  return (
    <WriteDialogFrame
      model={model}
      title={t('Insert Equation')}
      size="md"
      okLabel={t('Insert')}
      okDisabled={!linear.trim()}
      testId="write-equation-dialog"
      onOk={async () => !(await model.run('write.insert.equation', { linear }))}
    >
      <Field
        label={t('Equation')}
        hint={t(
          '^ superscript, _ subscript, a/b fraction, \\sqrt(x), \\sum, \\int, \\alpha … as in Word’s linear format',
        )}
      >
        {(id) => (
          <input
            id={id}
            className="kiln-write-input kiln-write-math"
            value={linear}
            spellCheck={false}
            // biome-ignore lint/a11y/noAutofocus: the equation box takes focus, as Word's does
            autoFocus
            placeholder={t('Type equation here.')}
            onChange={(e) => setLinear(e.target.value)}
          />
        )}
      </Field>
      <Section title={t('Examples')}>
        <div className="kiln-write-examples">
          {EQUATION_EXAMPLES.map(([eq, name]) => (
            <button key={eq} type="button" className="kiln-write-example" onClick={() => setLinear(eq)}>
              <span>{tx(name)}</span>
              <code>{eq}</code>
            </button>
          ))}
        </div>
      </Section>
    </WriteDialogFrame>
  );
}
