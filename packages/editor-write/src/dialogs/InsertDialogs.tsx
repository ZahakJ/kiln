/**
 * Insert ▸ Link, Symbol, Date & Time / Field, Bookmark; Review ▸ Word Count; Paste Special; New
 * Style and Modify Style. Each runs one core command; what the core rejects (a bad address, a
 * duplicate style name) comes back as its message.
 */
import type {
  ClipboardData,
  FontFormat,
  PasteMode,
  StyleSpec,
  TextAlign,
  WordCountInfo,
} from '@kiln/protocol';
import { Button, Select } from '@kiln/ui-kit';
import { useEffect, useMemo, useState } from 'react';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';
import { Check, Field, Grid, Radio, Section, WriteDialogFrame } from './common.tsx';
import { EMPTY_PARAGRAPH } from './ParagraphDialog.tsx';

function TextInput({
  id,
  value,
  onChange,
  placeholder,
  autoFocus,
}: {
  id: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
}) {
  return (
    <input
      id={id}
      className="kiln-write-input"
      value={value}
      placeholder={placeholder}
      // biome-ignore lint/a11y/noAutofocus: the dialog's first field takes focus when it opens, as in Word
      autoFocus={autoFocus}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function useNotice(model: WriteEditorModel) {
  return useWrite(model, (s) => s.notice);
}

export function LinkDialog({ model }: { model: WriteEditorModel }) {
  const f = model.state.format;
  const [url, setUrl] = useState(f?.link ?? 'https://');
  const [text, setText] = useState('');
  const [tip, setTip] = useState('');
  const [bookmarks, setBookmarks] = useState<string[]>([]);
  const notice = useNotice(model);
  useEffect(() => {
    model.set({ notice: null });
    void model.query('Bookmarks').then((r) => {
      if (r && 'Bookmarks' in r) setBookmarks(r.Bookmarks);
    });
  }, [model]);
  const collapsed = f?.collapsed ?? true;
  return (
    <WriteDialogFrame
      model={model}
      title={f?.link ? 'Edit Hyperlink' : 'Insert Hyperlink'}
      testId="write-link-dialog"
      okDisabled={!url.trim() || url.trim() === 'https://'}
      onOk={async () =>
        !(await model.run('write.insert.link', {
          url: url.trim(),
          text: collapsed && text ? text : null,
          tooltip: tip || null,
        }))
      }
      footerStart={
        f?.link ? (
          <Button
            onClick={async () => {
              if (await model.run('write.insert.remove_link')) model.closeDialog();
            }}
          >
            Remove Link
          </Button>
        ) : null
      }
    >
      {collapsed ? (
        <Field label="Text to display">
          {(id) => <TextInput id={id} value={text} onChange={setText} placeholder="The address, if empty" />}
        </Field>
      ) : null}
      <Field label="Address" hint="A web address, an e-mail (mailto:) or a place in this document">
        {(id) => <TextInput id={id} value={url} onChange={setUrl} autoFocus />}
      </Field>
      {bookmarks.length ? (
        <Field label="Place in this document">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value=""
              placeholder="Choose a bookmark"
              onValueChange={(b) => setUrl(`#${b}`)}
              options={bookmarks.map((b) => ({ value: b, label: b }))}
            />
          )}
        </Field>
      ) : null}
      <Field label="ScreenTip">{(id) => <TextInput id={id} value={tip} onChange={setTip} />}</Field>
      {notice ? (
        <p className="kiln-write-status" role="alert">
          {notice}
        </p>
      ) : null}
    </WriteDialogFrame>
  );
}

const SYMBOLS: Record<string, string> = {
  Common: '©®™§¶†‡•…–—‘’“”«»€£¥¢°±×÷≠≤≥≈∞√∑∏µπΩ∆',
  Arrows: '←↑→↓↔↕⇐⇒⇔↩↪➔➜➝➞➢➤⤴⤵',
  Math: '∀∂∃∅∇∈∉∋∏∑−∓∗∘∙√∝∞∠∧∨∩∪∫∴∼≅≈≠≡≤≥⊂⊃⊆⊇⊕⊗⊥',
  Greek: 'αβγδεζηθικλμνξοπρστυφχψωΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ',
  Currency: '¤$¢£¥₣₤₧₨₩₪₫€₭₮₱₲₳₴₵₹₺₽₿',
  Shapes: '■□▪▫▲△▶▷▼▽◀◁◆◇○●◎★☆♠♣♥♦✓✔✗✘',
};
const RECENT_KEY = 'kiln.write.symbols';

function recentSymbols(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string').slice(0, 20) : [];
  } catch {
    return [];
  }
}

export function SymbolDialog({ model }: { model: WriteEditorModel }) {
  const [set, setSet] = useState('Common');
  const [code, setCode] = useState('');
  const [recent, setRecent] = useState(recentSymbols);
  const insert = async (ch: string) => {
    if (!(await model.run('write.edit.insert_text', { text: ch }))) return;
    const next = [ch, ...recent.filter((r) => r !== ch)].slice(0, 20);
    setRecent(next);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    } catch {
      // ignore
    }
  };
  const fromCode = useMemo(() => {
    const n = Number.parseInt(code.replace(/^(U\+|0x)/i, ''), 16);
    return Number.isFinite(n) && n > 31 && n < 0x110000 && !(n >= 0xd800 && n < 0xe000)
      ? String.fromCodePoint(n)
      : null;
  }, [code]);
  const grid = (chars: string, label: string) => (
    <fieldset className="kiln-write-symbols" aria-label={label}>
      {[...chars].map((ch) => (
        <button
          key={ch}
          type="button"
          className="kiln-write-symbol"
          aria-label={`Insert ${ch}`}
          onClick={() => void insert(ch)}
        >
          {ch}
        </button>
      ))}
    </fieldset>
  );
  return (
    <WriteDialogFrame model={model} title="Symbol" size="lg" testId="write-symbol-dialog">
      <Grid cols={2}>
        <Field label="Subset">
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={set}
              onValueChange={setSet}
              options={Object.keys(SYMBOLS).map((s) => ({ value: s, label: s }))}
            />
          )}
        </Field>
        <Field label="Character code (hex)">
          {(id) => (
            <div className="kiln-write-inline">
              <TextInput id={id} value={code} onChange={setCode} placeholder="2603" />
              <Button disabled={!fromCode} onClick={() => fromCode && void insert(fromCode)}>
                Insert {fromCode ?? ''}
              </Button>
            </div>
          )}
        </Field>
      </Grid>
      {grid(SYMBOLS[set] ?? '', set)}
      {recent.length ? (
        <Section title="Recently used symbols">{grid(recent.join(''), 'Recently used')}</Section>
      ) : null}
    </WriteDialogFrame>
  );
}

const DATE_FORMATS = [
  'M/d/yyyy',
  'dddd, MMMM d, yyyy',
  'MMMM d, yyyy',
  'd MMMM yyyy',
  'yyyy-MM-dd',
  'd-MMM-yy',
  'MMM. d, yy',
  'MMMM yy',
  'h:mm am/pm',
  'h:mm:ss am/pm',
  'HH:mm',
  'HH:mm:ss',
];
const FIELDS: readonly [string, string][] = [
  ['PAGE', 'Page number'],
  ['NUMPAGES', 'Number of pages'],
  ['DATE \\@ "d MMMM yyyy"', 'Date'],
  ['TIME \\@ "HH:mm"', 'Time'],
  ['AUTHOR', 'Author'],
  ['TITLE', 'Title'],
  ['FILENAME', 'File name'],
  ['SECTION', 'Section number'],
];

export function DateTimeDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const [format, setFormat] = useState(DATE_FORMATS[0] ?? 'M/d/yyyy');
  const [auto, setAuto] = useState(false);
  const [field, setField] = useState(FIELDS[0]?.[0] ?? 'PAGE');
  const isField = tab === 'field';
  return (
    <WriteDialogFrame
      model={model}
      title={isField ? 'Field' : 'Date and Time'}
      testId={isField ? 'write-field-dialog' : 'write-date-dialog'}
      onOk={async () =>
        isField
          ? !(await model.run('write.insert.field', { code: field }))
          : !(await model.run('write.insert.date_time', { format, as_field: auto }))
      }
    >
      {isField ? (
        <Radio name="field" label="Insert a field" value={field} options={FIELDS} onChange={setField} />
      ) : (
        <>
          <Radio
            name="date-format"
            label="Available formats"
            value={format}
            options={DATE_FORMATS.map((f) => [f, f] as const)}
            onChange={setFormat}
          />
          <Check label="Update automatically" checked={auto} onChange={setAuto} />
        </>
      )}
    </WriteDialogFrame>
  );
}

export function WordCountDialog({ model }: { model: WriteEditorModel }) {
  const [notes, setNotes] = useState(false);
  const [info, setInfo] = useState<WordCountInfo | null>(null);
  useEffect(() => {
    let live = true;
    void model.query({ WordCount: { include_notes: notes } }).then((r) => {
      if (live && r && 'WordCount' in r) setInfo(r.WordCount);
    });
    return () => {
      live = false;
    };
  }, [model, notes]);
  const rows: [string, number | undefined][] = [
    ['Pages', info?.pages],
    ['Words', info?.words],
    ['Characters (no spaces)', info?.characters],
    ['Characters (with spaces)', info?.characters_with_spaces],
    ['Paragraphs', info?.paragraphs],
    ['Lines', info?.lines],
  ];
  return (
    <WriteDialogFrame model={model} title="Word Count" size="sm" testId="write-word-count-dialog">
      {info?.selection ? <p className="kiln-write-field__hint">Statistics of the selection</p> : null}
      <table className="kiln-write-stats">
        <tbody>
          {rows.map(([label, v]) => (
            <tr key={label}>
              <th scope="row">{label}</th>
              <td>{v === undefined ? '…' : v.toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <Check label="Include textboxes, footnotes and endnotes" checked={notes} onChange={setNotes} />
    </WriteDialogFrame>
  );
}

/** Reads the system clipboard where the browser allows it (otherwise the core asks the host). */
async function readClipboard(): Promise<ClipboardData | null> {
  try {
    if (typeof navigator === 'undefined' || !navigator.clipboard) return null;
    if (navigator.clipboard.read) {
      const items = await navigator.clipboard.read();
      let text = '';
      let html: string | null = null;
      for (const item of items) {
        if (item.types.includes('text/plain')) text = await (await item.getType('text/plain')).text();
        if (item.types.includes('text/html')) html = await (await item.getType('text/html')).text();
      }
      return { text, html, kiln: null };
    }
    return { text: await navigator.clipboard.readText(), html: null, kiln: null };
  } catch {
    return null;
  }
}

export function PasteSpecialDialog({ model }: { model: WriteEditorModel }) {
  const [mode, setMode] = useState<PasteMode>('KeepSource');
  return (
    <WriteDialogFrame
      model={model}
      title="Paste Special"
      testId="write-paste-special-dialog"
      onOk={async () => !(await model.run('suite.edit.paste_as', { mode, data: await readClipboard() }))}
    >
      <Radio
        name="paste-as"
        label="As"
        value={mode}
        options={[
          ['KeepSource', 'Kiln formatted text'],
          ['Html', 'HTML Format'],
          ['Merge', 'Text matching the destination'],
          ['TextOnly', 'Unformatted Text'],
        ]}
        onChange={setMode}
      />
    </WriteDialogFrame>
  );
}

export function BookmarkDialog({ model }: { model: WriteEditorModel }) {
  const [name, setName] = useState('');
  const [names, setNames] = useState<string[]>([]);
  const notice = useNotice(model);
  useEffect(() => {
    model.set({ notice: null });
    void model.query('Bookmarks').then((r) => {
      if (r && 'Bookmarks' in r) setNames(r.Bookmarks);
    });
  }, [model]);
  const valid = /^\p{L}[\p{L}\p{N}_]{0,39}$/u.test(name);
  return (
    <WriteDialogFrame
      model={model}
      title="Bookmark"
      size="sm"
      okLabel="Add"
      okDisabled={!valid}
      testId="write-bookmark-dialog"
      onOk={async () => !(await model.run('write.insert.bookmark', { name }))}
      footerStart={
        <Button
          disabled={!names.includes(name)}
          onClick={async () => {
            if (await model.run('write.find.go_to', { target: { Bookmark: { name } } })) model.closeDialog();
          }}
        >
          Go To
        </Button>
      }
    >
      <Field label="Bookmark name" hint="Start with a letter; use letters, digits and _">
        {(id) => <TextInput id={id} value={name} onChange={setName} autoFocus />}
      </Field>
      {names.length ? (
        <ul className="kiln-write-tabs-list" aria-label="Bookmarks">
          {names.map((n) => (
            <li key={n}>
              <button type="button" aria-pressed={n === name} onClick={() => setName(n)}>
                {n}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {notice ? (
        <p className="kiln-write-status" role="alert">
          {notice}
        </p>
      ) : null}
    </WriteDialogFrame>
  );
}

const EMPTY_FONT: FontFormat = {
  family: null,
  size: null,
  bold: null,
  italic: null,
  underline: null,
  underline_none: false,
  underline_color: null,
  color: null,
  strike: null,
  double_strike: null,
  superscript: null,
  subscript: null,
  small_caps: null,
  all_caps: null,
  hidden: null,
  spacing: null,
  scale: null,
  position: null,
  kerning: null,
};

/** Create a Style from the selection's formatting, or Modify Style. */
export function StyleDialog({ model, styleId }: { model: WriteEditorModel; styleId?: string }) {
  const styles = useWrite(model, (s) => s.styles) ?? [];
  const current = styles.find((s) => s.id === styleId);
  const [name, setName] = useState(current?.name ?? '');
  const [basedOn, setBasedOn] = useState(current?.based_on ?? '');
  const [next, setNext] = useState(current?.next ?? '');
  const [family, setFamily] = useState(current?.font ?? '');
  const [size, setSize] = useState(String(current?.size ?? ''));
  const [bold, setBold] = useState(current?.bold ?? false);
  const [italic, setItalic] = useState(current?.italic ?? false);
  const [align, setAlign] = useState<TextAlign | ''>('');
  const [quick, setQuick] = useState(true);
  const notice = useNotice(model);
  useEffect(() => {
    model.set({ notice: null });
    void model.query('Styles').then((r) => {
      if (r && 'Styles' in r) model.store.setState({ styles: r.Styles });
    });
  }, [model]);
  const paraStyles = styles.filter((s) => s.kind !== 'Character');
  const ok = async () => {
    if (!current) return !(await model.run('write.styles.new_from_selection', { name: name.trim() }));
    const sizeNum = Number(size);
    const spec: StyleSpec = {
      name: name.trim() || null,
      based_on: basedOn,
      next: next || null,
      font: {
        ...EMPTY_FONT,
        family: family.trim() || null,
        size: Number.isFinite(sizeNum) && sizeNum > 0 ? sizeNum : null,
        bold,
        italic,
      },
      paragraph: { ...EMPTY_PARAGRAPH, alignment: align || null },
      quick,
    };
    return !(await model.run('write.styles.modify', { style_id: current.id, spec }));
  };
  return (
    <WriteDialogFrame
      model={model}
      title={current ? 'Modify Style' : 'Create New Style from Formatting'}
      testId="write-style-dialog"
      okDisabled={!name.trim()}
      onOk={ok}
    >
      <Field label="Name">{(id) => <TextInput id={id} value={name} onChange={setName} autoFocus />}</Field>
      {current ? (
        <>
          <Grid cols={2}>
            <Field label="Style based on">
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={basedOn || '(none)'}
                  onValueChange={(v) => setBasedOn(v === '(none)' ? '' : v)}
                  options={[
                    { value: '(none)', label: '(no style)' },
                    ...paraStyles
                      .filter((s) => s.id !== current.id)
                      .map((s) => ({ value: s.id, label: s.name })),
                  ]}
                />
              )}
            </Field>
            <Field label="Style for following paragraph">
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={next || current.id}
                  onValueChange={setNext}
                  options={paraStyles.map((s) => ({ value: s.id, label: s.name }))}
                />
              )}
            </Field>
            <Field label="Font">{(id) => <TextInput id={id} value={family} onChange={setFamily} />}</Field>
            <Field label="Size (pt)">{(id) => <TextInput id={id} value={size} onChange={setSize} />}</Field>
          </Grid>
          <div className="kiln-write-inline">
            <Check label="Bold" checked={bold} onChange={setBold} />
            <Check label="Italic" checked={italic} onChange={setItalic} />
            <Select
              aria-label="Alignment"
              value={align || 'keep'}
              onValueChange={(v) => setAlign(v === 'keep' ? '' : (v as TextAlign))}
              options={[
                { value: 'keep', label: 'Alignment as is' },
                { value: 'Left', label: 'Left' },
                { value: 'Center', label: 'Centered' },
                { value: 'Right', label: 'Right' },
                { value: 'Justify', label: 'Justified' },
              ]}
            />
          </div>
          <Check label="Add to the Styles gallery" checked={quick} onChange={setQuick} />
        </>
      ) : (
        <p className="kiln-write-field__hint">The new style takes the formatting of the selection.</p>
      )}
      {notice ? (
        <p className="kiln-write-status" role="alert">
          {notice}
        </p>
      ) : null}
    </WriteDialogFrame>
  );
}
