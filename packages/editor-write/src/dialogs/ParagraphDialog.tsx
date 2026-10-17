/** The Paragraph dialog (Home/Layout ▸ Paragraph ↘) and the Tabs dialog it opens. */
import type { ParagraphFormat, TabAlign, TabLeaderKind, TabStopSpec, TextAlign } from '@kiln/protocol';
import { Button, NumberInput, Select, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useState } from 'react';
import type { WriteEditorModel } from '../model.ts';
import { Check, cmOf, Field, Grid, LEN_UNIT, ptOf, Radio, Section, WriteDialogFrame } from './common.tsx';

type Special = 'none' | 'first' | 'hanging';
type LineRule = 'single' | '1.5' | 'double' | 'atLeast' | 'exactly' | 'multiple';

export const EMPTY_PARAGRAPH: ParagraphFormat = {
  alignment: null,
  indent_left: null,
  indent_right: null,
  first_line: null,
  space_before: null,
  space_after: null,
  line_multiple: null,
  line_exact: null,
  line_at_least: null,
  keep_next: null,
  keep_lines: null,
  widow_control: null,
  page_break_before: null,
  contextual_spacing: null,
  outline_level: null,
  rtl: null,
};

export function ParagraphDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const f = model.state.format;
  const [align, setAlign] = useState<TextAlign>(f?.alignment ?? 'Left');
  const [outline, setOutline] = useState(String(f?.outline_level ?? 0));
  const [left, setLeft] = useState(cmOf(f?.indent_left ?? 0));
  const [right, setRight] = useState(cmOf(f?.indent_right ?? 0));
  const fl = f?.first_line ?? 0;
  const [special, setSpecial] = useState<Special>(fl > 0 ? 'first' : fl < 0 ? 'hanging' : 'none');
  const [by, setBy] = useState(cmOf(Math.abs(fl)) || cmOf(36));
  const [before, setBefore] = useState(f?.space_before ?? 0);
  const [after, setAfter] = useState(f?.space_after ?? 0);
  const m = f?.line_multiple;
  const initialRule: LineRule =
    m === null || m === undefined
      ? 'exactly'
      : Math.abs(m - 1) < 0.01
        ? 'single'
        : Math.abs(m - 1.5) < 0.01
          ? '1.5'
          : Math.abs(m - 2) < 0.01
            ? 'double'
            : 'multiple';
  const [rule, setRule] = useState<LineRule>(initialRule);
  const [at, setAt] = useState(m ?? f?.line_points ?? 12);
  const [contextual, setContextual] = useState(f?.contextual_spacing ?? false);
  const [widow, setWidow] = useState(f?.widow_control ?? true);
  const [keepNext, setKeepNext] = useState(f?.keep_next ?? false);
  const [keepLines, setKeepLines] = useState(f?.keep_lines ?? false);
  const [pageBreak, setPageBreak] = useState(f?.page_break_before ?? false);

  const ok = async () => {
    const lineMultiple =
      rule === 'single' ? 1 : rule === '1.5' ? 1.5 : rule === 'double' ? 2 : rule === 'multiple' ? at : null;
    const format: ParagraphFormat = {
      ...EMPTY_PARAGRAPH,
      alignment: align,
      outline_level: Number(outline),
      indent_left: ptOf(left),
      indent_right: ptOf(right),
      first_line: special === 'none' ? 0 : special === 'first' ? ptOf(by) : -ptOf(by),
      space_before: before,
      space_after: after,
      line_multiple: lineMultiple,
      line_exact: rule === 'exactly' ? at : null,
      line_at_least: rule === 'atLeast' ? at : null,
      contextual_spacing: contextual,
      widow_control: widow,
      keep_next: keepNext,
      keep_lines: keepLines,
      page_break_before: pageBreak,
    };
    return !(await model.run('write.format.paragraph', { format }));
  };

  return (
    <WriteDialogFrame
      model={model}
      title="Paragraph"
      size="lg"
      onOk={ok}
      testId="write-paragraph-dialog"
      footerStart={
        <Button
          onClick={() => {
            model.openDialog('tabs');
          }}
        >
          Tabs…
        </Button>
      }
    >
      <Tabs defaultValue={tab ?? 'indents'}>
        <TabsList aria-label="Paragraph settings">
          <TabsTrigger value="indents">Indents and Spacing</TabsTrigger>
          <TabsTrigger value="breaks">Line and Page Breaks</TabsTrigger>
        </TabsList>
        <TabsContent value="indents">
          <Section title="General">
            <Grid cols={2}>
              <Field label="Alignment">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={align}
                    onValueChange={(v) => setAlign(v as TextAlign)}
                    options={[
                      { value: 'Left', label: 'Left' },
                      { value: 'Center', label: 'Centered' },
                      { value: 'Right', label: 'Right' },
                      { value: 'Justify', label: 'Justified' },
                    ]}
                  />
                )}
              </Field>
              <Field label="Outline level">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={outline}
                    onValueChange={setOutline}
                    options={[
                      { value: '0', label: 'Body Text' },
                      ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => ({ value: String(n), label: `Level ${n}` })),
                    ]}
                  />
                )}
              </Field>
            </Grid>
          </Section>
          <Section title="Indentation">
            <Grid cols={4}>
              <Field label="Left">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={left}
                    onValueChange={setLeft}
                    unit={LEN_UNIT}
                    min={-55}
                    max={55}
                    step={0.1}
                  />
                )}
              </Field>
              <Field label="Right">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={right}
                    onValueChange={setRight}
                    unit={LEN_UNIT}
                    min={-55}
                    max={55}
                    step={0.1}
                  />
                )}
              </Field>
              <Field label="Special">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={special}
                    onValueChange={(v) => setSpecial(v as Special)}
                    options={[
                      { value: 'none', label: '(none)' },
                      { value: 'first', label: 'First line' },
                      { value: 'hanging', label: 'Hanging' },
                    ]}
                  />
                )}
              </Field>
              <Field label="By">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={by}
                    onValueChange={setBy}
                    unit={LEN_UNIT}
                    min={0}
                    max={55}
                    step={0.1}
                    disabled={special === 'none'}
                  />
                )}
              </Field>
            </Grid>
          </Section>
          <Section title="Spacing">
            <Grid cols={4}>
              <Field label="Before">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={before}
                    onValueChange={setBefore}
                    unit="pt"
                    min={0}
                    max={1584}
                    step={6}
                  />
                )}
              </Field>
              <Field label="After">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={after}
                    onValueChange={setAfter}
                    unit="pt"
                    min={0}
                    max={1584}
                    step={6}
                  />
                )}
              </Field>
              <Field label="Line spacing">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={rule}
                    onValueChange={(v) => {
                      setRule(v as LineRule);
                      if (v === 'atLeast' || v === 'exactly') setAt(12);
                      if (v === 'multiple') setAt(1.15);
                    }}
                    options={[
                      { value: 'single', label: 'Single' },
                      { value: '1.5', label: '1.5 lines' },
                      { value: 'double', label: 'Double' },
                      { value: 'atLeast', label: 'At least' },
                      { value: 'exactly', label: 'Exactly' },
                      { value: 'multiple', label: 'Multiple' },
                    ]}
                  />
                )}
              </Field>
              <Field label="At">
                {(id) => (
                  <NumberInput
                    id={id}
                    value={at}
                    onValueChange={setAt}
                    unit={rule === 'multiple' ? '' : 'pt'}
                    min={rule === 'multiple' ? 0.06 : 1}
                    max={rule === 'multiple' ? 132 : 1584}
                    step={rule === 'multiple' ? 0.05 : 1}
                    disabled={rule === 'single' || rule === '1.5' || rule === 'double'}
                  />
                )}
              </Field>
            </Grid>
            <Check
              label="Don't add space between paragraphs of the same style"
              checked={contextual}
              onChange={setContextual}
            />
          </Section>
        </TabsContent>
        <TabsContent value="breaks">
          <Section title="Pagination">
            <Check label="Widow/Orphan control" checked={widow} onChange={setWidow} />
            <Check label="Keep with next" checked={keepNext} onChange={setKeepNext} />
            <Check label="Keep lines together" checked={keepLines} onChange={setKeepLines} />
            <Check label="Page break before" checked={pageBreak} onChange={setPageBreak} />
          </Section>
        </TabsContent>
      </Tabs>
    </WriteDialogFrame>
  );
}

const ALIGNS: readonly (readonly [TabAlign, string])[] = [
  ['Left', 'Left'],
  ['Center', 'Center'],
  ['Right', 'Right'],
  ['Decimal', 'Decimal'],
  ['Bar', 'Bar'],
];
const LEADERS: readonly (readonly [TabLeaderKind, string])[] = [
  ['None', 'None'],
  ['Dot', '.......'],
  ['Hyphen', '-------'],
  ['Underscore', '_______'],
];

/** The Tabs dialog: set, clear and clear all tab stops of the selected paragraphs. */
export function TabsDialog({ model }: { model: WriteEditorModel }) {
  const [stops, setStops] = useState<TabStopSpec[]>(() => [...(model.state.format?.tabs ?? [])]);
  const [pos, setPos] = useState(cmOf(stops[0]?.pos ?? 36) || cmOf(36));
  const [align, setAlign] = useState<TabAlign>('Left');
  const [leader, setLeader] = useState<TabLeaderKind>('None');
  const [selected, setSelected] = useState<number | null>(null);
  const set = () => {
    const p = ptOf(pos);
    setStops((s) =>
      [...s.filter((t) => Math.abs(t.pos - p) > 0.5), { pos: p, align, leader }].sort(
        (a, b) => a.pos - b.pos,
      ),
    );
  };
  return (
    <WriteDialogFrame
      model={model}
      title="Tabs"
      onOk={async () => !(await model.run('write.format.tabs', { tabs: stops }))}
      testId="write-tabs-dialog"
    >
      <Grid cols={2}>
        <div>
          <Field label="Tab stop position">
            {(id) => (
              <NumberInput
                id={id}
                value={pos}
                onValueChange={setPos}
                unit={LEN_UNIT}
                min={0}
                max={55}
                step={0.25}
              />
            )}
          </Field>
          <ul className="kiln-write-tabs-list" aria-label="Tab stops">
            {stops.map((t, i) => (
              <li key={t.pos}>
                <button
                  type="button"
                  aria-pressed={selected === i}
                  onClick={() => {
                    setSelected(i);
                    setPos(cmOf(t.pos));
                    setAlign(t.align);
                    setLeader(t.leader);
                  }}
                >
                  {cmOf(t.pos).toLocaleString()} {LEN_UNIT} · {t.align}
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div>
          <Radio name="tab-align" label="Alignment" value={align} options={ALIGNS} onChange={setAlign} />
          <Radio name="tab-leader" label="Leader" value={leader} options={LEADERS} onChange={setLeader} />
        </div>
      </Grid>
      <div className="kiln-write-inline">
        <Button onClick={set}>Set</Button>
        <Button
          disabled={selected === null}
          onClick={() => {
            setStops((s) => s.filter((_, i) => i !== selected));
            setSelected(null);
          }}
        >
          Clear
        </Button>
        <Button onClick={() => setStops([])}>Clear All</Button>
      </div>
    </WriteDialogFrame>
  );
}
