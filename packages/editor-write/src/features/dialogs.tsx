/**
 * The dialogs of Kiln Write's features (WP-66): Table of Contents (custom), Insert Caption,
 * Cross-reference, Footnote and Endnote, Layout (More Layout Options: position, wrapping, size,
 * rotation, alt text), document Properties and the word-count goal. Each sends one core command
 * built from its fields; the core validates and answers with a message when it disagrees.
 */
import type {
  CaptionPosition,
  CrossRefItem,
  CrossRefShow,
  DocPropertiesInfo,
  ObjectLayoutSpec,
  RelativeTo,
  TabLeaderKind,
  WrapMode,
} from '@kiln/protocol';
import {
  Button,
  Dialog,
  DialogContent,
  NumberInput,
  Select,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@kiln/ui-kit';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import { useStore } from 'zustand';
import { Check, Field, Grid, Radio, Section } from '../dialogs/common.tsx';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';
import { featuresOf } from './model.ts';

function Frame({
  model,
  title,
  onOk,
  okLabel = 'OK',
  okDisabled,
  children,
  testId,
  size = 'md',
}: {
  model: WriteEditorModel;
  title: string;
  onOk: () => Promise<boolean> | boolean;
  okLabel?: string;
  okDisabled?: boolean;
  children: ReactNode;
  testId: string;
  size?: 'sm' | 'md' | 'lg';
}) {
  const features = featuresOf(model);
  const notice = useWrite(model, (s) => s.notice);
  const close = () => {
    features.closeDialog();
    // Back to the page, as in Word: typing goes on where the dialog was opened.
    setTimeout(() => model.controller?.focus(), 0);
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={title}
        size={size}
        data-testid={testId}
        footer={
          <>
            <Button onClick={close}>Cancel</Button>
            <Button
              variant="primary"
              disabled={okDisabled}
              onClick={async () => {
                model.set({ notice: null });
                if (await onOk()) close();
              }}
            >
              {okLabel}
            </Button>
          </>
        }
      >
        <div className="kiln-write-dialog">
          {children}
          {notice ? (
            <p className="kiln-write-dialog__error" role="alert">
              {notice}
            </p>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}

const LEADERS: { value: TabLeaderKind; label: string }[] = [
  { value: 'Dot', label: '. . . . . . .' },
  { value: 'Hyphen', label: '- - - - - - -' },
  { value: 'Underscore', label: '_______' },
  { value: 'None', label: '(none)' },
];

export function TocDialog({ model }: { model: WriteEditorModel }) {
  const [levels, setLevels] = useState(3);
  const [pages, setPages] = useState(true);
  const [right, setRight] = useState(true);
  const [links, setLinks] = useState(true);
  const [leader, setLeader] = useState<TabLeaderKind>('Dot');
  const [title, setTitle] = useState('Contents');
  const preview = useMemo(
    () =>
      [
        'Heading 1',
        'Heading 2',
        'Heading 3',
        'Heading 4',
        'Heading 5',
        'Heading 6',
        'Heading 7',
        'Heading 8',
        'Heading 9',
      ]
        .slice(0, levels)
        .map((h, i) => ({ h, i, page: 1 + i * 2 })),
    [levels],
  );
  const dots = { Dot: '.', Hyphen: '-', Underscore: '_', None: ' ' }[leader];
  return (
    <Frame
      model={model}
      title="Table of Contents"
      testId="write-toc-dialog"
      size="lg"
      onOk={() =>
        model.run('write.references.table_of_contents', {
          spec: {
            levels,
            hyperlinks: links,
            page_numbers: pages,
            right_align: right,
            leader,
            title: title.trim() || null,
          },
        })
      }
    >
      <div className="kiln-toc-dialog">
        <div className="kiln-toc-dialog__preview" aria-label="Print preview" role="img">
          {title.trim() ? <div className="kiln-toc-dialog__title">{title}</div> : null}
          {preview.map(({ h, i, page }) => (
            <div key={h} className="kiln-toc-dialog__row" style={{ paddingLeft: `${i * 14}px` }}>
              <span className={links ? 'kiln-toc-dialog__link' : undefined}>{h}</span>
              {pages ? (
                right ? (
                  <>
                    <span className="kiln-toc-dialog__leader">{dots.repeat(40)}</span>
                    <span>{page}</span>
                  </>
                ) : (
                  <span>&nbsp;{page}</span>
                )
              ) : null}
            </div>
          ))}
        </div>
        <div className="kiln-toc-dialog__fields">
          <Field label="Title">
            {(id) => (
              <input
                id={id}
                className="kiln-write-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            )}
          </Field>
          <Check label="Show page numbers" checked={pages} onChange={setPages} />
          <Check label="Right align page numbers" checked={right} onChange={setRight} />
          <Field label="Tab leader">
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={leader}
                onValueChange={(v) => setLeader(v as TabLeaderKind)}
                options={LEADERS}
                disabled={!pages || !right}
              />
            )}
          </Field>
          <Check
            label="Use hyperlinks instead of page numbers in Web Layout"
            checked={links}
            onChange={setLinks}
          />
          <Field label="Show levels">
            {(id) => (
              <NumberInput
                id={id}
                aria-label="Show levels"
                value={levels}
                onValueChange={(v) => setLevels(Math.max(1, Math.min(9, Math.round(v))))}
                min={1}
                max={9}
                precision={0}
                width={80}
              />
            )}
          </Field>
        </div>
      </div>
    </Frame>
  );
}

export function CaptionDialog({ model }: { model: WriteEditorModel }) {
  const object = useWrite(model, (s) => s.format?.features.object ?? null);
  const inTable = useWrite(model, (s) => !!s.format?.table);
  const [label, setLabel] = useState(inTable ? 'Table' : object ? 'Figure' : 'Figure');
  const [text, setText] = useState('');
  const [position, setPosition] = useState<CaptionPosition>(inTable ? 'Above' : 'Below');
  const [exclude, setExclude] = useState(false);
  const [chapter, setChapter] = useState(false);
  const [custom, setCustom] = useState<string[]>([]);
  const [newLabel, setNewLabel] = useState<string | null>(null);
  const labels = ['Figure', 'Table', 'Equation', ...custom];
  return (
    <Frame
      model={model}
      title="Caption"
      testId="write-caption-dialog"
      onOk={() =>
        model.run('write.references.insert_caption', {
          spec: {
            label,
            text: text ? (text.startsWith(':') || text.startsWith(' ') ? text : `: ${text}`) : '',
            position,
            exclude_label: exclude,
            chapter_level: chapter ? 1 : null,
          },
        })
      }
    >
      <p className="kiln-caption-dialog__preview" aria-live="polite">
        {exclude ? '' : `${label} `}
        {chapter ? '1-' : ''}1
        {text ? (text.startsWith(':') || text.startsWith(' ') ? text : `: ${text}`) : ''}
      </p>
      <Field label="Caption text (after the number)">
        {(id) => (
          <input
            id={id}
            className="kiln-write-input"
            value={text}
            placeholder="Glaze test tiles"
            // biome-ignore lint/a11y/noAutofocus: the caption text is what the dialog is for
            autoFocus
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
      <Grid>
        <Field label="Label">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={label}
              onValueChange={setLabel}
              options={labels.map((l) => ({ value: l, label: l }))}
            />
          )}
        </Field>
        <Field label="Position">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={position}
              onValueChange={(v) => setPosition(v as CaptionPosition)}
              options={[
                { value: 'Below', label: 'Below selected item' },
                { value: 'Above', label: 'Above selected item' },
              ]}
            />
          )}
        </Field>
      </Grid>
      <Check label="Exclude label from caption" checked={exclude} onChange={setExclude} />
      <Check label="Include chapter number (Heading 1)" checked={chapter} onChange={setChapter} />
      {newLabel === null ? (
        <Button size="sm" onClick={() => setNewLabel('')}>
          New Label…
        </Button>
      ) : (
        <div className="kiln-write-row">
          <input
            className="kiln-write-input"
            aria-label="New label"
            placeholder="Label (e.g. Chart)"
            value={newLabel}
            // biome-ignore lint/a11y/noAutofocus: the field the button just opened
            autoFocus
            onChange={(e) => setNewLabel(e.target.value)}
          />
          <Button
            size="sm"
            disabled={!newLabel.trim() || labels.includes(newLabel.trim())}
            onClick={() => {
              const l = newLabel.trim();
              setCustom([...custom, l]);
              setLabel(l);
              setNewLabel(null);
            }}
          >
            Add
          </Button>
        </div>
      )}
    </Frame>
  );
}

const SHOW: Record<string, { value: CrossRefShow; label: string }[]> = {
  Heading: [
    { value: 'Text', label: 'Heading text' },
    { value: 'PageNumber', label: 'Page number' },
    { value: 'Number', label: 'Heading number' },
    { value: 'AboveBelow', label: 'Above/below' },
  ],
  Bookmark: [
    { value: 'Text', label: 'Bookmark text' },
    { value: 'PageNumber', label: 'Page number' },
    { value: 'AboveBelow', label: 'Above/below' },
  ],
  Footnote: [
    { value: 'Number', label: 'Footnote number' },
    { value: 'PageNumber', label: 'Page number' },
  ],
  Endnote: [
    { value: 'Number', label: 'Endnote number' },
    { value: 'PageNumber', label: 'Page number' },
  ],
  caption: [
    { value: 'Text', label: 'Entire caption' },
    { value: 'Number', label: 'Only label and number' },
    { value: 'PageNumber', label: 'Page number' },
    { value: 'AboveBelow', label: 'Above/below' },
  ],
};

export function CrossRefDialog({ model }: { model: WriteEditorModel }) {
  const [items, setItems] = useState<CrossRefItem[] | null>(null);
  const [category, setCategory] = useState('Heading');
  const [index, setIndex] = useState(0);
  const [show, setShow] = useState<CrossRefShow>('Text');
  const [link, setLink] = useState(true);
  useEffect(() => {
    void model.query('CrossRefTargets').then((r) => {
      if (r && 'CrossRefTargets' in r) setItems(r.CrossRefTargets);
    });
  }, [model]);
  const categories = useMemo(() => {
    const set = new Set<string>(['Heading', 'Bookmark', 'Footnote', 'Endnote']);
    for (const i of items ?? []) set.add(i.category);
    return [...set];
  }, [items]);
  const list = (items ?? []).filter((i) => i.category === category);
  const showOptions = SHOW[category] ?? SHOW.caption ?? [];
  const chosen = list[index] ?? null;
  return (
    <Frame
      model={model}
      title="Cross-reference"
      testId="write-crossref-dialog"
      okLabel="Insert"
      okDisabled={!chosen}
      size="lg"
      onOk={() =>
        chosen
          ? model.run('write.insert.cross_reference', {
              spec: { target: chosen.target, show, hyperlink: link },
            })
          : false
      }
    >
      <Grid>
        <Field label="Reference type">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={category}
              onValueChange={(v) => {
                setCategory(v);
                setIndex(0);
                setShow((SHOW[v] ?? SHOW.caption ?? [])[0]?.value ?? 'Text');
              }}
              options={categories.map((c) => ({ value: c, label: c }))}
            />
          )}
        </Field>
        <Field label="Insert reference to">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={show}
              onValueChange={(v) => setShow(v as CrossRefShow)}
              options={showOptions}
            />
          )}
        </Field>
      </Grid>
      <Check label="Insert as hyperlink" checked={link} onChange={setLink} />
      <div
        className="kiln-crossref-dialog__list"
        role="listbox"
        aria-label={`For which ${category.toLowerCase()}`}
      >
        {items === null ? <p className="kiln-comments-pane__empty">Reading the document…</p> : null}
        {items !== null && list.length === 0 ? (
          <p className="kiln-comments-pane__empty">The document has no {category.toLowerCase()} items.</p>
        ) : null}
        {list.map((i, k) => (
          <button
            key={`${i.category}-${k}-${i.label}`}
            type="button"
            role="option"
            aria-selected={k === index}
            className="kiln-crossref-dialog__item"
            style={{ paddingLeft: `${8 + (i.level - 1) * 14}px` }}
            onClick={() => setIndex(k)}
          >
            {i.label}
          </button>
        ))}
      </div>
    </Frame>
  );
}

const NUMBER_FORMATS: { value: string; label: string }[] = [
  { value: 'decimal', label: '1, 2, 3, …' },
  { value: 'lowerLetter', label: 'a, b, c, …' },
  { value: 'upperLetter', label: 'A, B, C, …' },
  { value: 'lowerRoman', label: 'i, ii, iii, …' },
  { value: 'upperRoman', label: 'I, II, III, …' },
  { value: 'chicago', label: '*, †, ‡, §, …' },
];

export function FootnoteDialog({ model }: { model: WriteEditorModel }) {
  const [endnotes, setEndnotes] = useState(false);
  const [format, setFormat] = useState('decimal');
  const [start, setStart] = useState(1);
  const [restart, setRestart] = useState('continuous');
  return (
    <Frame
      model={model}
      title="Footnote and Endnote"
      testId="write-footnote-dialog"
      okLabel="Apply"
      onOk={() => model.run('write.references.note_options', { spec: { endnotes, format, start, restart } })}
    >
      <Radio
        name="note-kind"
        label="Location"
        value={endnotes ? 'end' : 'foot'}
        onChange={(v) => {
          setEndnotes(v === 'end');
          setFormat(v === 'end' ? 'lowerRoman' : 'decimal');
        }}
        options={[
          ['foot', 'Footnotes (bottom of page)'],
          ['end', 'Endnotes (end of document)'],
        ]}
      />
      <Section title="Format">
        <Grid>
          <Field label="Number format">
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={format}
                onValueChange={setFormat}
                options={NUMBER_FORMATS}
              />
            )}
          </Field>
          <Field label="Start at">
            {(id) => (
              <NumberInput
                id={id}
                aria-label="Start at"
                value={start}
                onValueChange={(v) => setStart(Math.max(1, Math.round(v)))}
                min={1}
                max={32767}
                precision={0}
                width={90}
              />
            )}
          </Field>
          <Field label="Numbering">
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={restart}
                onValueChange={setRestart}
                options={[
                  { value: 'continuous', label: 'Continuous' },
                  { value: 'eachSect', label: 'Restart each section' },
                  ...(endnotes ? [] : [{ value: 'eachPage', label: 'Restart each page' }]),
                ]}
              />
            )}
          </Field>
        </Grid>
      </Section>
      <Section title="Convert">
        <div className="kiln-write-row">
          <Button
            size="sm"
            onClick={() => void model.run('write.references.convert_notes', { to_endnotes: true })}
          >
            Convert all footnotes to endnotes
          </Button>
          <Button
            size="sm"
            onClick={() => void model.run('write.references.convert_notes', { to_endnotes: false })}
          >
            Convert all endnotes to footnotes
          </Button>
        </div>
      </Section>
    </Frame>
  );
}

const WRAPS: [WrapMode, string][] = [
  ['InLine', 'In line with text'],
  ['Square', 'Square'],
  ['Tight', 'Tight'],
  ['Through', 'Through'],
  ['TopAndBottom', 'Top and bottom'],
  ['Behind', 'Behind text'],
  ['InFront', 'In front of text'],
];

export function LayoutDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const object = useWrite(model, (s) => s.format?.features.object ?? null);
  const [wrap, setWrap] = useState<WrapMode>(object?.wrap ?? 'Square');
  const [x, setX] = useState(72);
  const [xFrom, setXFrom] = useState<RelativeTo>('Margin');
  const [y, setY] = useState(0);
  const [yFrom, setYFrom] = useState<RelativeTo>('ColumnOrParagraph');
  const [width, setWidth] = useState(object?.width ?? 100);
  const [height, setHeight] = useState(object?.height ?? 100);
  const [rotation, setRotation] = useState(object?.rotation ?? 0);
  const [lock, setLock] = useState(object?.kind === 'Picture');
  const [alt, setAlt] = useState(object?.alt_text ?? '');
  const [decorative, setDecorative] = useState(object?.decorative ?? false);
  const [positionTouched, setPositionTouched] = useState(false);
  if (!object)
    return (
      <Frame model={model} title="Layout" testId="write-layout-dialog" onOk={() => true}>
        <p>Select a picture, shape or text box first.</p>
      </Frame>
    );
  const ratio = object.height > 0 ? object.width / object.height : 1;
  const from = [
    { value: 'Page', label: 'Page' },
    { value: 'Margin', label: 'Margin' },
    { value: 'ColumnOrParagraph', label: 'Column / Paragraph' },
  ];
  const spec: ObjectLayoutSpec = {
    wrap,
    x: positionTouched && wrap !== 'InLine' ? x : null,
    x_from: positionTouched ? xFrom : null,
    y: positionTouched && wrap !== 'InLine' ? y : null,
    y_from: positionTouched ? yFrom : null,
    width,
    height,
    rotation,
    distance: null,
    move_with_text: null,
    allow_overlap: null,
    name: null,
    alt_text: alt,
    decorative,
  };
  return (
    <Frame
      model={model}
      title="Layout"
      testId="write-layout-dialog"
      size="lg"
      onOk={() => model.run('write.arrange.layout', { id: object.id, spec })}
    >
      <Tabs defaultValue={tab === 'wrap' ? 'wrap' : tab === 'size' ? 'size' : 'position'}>
        <TabsList aria-label="Layout">
          <TabsTrigger value="position">Position</TabsTrigger>
          <TabsTrigger value="wrap">Text Wrapping</TabsTrigger>
          <TabsTrigger value="size">Size</TabsTrigger>
          <TabsTrigger value="alt">Alt Text</TabsTrigger>
        </TabsList>
        <TabsContent value="position">
          <Section title="Horizontal">
            <Grid>
              <Field label="Absolute position (pt)">
                {(id) => (
                  <NumberInput
                    id={id}
                    aria-label="Horizontal position"
                    value={x}
                    onValueChange={(v) => {
                      setX(v);
                      setPositionTouched(true);
                    }}
                    unit="pt"
                    min={-1584}
                    max={1584}
                  />
                )}
              </Field>
              <Field label="to the right of">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={xFrom}
                    onValueChange={(v) => {
                      setXFrom(v as RelativeTo);
                      setPositionTouched(true);
                    }}
                    options={from}
                  />
                )}
              </Field>
            </Grid>
          </Section>
          <Section title="Vertical">
            <Grid>
              <Field label="Absolute position (pt)">
                {(id) => (
                  <NumberInput
                    id={id}
                    aria-label="Vertical position"
                    value={y}
                    onValueChange={(v) => {
                      setY(v);
                      setPositionTouched(true);
                    }}
                    unit="pt"
                    min={-1584}
                    max={1584}
                  />
                )}
              </Field>
              <Field label="below">
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={yFrom}
                    onValueChange={(v) => {
                      setYFrom(v as RelativeTo);
                      setPositionTouched(true);
                    }}
                    options={from}
                  />
                )}
              </Field>
            </Grid>
          </Section>
        </TabsContent>
        <TabsContent value="wrap">
          <div className="kiln-wrap-grid" role="radiogroup" aria-label="Wrapping style">
            {WRAPS.map(([w, label]) => (
              // biome-ignore lint/a11y/useSemanticElements: picture tiles act as a radio group, as in Word's Layout dialog
              <button
                key={w}
                type="button"
                role="radio"
                aria-checked={wrap === w}
                className="kiln-wrap-grid__item"
                data-wrap={w}
                onClick={() => setWrap(w)}
              >
                <span className={`kiln-wrap-grid__icon kiln-wrap-grid__icon--${w}`} aria-hidden="true" />
                {label}
              </button>
            ))}
          </div>
        </TabsContent>
        <TabsContent value="size">
          <Grid>
            <Field label="Height (pt)">
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label="Height"
                  value={height}
                  onValueChange={(v) => {
                    setHeight(v);
                    if (lock) setWidth(Math.round(v * ratio * 100) / 100);
                  }}
                  unit="pt"
                  min={1}
                  max={1584}
                />
              )}
            </Field>
            <Field label="Width (pt)">
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label="Width"
                  value={width}
                  onValueChange={(v) => {
                    setWidth(v);
                    if (lock) setHeight(Math.round((v / ratio) * 100) / 100);
                  }}
                  unit="pt"
                  min={1}
                  max={1584}
                />
              )}
            </Field>
            <Field label="Rotation">
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label="Rotation"
                  value={rotation}
                  onValueChange={setRotation}
                  unit="°"
                  min={-360}
                  max={360}
                />
              )}
            </Field>
          </Grid>
          <Check label="Lock aspect ratio" checked={lock} onChange={setLock} />
        </TabsContent>
        <TabsContent value="alt">
          <Field label="Alt text (how would you describe this to someone who can't see it?)">
            {(id) => (
              <textarea
                id={id}
                className="kiln-write-input kiln-write-textarea"
                rows={3}
                value={alt}
                disabled={decorative}
                onChange={(e) => setAlt(e.target.value)}
              />
            )}
          </Field>
          <Check label="Mark as decorative" checked={decorative} onChange={setDecorative} />
        </TabsContent>
      </Tabs>
    </Frame>
  );
}

const plural = (n: number, noun: string) => `${n.toLocaleString()} ${noun}${n === 1 ? '' : 's'}`;

export function PropertiesDialog({ model }: { model: WriteEditorModel }) {
  const [info, setInfo] = useState<DocPropertiesInfo | null>(null);
  const [edit, setEdit] = useState<Record<string, string>>({});
  useEffect(() => {
    void model.query('Properties').then((r) => {
      if (r && 'Properties' in r) setInfo(r.Properties);
    });
  }, [model]);
  const field = (key: string, label: string, value: string | null | undefined) => (
    <Field label={label}>
      {(id) => (
        <input
          id={id}
          className="kiln-write-input"
          value={edit[key] ?? value ?? ''}
          onChange={(e) => setEdit({ ...edit, [key]: e.target.value })}
        />
      )}
    </Field>
  );
  return (
    <Frame
      model={model}
      title="Properties"
      testId="write-properties-dialog"
      onOk={() => model.run('write.file.properties', { props: edit })}
    >
      {info ? (
        <div className="kiln-properties-dialog__fields">
          {field('title', 'Title', info.title)}
          {field('subject', 'Subject', info.subject)}
          {field('author', 'Author', info.author)}
          {field('keywords', 'Tags', info.keywords)}
          {field('description', 'Comments', info.description)}
          <p className="kiln-properties-dialog__stats">
            {plural(info.pages, 'page')} · {plural(info.words, 'word')} ·{' '}
            {plural(info.characters, 'character')}
          </p>
        </div>
      ) : (
        <p>Reading the properties…</p>
      )}
    </Frame>
  );
}

export function WordGoalDialog({ model }: { model: WriteEditorModel }) {
  const goal = useWrite(model, (s) => s.format?.features.word_goal ?? 0);
  const words = useWrite(model, (s) => s.status?.words ?? 0);
  const [value, setValue] = useState(goal || Math.max(500, Math.ceil(((words ?? 0) + 1) / 500) * 500));
  return (
    <Frame
      model={model}
      title="Word goal"
      testId="write-word-goal-dialog"
      size="sm"
      onOk={() => model.run('write.view.word_goal', { words: Math.max(0, Math.round(value)) })}
    >
      <p>
        A target for this document, saved with it. The status bar shows your progress (
        {(words ?? 0).toLocaleString()} words so far).
      </p>
      <Field label="Goal (words)">
        {(id) => (
          <NumberInput
            id={id}
            aria-label="Goal (words)"
            value={value}
            onValueChange={setValue}
            min={0}
            max={10_000_000}
            precision={0}
            step={100}
            width={140}
          />
        )}
      </Field>
      {goal > 0 ? (
        <Button
          size="sm"
          variant="ghost"
          onClick={() => void model.run('write.view.word_goal', { words: 0 })}
        >
          Remove the goal
        </Button>
      ) : null}
    </Frame>
  );
}

/** The open feature dialog. */
export function FeatureDialogs({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const dialog = useStore(features.store, (s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.id) {
    case 'toc':
      return <TocDialog model={model} />;
    case 'caption':
      return <CaptionDialog model={model} />;
    case 'crossRef':
      return <CrossRefDialog model={model} />;
    case 'footnote':
      return <FootnoteDialog model={model} />;
    case 'layout':
      return <LayoutDialog model={model} {...(dialog.tab ? { tab: dialog.tab } : {})} />;
    case 'properties':
      return <PropertiesDialog model={model} />;
    case 'wordGoal':
      return <WordGoalDialog model={model} />;
    case 'updateToc':
      return null;
  }
}
