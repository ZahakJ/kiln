/**
 * The dialogs of Kiln Write's features: Table of Contents (custom), Insert Caption,
 * Cross-reference, Footnote and Endnote, Layout (More Layout Options: position, wrapping, size,
 * rotation, alt text), document Properties and the word-count goal. Each sends one core command
 * built from its fields; the core validates and answers with a message when it disagrees.
 */
import { formatNumber, msg, t, tx, txOptions } from '@kiln/i18n';
import type {
  CaptionPosition,
  CrossRefItem,
  CrossRefShow,
  ObjectLayoutSpec,
  PictureCrop,
  RelativeTo,
  ShapeMargins,
  TabLeaderKind,
  TextAnchor,
  WrapMode,
} from '@kiln/protocol';
import {
  Button,
  Dialog,
  DialogContent,
  NumberInput,
  Select,
  PropertiesDialog as SuitePropertiesDialog,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@kiln/ui-kit';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
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
  const close = () => features.closeDialog();
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={title}
        size={size}
        data-testid={testId}
        // Back to the page, as in Word: typing goes on where the dialog was opened. Done as the
        // dialog lets go of the focus, not on a timer that a slower machine can run too early.
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          model.controller?.focus();
        }}
        footer={
          <>
            <Button onClick={close}>{t('Cancel')}</Button>
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
  { value: 'None', label: msg('(none)') },
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
      title={t('Table of Contents')}
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
        <div className="kiln-toc-dialog__preview" aria-label={t('Print preview')} role="img">
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
          <Field label={t('Title')}>
            {(id) => (
              <input
                id={id}
                className="kiln-write-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            )}
          </Field>
          <Check label={t('Show page numbers')} checked={pages} onChange={setPages} />
          <Check label={t('Right align page numbers')} checked={right} onChange={setRight} />
          <Field label={t('Tab leader')}>
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={leader}
                onValueChange={(v) => setLeader(v as TabLeaderKind)}
                options={txOptions(LEADERS)}
                disabled={!pages || !right}
              />
            )}
          </Field>
          <Check
            label={t('Use hyperlinks instead of page numbers in Web Layout')}
            checked={links}
            onChange={setLinks}
          />
          <Field label={t('Show levels')}>
            {(id) => (
              <NumberInput
                id={id}
                aria-label={t('Show levels')}
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
      title={t('Caption')}
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
      <Field label={t('Caption text (after the number)')}>
        {(id) => (
          <input
            id={id}
            className="kiln-write-input"
            value={text}
            placeholder={t('Glaze test tiles')}
            // biome-ignore lint/a11y/noAutofocus: the caption text is what the dialog is for
            autoFocus
            onChange={(e) => setText(e.target.value)}
          />
        )}
      </Field>
      <Grid>
        <Field label={t('Label')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={label}
              onValueChange={setLabel}
              options={labels.map((l) => ({ value: l, label: l }))}
            />
          )}
        </Field>
        <Field label={t('Position')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={position}
              onValueChange={(v) => setPosition(v as CaptionPosition)}
              options={[
                { value: 'Below', label: t('Below selected item') },
                { value: 'Above', label: t('Above selected item') },
              ]}
            />
          )}
        </Field>
      </Grid>
      <Check label={t('Exclude label from caption')} checked={exclude} onChange={setExclude} />
      <Check label={t('Include chapter number (Heading 1)')} checked={chapter} onChange={setChapter} />
      {newLabel === null ? (
        <Button size="sm" onClick={() => setNewLabel('')}>
          {t('New Label…')}
        </Button>
      ) : (
        <div className="kiln-write-row">
          <input
            className="kiln-write-input"
            aria-label={t('New label')}
            placeholder={t('Label (e.g. Chart)')}
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
            {t('Add')}
          </Button>
        </div>
      )}
    </Frame>
  );
}

/** The core's cross-reference categories, translated where they are shown. */
const CATEGORIES = [
  msg('Heading'),
  msg('Bookmark'),
  msg('Footnote'),
  msg('Endnote'),
  msg('Figure'),
  msg('Table'),
  msg('Equation'),
];
void CATEGORIES;

const SHOW: Record<string, { value: CrossRefShow; label: string }[]> = {
  Heading: [
    { value: 'Text', label: msg('Heading text') },
    { value: 'PageNumber', label: msg('Page number') },
    { value: 'Number', label: msg('Heading number') },
    { value: 'AboveBelow', label: msg('Above/below') },
  ],
  Bookmark: [
    { value: 'Text', label: msg('Bookmark text') },
    { value: 'PageNumber', label: msg('Page number') },
    { value: 'AboveBelow', label: msg('Above/below') },
  ],
  Footnote: [
    { value: 'Number', label: msg('Footnote number') },
    { value: 'PageNumber', label: msg('Page number') },
  ],
  Endnote: [
    { value: 'Number', label: msg('Endnote number') },
    { value: 'PageNumber', label: msg('Page number') },
  ],
  caption: [
    { value: 'Text', label: msg('Entire caption') },
    { value: 'Number', label: msg('Only label and number') },
    { value: 'PageNumber', label: msg('Page number') },
    { value: 'AboveBelow', label: msg('Above/below') },
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
      title={t('Cross-reference')}
      testId="write-crossref-dialog"
      okLabel={t('Insert')}
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
        <Field label={t('Reference type')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={category}
              onValueChange={(v) => {
                setCategory(v);
                setIndex(0);
                setShow((SHOW[v] ?? SHOW.caption ?? [])[0]?.value ?? 'Text');
              }}
              options={categories.map((c) => ({ value: c, label: tx(c) }))}
            />
          )}
        </Field>
        <Field label={t('Insert reference to')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={show}
              onValueChange={(v) => setShow(v as CrossRefShow)}
              options={txOptions(showOptions)}
            />
          )}
        </Field>
      </Grid>
      <Check label={t('Insert as hyperlink')} checked={link} onChange={setLink} />
      {items === null ? <p className="kiln-comments-pane__empty">{t('Reading the document…')}</p> : null}
      {items !== null && list.length === 0 ? (
        <p className="kiln-comments-pane__empty">
          {t('The document has no “{category}” items.', { category: tx(category) })}
        </p>
      ) : null}
      {/* A listbox holds options only: the messages above stand outside it. */}
      <div
        className="kiln-crossref-dialog__list"
        role="listbox"
        aria-label={t('For which “{category}”', { category: tx(category) })}
        hidden={list.length === 0}
      >
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
  { value: 'lowerLetter', label: msg('a, b, c, …') },
  { value: 'upperLetter', label: msg('A, B, C, …') },
  { value: 'lowerRoman', label: msg('i, ii, iii, …') },
  { value: 'upperRoman', label: msg('I, II, III, …') },
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
      title={t('Footnote and Endnote')}
      testId="write-footnote-dialog"
      okLabel={t('Apply')}
      onOk={() => model.run('write.references.note_options', { spec: { endnotes, format, start, restart } })}
    >
      <Radio
        name="note-kind"
        label={t('Location')}
        value={endnotes ? 'end' : 'foot'}
        onChange={(v) => {
          setEndnotes(v === 'end');
          setFormat(v === 'end' ? 'lowerRoman' : 'decimal');
        }}
        options={[
          ['foot', t('Footnotes (bottom of page)')],
          ['end', t('Endnotes (end of document)')],
        ]}
      />
      <Section title={t('Format')}>
        <Grid>
          <Field label={t('Number format')}>
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={format}
                onValueChange={setFormat}
                options={txOptions(NUMBER_FORMATS)}
              />
            )}
          </Field>
          <Field label={t('Start at')}>
            {(id) => (
              <NumberInput
                id={id}
                aria-label={t('Start at')}
                value={start}
                onValueChange={(v) => setStart(Math.max(1, Math.round(v)))}
                min={1}
                max={32767}
                precision={0}
                width={90}
              />
            )}
          </Field>
          <Field label={t('Numbering')}>
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={restart}
                onValueChange={setRestart}
                options={[
                  { value: 'continuous', label: t('Continuous') },
                  { value: 'eachSect', label: t('Restart each section') },
                  ...(endnotes ? [] : [{ value: 'eachPage', label: t('Restart each page') }]),
                ]}
              />
            )}
          </Field>
        </Grid>
      </Section>
      <Section title={t('Convert')}>
        <div className="kiln-write-row">
          <Button
            size="sm"
            onClick={() => void model.run('write.references.convert_notes', { to_endnotes: true })}
          >
            {t('Convert all footnotes to endnotes')}
          </Button>
          <Button
            size="sm"
            onClick={() => void model.run('write.references.convert_notes', { to_endnotes: false })}
          >
            {t('Convert all endnotes to footnotes')}
          </Button>
        </div>
      </Section>
    </Frame>
  );
}

const WRAPS: [WrapMode, string][] = [
  ['InLine', msg('In line with text')],
  ['Square', msg('Square')],
  ['Tight', msg('Tight')],
  ['Through', msg('Through')],
  ['TopAndBottom', msg('Top and bottom')],
  ['Behind', msg('Behind text')],
  ['InFront', msg('In front of text')],
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
  const [crop, setCrop] = useState<PictureCrop>(object?.crop ?? { left: 0, top: 0, right: 0, bottom: 0 });
  const [anchor, setAnchor] = useState<TextAnchor>(object?.shape?.anchor ?? 'Top');
  const [margins, setMargins] = useState<ShapeMargins>(
    object?.shape?.margins ?? { left: 7.2, top: 3.6, right: 7.2, bottom: 3.6 },
  );
  const textChanged =
    !!object?.shape &&
    (anchor !== object.shape.anchor ||
      (['left', 'top', 'right', 'bottom'] as const).some(
        (k) => Math.abs(margins[k] - (object.shape?.margins[k] ?? 0)) > 0.001,
      ));
  const cropChanged =
    !!object?.crop &&
    (['left', 'top', 'right', 'bottom'] as const).some(
      (k) => Math.abs(crop[k] - (object.crop?.[k] ?? 0)) > 0.001,
    );
  if (!object)
    return (
      <Frame model={model} title={t('Layout')} testId="write-layout-dialog" onOk={() => true}>
        <p>{t('Select a picture, shape or text box first.')}</p>
      </Frame>
    );
  const ratio = object.height > 0 ? object.width / object.height : 1;
  const from = [
    { value: 'Page', label: t('Page') },
    { value: 'Margin', label: t('Margin') },
    { value: 'ColumnOrParagraph', label: t('Column / Paragraph') },
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
      title={t('Layout')}
      testId="write-layout-dialog"
      size="lg"
      onOk={async () => {
        let ok = await model.run('write.arrange.layout', { id: object.id, spec });
        if (ok && textChanged) ok = await model.run('write.shape.text_layout', { anchor, margins });
        // Cropping keeps the picture's scale, so it follows the size the dialog set.
        return ok && cropChanged ? model.run('write.picture.crop', { crop }) : ok;
      }}
    >
      <Tabs
        defaultValue={
          tab === 'crop' && object.crop
            ? 'crop'
            : tab === 'text' && object.shape
              ? 'text'
              : tab && ['wrap', 'size', 'alt'].includes(tab)
                ? tab
                : 'position'
        }
      >
        <TabsList aria-label={t('Layout')}>
          <TabsTrigger value="position">{t('Position')}</TabsTrigger>
          <TabsTrigger value="wrap">{t('Text Wrapping')}</TabsTrigger>
          <TabsTrigger value="size">{t('Size')}</TabsTrigger>
          {object.crop ? <TabsTrigger value="crop">{t('Crop')}</TabsTrigger> : null}
          {object.shape ? <TabsTrigger value="text">{t('Text Box')}</TabsTrigger> : null}
          <TabsTrigger value="alt">{t('Alt Text')}</TabsTrigger>
        </TabsList>
        <TabsContent value="position">
          <Section title={t('Horizontal')}>
            <Grid>
              <Field label={t('Absolute position (pt)')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    aria-label={t('Horizontal position')}
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
              <Field label={t('to the right of')}>
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
          <Section title={t('Vertical')}>
            <Grid>
              <Field label={t('Absolute position (pt)')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    aria-label={t('Vertical position')}
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
              <Field label={t('below')}>
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
          <div className="kiln-wrap-grid" role="radiogroup" aria-label={t('Wrapping style')}>
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
                {tx(label)}
              </button>
            ))}
          </div>
        </TabsContent>
        <TabsContent value="size">
          <Grid>
            <Field label={t('Height (pt)')}>
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label={t('Height')}
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
            <Field label={t('Width (pt)')}>
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label={t('Width')}
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
            <Field label={t('Rotation')}>
              {(id) => (
                <NumberInput
                  id={id}
                  aria-label={t('Rotation')}
                  value={rotation}
                  onValueChange={setRotation}
                  unit="°"
                  min={-360}
                  max={360}
                />
              )}
            </Field>
          </Grid>
          <Check label={t('Lock aspect ratio')} checked={lock} onChange={setLock} />
        </TabsContent>
        {object.shape ? (
          <TabsContent value="text">
            <Field label={t('Vertical alignment')}>
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={anchor}
                  onValueChange={(v) => setAnchor(v as TextAnchor)}
                  options={[
                    { value: 'Top', label: t('Top') },
                    { value: 'Middle', label: t('Middle') },
                    { value: 'Bottom', label: t('Bottom') },
                  ]}
                />
              )}
            </Field>
            <Section title={t('Internal margin')}>
              <Grid>
                {(
                  [
                    ['left', t('Left')],
                    ['right', t('Right')],
                    ['top', t('Top')],
                    ['bottom', t('Bottom')],
                  ] as const
                ).map(([k, label]) => (
                  <Field key={k} label={t('{label} (pt)', { label })}>
                    {(id) => (
                      <NumberInput
                        id={id}
                        aria-label={t('{label} margin', { label })}
                        value={margins[k]}
                        onValueChange={(v) => setMargins({ ...margins, [k]: v })}
                        unit="pt"
                        min={0}
                        max={288}
                        precision={1}
                      />
                    )}
                  </Field>
                ))}
              </Grid>
            </Section>
          </TabsContent>
        ) : null}
        {object.crop ? (
          <TabsContent value="crop">
            <p className="kiln-layout-dialog__hint">
              {t('How much of the picture to cut from each side. The picture keeps its scale.')}
            </p>
            <Grid>
              {(
                [
                  ['left', t('Left')],
                  ['right', t('Right')],
                  ['top', t('Top')],
                  ['bottom', t('Bottom')],
                ] as const
              ).map(([k, label]) => (
                <Field key={k} label={`${label} (%)`}>
                  {(id) => (
                    <NumberInput
                      id={id}
                      aria-label={t('Crop {label}', { label: label.toLowerCase() })}
                      value={crop[k]}
                      onValueChange={(v) => setCrop({ ...crop, [k]: v })}
                      unit="%"
                      min={0}
                      max={99}
                      precision={1}
                    />
                  )}
                </Field>
              ))}
            </Grid>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setCrop({ left: 0, top: 0, right: 0, bottom: 0 })}
            >
              {t('No cropping')}
            </Button>
          </TabsContent>
        ) : null}
        <TabsContent value="alt">
          <Field label={t("Alt text (how would you describe this to someone who can't see it?)")}>
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
          <Check label={t('Mark as decorative')} checked={decorative} onChange={setDecorative} />
        </TabsContent>
      </Tabs>
    </Frame>
  );
}

/**
 * Properties… (Office's Advanced Properties): the suite's Properties dialog, the same
 * File ▸ Info opens in every app.
 */
export function PropertiesDialog({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const [title, setTitle] = useState('');
  useEffect(() => {
    void model.client
      .query(model.doc, 'Summary')
      .then((r) => 'Summary' in r && setTitle(r.Summary.title))
      .catch(() => undefined);
  }, [model]);
  const close = () => {
    features.closeDialog();
    setTimeout(() => model.controller?.focus(), 0);
  };
  return (
    <SuitePropertiesDialog
      open
      onOpenChange={(o) => (o ? undefined : close())}
      app="write"
      name={title}
      load={async () => {
        const r = await model.client.query(model.doc, 'Properties');
        if (!('Properties' in r)) throw new Error('no properties');
        return r.Properties;
      }}
      save={async (props) => {
        // Write's own command (an undo step), as Word's Advanced Properties.
        if (!(await model.run('write.file.properties', { props }))) throw new Error('not saved');
      }}
    />
  );
}

/** Numbering ▸ Set Numbering Value…: the number the list item at the caret starts at. */
export function NumberingValueDialog({ model }: { model: WriteEditorModel }) {
  const [value, setValue] = useState(1);
  return (
    <Frame
      model={model}
      title={t('Set Numbering Value')}
      testId="write-numbering-value-dialog"
      size="sm"
      onOk={() => model.run('write.list.set_value', { value: Math.max(0, Math.round(value)) })}
    >
      <Field label={t('Set value to')}>
        {(id) => (
          <NumberInput
            id={id}
            aria-label={t('Set value to')}
            value={value}
            onValueChange={setValue}
            min={0}
            max={32767}
            precision={0}
            step={1}
            width={120}
          />
        )}
      </Field>
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
      title={t('Word goal')}
      testId="write-word-goal-dialog"
      size="sm"
      onOk={() => model.run('write.view.word_goal', { words: Math.max(0, Math.round(value)) })}
    >
      <p>
        {t(
          'A target for this document, saved with it. The status bar shows your progress ({value} words so far).',
          { value: formatNumber(words ?? 0) },
        )}
      </p>
      <Field label={t('Goal (words)')}>
        {(id) => (
          <NumberInput
            id={id}
            aria-label={t('Goal (words)')}
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
          {t('Remove the goal')}
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
    case 'numberingValue':
      return <NumberingValueDialog model={model} />;
    case 'icons':
      return <IconsDialog model={model} />;
    case 'updateToc':
      return null;
  }
}

/** Icon colours: ink and the brand's accents (the document's own colours are a theme away). */
const ICON_COLOURS: readonly [string, string][] = [
  ['#2B1C14', msg('Char')],
  ['#C2481C', msg('Ember')],
  ['#2F5AA8', msg('Blue')],
  ['#2E7D4F', msg('Green')],
  ['#8A3FA0', msg('Plum')],
  ['#B8860B', msg('Ochre')],
  ['#6B6560', msg('Stone')],
];

/** An icon's SVG drawn into a PNG `size` pixels square, in `colour`. */
async function iconPng(svg: SVGSVGElement, colour: string, size: number): Promise<Uint8Array> {
  const copy = svg.cloneNode(true) as SVGSVGElement;
  copy.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  copy.setAttribute('width', String(size));
  copy.setAttribute('height', String(size));
  copy.setAttribute('stroke', colour);
  copy.removeAttribute('class');
  const url = URL.createObjectURL(
    new Blob([new XMLSerializer().serializeToString(copy)], { type: 'image/svg+xml' }),
  );
  try {
    const img = new Image();
    img.decoding = 'async';
    img.src = url;
    await img.decode();
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('no canvas');
    ctx.drawImage(img, 0, 0, size, size);
    const blob: Blob | null = await new Promise((r) => canvas.toBlob(r, 'image/png'));
    if (!blob) throw new Error('no picture');
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** Insert ▸ Icons: Kiln's built-in icons, searchable, in a colour; inserted as a picture an inch
 * square with the icon's name as alt text. */
export function IconsDialog({ model }: { model: WriteEditorModel }) {
  const [set, setSet] = useState<typeof import('./iconset.ts') | null>(null);
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<string | null>(null);
  const [colour, setColour] = useState(ICON_COLOURS[0]?.[0] ?? '#2B1C14');
  const grid = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let live = true;
    void import('./iconset.ts').then((m) => {
      if (live) setSet(m);
    });
    return () => {
      live = false;
    };
  }, []);
  const q = query.trim().toLowerCase();
  const categories = useMemo(
    () =>
      (set?.ICON_CATEGORIES ?? [])
        .map((c) => ({
          ...c,
          icons: c.icons.filter(
            (i) =>
              !q ||
              i.name.toLowerCase().includes(q) ||
              c.name.toLowerCase().includes(q) ||
              tx(i.name).toLowerCase().includes(q) ||
              tx(c.name).toLowerCase().includes(q),
          ),
        }))
        .filter((c) => c.icons.length > 0),
    [set, q],
  );
  const insert = async (): Promise<boolean> => {
    const svg = grid.current?.querySelector<SVGSVGElement>(`[data-icon="${CSS.escape(picked ?? '')}"] svg`);
    if (!picked || !svg) return false;
    const bytes = await iconPng(svg, colour, 384);
    const ok = await model.run('write.insert.picture', {
      name: `${picked}.png`,
      bytes: Array.from(bytes),
      floating: false,
    });
    if (!ok) return false;
    // The new picture is selected: size it and give it its alt text.
    await model.run('write.arrange.layout', {
      id: null,
      spec: {
        wrap: null,
        x: null,
        x_from: null,
        y: null,
        y_from: null,
        width: 72,
        height: 72,
        rotation: null,
        distance: null,
        move_with_text: null,
        allow_overlap: null,
        name: `${picked} icon`,
        alt_text: `${picked} icon`,
        decorative: null,
      },
    });
    return true;
  };
  return (
    <Frame
      model={model}
      title={t('Insert Icons')}
      testId="write-icons-dialog"
      size="lg"
      okLabel={t('Insert')}
      okDisabled={!picked}
      onOk={insert}
    >
      <div className="kiln-icons-dialog__bar">
        <input
          type="search"
          className="kiln-write-input"
          placeholder={t('Search icons')}
          aria-label={t('Search icons')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="kiln-icons-dialog__colours" role="radiogroup" aria-label={t('Colour')}>
          {ICON_COLOURS.map(([hex, name]) => (
            // biome-ignore lint/a11y/useSemanticElements: colour swatches act as a radio group
            <button
              key={hex}
              type="button"
              role="radio"
              aria-checked={colour === hex}
              aria-label={tx(name)}
              title={tx(name)}
              className="kiln-icons-dialog__swatch"
              style={{ background: hex }}
              onClick={() => setColour(hex)}
            />
          ))}
        </div>
      </div>
      <div className="kiln-icons-dialog__grid" ref={grid} style={{ color: colour }}>
        {!set ? <p>{t('Loading the icons…')}</p> : null}
        {set && categories.length === 0 ? <p>{t('No icons match “{query}”.', { query })}</p> : null}
        {categories.map((c) => (
          <section key={c.name} aria-label={tx(c.name)}>
            <h3 className="kiln-icons-dialog__heading">{tx(c.name)}</h3>
            <div className="kiln-icons-dialog__tiles" role="listbox" aria-label={tx(c.name)}>
              {c.icons.map((i) => {
                const Glyph = i.icon;
                return (
                  <button
                    key={i.name}
                    type="button"
                    role="option"
                    aria-selected={picked === i.name}
                    className="kiln-icons-dialog__tile"
                    data-icon={i.name}
                    title={tx(i.name)}
                    onClick={() => setPicked(i.name)}
                  >
                    <Glyph size={28} strokeWidth={1.75} aria-hidden="true" />
                    <span>{tx(i.name)}</span>
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>
    </Frame>
  );
}
