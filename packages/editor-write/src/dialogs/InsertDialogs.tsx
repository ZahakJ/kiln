/**
 * Insert ▸ Link, Symbol, Date & Time / Field, Bookmark; Review ▸ Word Count; Paste Special; New
 * Style and Modify Style. Each runs one core command; what the core rejects (a bad address, a
 * duplicate style name) comes back as its message.
 */
import { formatNumber, msg, t, tx } from '@kiln/i18n';
import type {
  ClipboardData,
  FontFormat,
  PasteMode,
  StyleSpec,
  SymbolChar,
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
      title={f?.link ? t('Edit Hyperlink') : t('Insert Hyperlink')}
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
            {t('Remove Link')}
          </Button>
        ) : null
      }
    >
      {collapsed ? (
        <Field label={t('Text to display')}>
          {(id) => (
            <TextInput id={id} value={text} onChange={setText} placeholder={t('The address, if empty')} />
          )}
        </Field>
      ) : null}
      <Field label={t('Address')} hint={t('A web address, an e-mail (mailto:) or a place in this document')}>
        {(id) => <TextInput id={id} value={url} onChange={setUrl} autoFocus />}
      </Field>
      {bookmarks.length ? (
        <Field label={t('Place in this document')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value=""
              placeholder={t('Choose a bookmark')}
              onValueChange={(b) => setUrl(`#${b}`)}
              options={bookmarks.map((b) => ({ value: b, label: b }))}
            />
          )}
        </Field>
      ) : null}
      <Field label={t('ScreenTip')}>{(id) => <TextInput id={id} value={tip} onChange={setTip} />}</Field>
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
/** Word's symbol fonts: their characters are kept as `w:sym`, so Word shows them in the font. */
const SYMBOL_FONTS = ['Symbol', 'Wingdings', 'Wingdings 2', 'Wingdings 3', 'Webdings'] as const;
const NORMAL = '(normal text)';
/** The subsets' names (the keys of `SYMBOLS`). */
const SUBSETS: Readonly<Record<string, string>> = {
  Common: msg('Common'),
  Arrows: msg('Arrows'),
  Math: msg('Math'),
  Greek: msg('Greek'),
  Currency: msg('Currency'),
  Shapes: msg('Shapes'),
};

/** A recently used symbol: a character, or a symbol font's character code. */
interface RecentSymbol {
  text: string;
  font?: string;
  code?: number;
}

function recentSymbols(): RecentSymbol[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
    if (!Array.isArray(v)) return [];
    return v
      .map((x): RecentSymbol | null =>
        typeof x === 'string'
          ? { text: x }
          : typeof x === 'object' && x !== null && typeof x.text === 'string'
            ? {
                text: x.text,
                ...(typeof x.font === 'string' ? { font: x.font } : {}),
                ...(typeof x.code === 'number' ? { code: x.code } : {}),
              }
            : null,
      )
      .filter((x): x is RecentSymbol => x !== null)
      .slice(0, 20);
  } catch {
    return [];
  }
}

export function SymbolDialog({ model }: { model: WriteEditorModel }) {
  const [font, setFont] = useState<string>(NORMAL);
  const [set, setSet] = useState('Common');
  const [code, setCode] = useState('');
  const [recent, setRecent] = useState(recentSymbols);
  const [table, setTable] = useState<SymbolChar[]>([]);
  useEffect(() => {
    if (font === NORMAL) return;
    let live = true;
    void model.query({ SymbolTable: { font } }).then((r) => {
      if (live && r && 'SymbolTable' in r) setTable(r.SymbolTable);
    });
    return () => {
      live = false;
    };
  }, [model, font]);
  const remember = (s: RecentSymbol) => {
    const next = [s, ...recent.filter((r) => r.text !== s.text || r.font !== s.font)].slice(0, 20);
    setRecent(next);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    } catch {
      // ignore
    }
  };
  const insert = async (s: RecentSymbol) => {
    const ok =
      s.font && s.code !== undefined
        ? await model.run('write.insert.symbol_char', { font: s.font, code: s.code })
        : await model.run('write.edit.insert_text', { text: s.text });
    if (ok) remember(s);
  };
  const fromCode = useMemo(() => {
    const n = Number.parseInt(code.replace(/^(U\+|0x)/i, ''), 16);
    return Number.isFinite(n) && n > 31 && n < 0x110000 && !(n >= 0xd800 && n < 0xe000)
      ? String.fromCodePoint(n)
      : null;
  }, [code]);
  const grid = (items: readonly RecentSymbol[], label: string) => (
    <fieldset className="kiln-write-symbols" aria-label={label}>
      {items.map((it) => (
        <button
          key={`${it.font ?? ''}:${it.code ?? it.text}`}
          type="button"
          className="kiln-write-symbol"
          aria-label={
            it.font
              ? t('Insert {text} ({font} {code})', {
                  text: it.text,
                  font: it.font,
                  code: it.code?.toString(16).toUpperCase() ?? '',
                })
              : t('Insert {text}', { text: it.text })
          }
          title={
            it.font
              ? `${it.font}: ${it.code?.toString(16).toUpperCase()}`
              : t('U+{code}', {
                  code: (it.text.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0'),
                })
          }
          onClick={() => void insert(it)}
        >
          {it.text}
        </button>
      ))}
    </fieldset>
  );
  const chars: RecentSymbol[] =
    font === NORMAL
      ? [...(SYMBOLS[set] ?? '')].map((text) => ({ text }))
      : table.map((c) => ({ text: c.text, font, code: c.code }));
  return (
    <WriteDialogFrame model={model} title={t('Symbol')} size="lg" testId="write-symbol-dialog">
      <Grid cols={3}>
        <Field label={t('Font')}>
          {(id) => (
            <Select
              labelId={`${id}-label`}
              value={font}
              onValueChange={setFont}
              options={[NORMAL, ...SYMBOL_FONTS].map((f) => ({
                value: f,
                label: f === NORMAL ? t('(normal text)') : f,
              }))}
            />
          )}
        </Field>
        {font === NORMAL ? (
          <Field label={t('Subset')}>
            {(id) => (
              <Select
                labelId={`${id}-label`}
                value={set}
                onValueChange={setSet}
                options={Object.keys(SYMBOLS).map((s) => ({ value: s, label: tx(SUBSETS[s] ?? s) }))}
              />
            )}
          </Field>
        ) : (
          <span />
        )}
        <Field label={t('Character code (hex)')} hint={t('Or type the code in the document and press Alt+X')}>
          {(id) => (
            <div className="kiln-write-inline">
              <TextInput id={id} value={code} onChange={setCode} placeholder="2603" />
              <Button disabled={!fromCode} onClick={() => fromCode && void insert({ text: fromCode })}>
                {t('Insert')} {fromCode ?? ''}
              </Button>
            </div>
          )}
        </Field>
      </Grid>
      {grid(chars, font === NORMAL ? tx(SUBSETS[set] ?? set) : font)}
      {recent.length ? (
        <Section title={t('Recently used symbols')}>{grid(recent, t('Recently used'))}</Section>
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
  ['PAGE', msg('Page number')],
  ['NUMPAGES', msg('Number of pages')],
  ['DATE \\@ "d MMMM yyyy"', msg('Date')],
  ['TIME \\@ "HH:mm"', msg('Time')],
  ['AUTHOR', msg('Author')],
  ['TITLE', msg('Title')],
  ['FILENAME', msg('File name')],
  ['SECTION', msg('Section number')],
];

export function DateTimeDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const [format, setFormat] = useState(DATE_FORMATS[0] ?? 'M/d/yyyy');
  const [auto, setAuto] = useState(false);
  const [field, setField] = useState(FIELDS[0]?.[0] ?? 'PAGE');
  const isField = tab === 'field';
  return (
    <WriteDialogFrame
      model={model}
      title={isField ? t('Field') : t('Date and Time')}
      testId={isField ? 'write-field-dialog' : 'write-date-dialog'}
      onOk={async () =>
        isField
          ? !(await model.run('write.insert.field', { code: field }))
          : !(await model.run('write.insert.date_time', { format, as_field: auto }))
      }
    >
      {isField ? (
        <Radio name="field" label={t('Insert a field')} value={field} options={FIELDS} onChange={setField} />
      ) : (
        <>
          <Radio
            name="date-format"
            label={t('Available formats')}
            value={format}
            options={DATE_FORMATS.map((f) => [f, f] as const)}
            onChange={setFormat}
          />
          <Check label={t('Update automatically')} checked={auto} onChange={setAuto} />
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
    [t('Pages'), info?.pages],
    [t('Words'), info?.words],
    [t('Characters (no spaces)'), info?.characters],
    [t('Characters (with spaces)'), info?.characters_with_spaces],
    [t('Paragraphs'), info?.paragraphs],
    [t('Lines'), info?.lines],
  ];
  return (
    <WriteDialogFrame model={model} title={t('Word Count')} size="sm" testId="write-word-count-dialog">
      {info?.selection ? <p className="kiln-write-field__hint">{t('Statistics of the selection')}</p> : null}
      <table className="kiln-write-stats">
        <tbody>
          {rows.map(([label, v]) => (
            <tr key={label}>
              <th scope="row">{label}</th>
              <td>{v === undefined ? '…' : formatNumber(v)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <Check label={t('Include textboxes, footnotes and endnotes')} checked={notes} onChange={setNotes} />
    </WriteDialogFrame>
  );
}

/** Reads the system clipboard where the browser allows it (otherwise the core asks the host). */
/** What the system clipboard holds: text and HTML, and a picture when there is one. */
interface ClipboardRead {
  data: ClipboardData | null;
  picture: { type: string; bytes: Uint8Array } | null;
}

async function readClipboard(): Promise<ClipboardRead> {
  try {
    if (typeof navigator === 'undefined' || !navigator.clipboard) return { data: null, picture: null };
    if (navigator.clipboard.read) {
      const items = await navigator.clipboard.read();
      let text = '';
      let html: string | null = null;
      let picture: ClipboardRead['picture'] = null;
      for (const item of items) {
        if (item.types.includes('text/plain')) text = await (await item.getType('text/plain')).text();
        if (item.types.includes('text/html')) html = await (await item.getType('text/html')).text();
        const image = item.types.find((t) => t.startsWith('image/'));
        if (image && !picture) {
          picture = { type: image, bytes: new Uint8Array(await (await item.getType(image)).arrayBuffer()) };
        }
      }
      return { data: { text, html, kiln: null }, picture };
    }
    return { data: { text: await navigator.clipboard.readText(), html: null, kiln: null }, picture: null };
  } catch {
    return { data: null, picture: null };
  }
}

type PasteChoice = PasteMode | 'Picture' | 'Markdown';

/** Paste Special (Ctrl+Alt+V): Kiln's formatted text, HTML, unformatted text, a picture, Markdown. */
export function PasteSpecialDialog({ model }: { model: WriteEditorModel }) {
  const [mode, setMode] = useState<PasteChoice>('KeepSource');
  const [clip, setClip] = useState<ClipboardRead | null>(null);
  useEffect(() => {
    void readClipboard().then((c) => {
      setClip(c);
      // Only a picture on the clipboard: paste it as one.
      if (c.picture && !c.data?.text && !c.data?.html) setMode('Picture');
    });
  }, []);
  const hasText = !!clip?.data && (clip.data.text !== '' || !!clip.data.html);
  const options: (readonly [PasteChoice, string])[] = [
    ...(hasText || !clip
      ? ([
          ['KeepSource', t('Kiln formatted text')],
          ['Html', t('HTML Format')],
          ['Merge', t('Text matching the destination')],
          ['TextOnly', t('Unformatted Unicode Text')],
          ['Markdown', t('Markdown (as styled text)')],
        ] as const)
      : []),
    ...(clip?.picture
      ? ([['Picture', `Picture (${clip.picture.type.replace('image/', '').toUpperCase()})`]] as const)
      : []),
  ];
  return (
    <WriteDialogFrame
      model={model}
      title={t('Paste Special')}
      testId="write-paste-special-dialog"
      onOk={async () => {
        const c = clip ?? (await readClipboard());
        if (mode === 'Picture') {
          if (!c.picture) return true;
          const ext = c.picture.type.split('/')[1]?.replace('jpeg', 'jpg') ?? 'png';
          return !(await model.run('write.insert.picture', {
            name: `Pasted picture.${ext}`,
            bytes: Array.from(c.picture.bytes),
            floating: false,
          }));
        }
        if (mode === 'Markdown')
          return !(await model.run('write.insert.paste_markdown', { text: c.data?.text ?? '' }));
        return !(await model.run('suite.edit.paste_as', { mode, data: c.data }));
      }}
    >
      {clip && options.length === 0 ? (
        <p className="kiln-write-status">{t('The clipboard is empty.')}</p>
      ) : (
        <Radio name="paste-as" label={t('As')} value={mode} options={options} onChange={setMode} />
      )}
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
      title={t('Bookmark')}
      size="sm"
      okLabel={t('Add')}
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
          {t('Go To')}
        </Button>
      }
    >
      <Field label={t('Bookmark name')} hint={t('Start with a letter; use letters, digits and _')}>
        {(id) => <TextInput id={id} value={name} onChange={setName} autoFocus />}
      </Field>
      {names.length ? (
        <ul className="kiln-write-tabs-list" aria-label={t('Bookmarks')}>
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
      title={current ? t('Modify Style') : t('Create New Style from Formatting')}
      testId="write-style-dialog"
      okDisabled={!name.trim()}
      onOk={ok}
    >
      <Field label={t('Name')}>
        {(id) => <TextInput id={id} value={name} onChange={setName} autoFocus />}
      </Field>
      {current ? (
        <>
          <Grid cols={2}>
            <Field label={t('Style based on')}>
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={basedOn || '(none)'}
                  onValueChange={(v) => setBasedOn(v === '(none)' ? '' : v)}
                  options={[
                    { value: '(none)', label: t('(no style)') },
                    ...paraStyles
                      .filter((s) => s.id !== current.id)
                      .map((s) => ({ value: s.id, label: s.name })),
                  ]}
                />
              )}
            </Field>
            <Field label={t('Style for following paragraph')}>
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={next || current.id}
                  onValueChange={setNext}
                  options={paraStyles.map((s) => ({ value: s.id, label: s.name }))}
                />
              )}
            </Field>
            <Field label={t('Font')}>
              {(id) => <TextInput id={id} value={family} onChange={setFamily} />}
            </Field>
            <Field label={t('Size (pt)')}>
              {(id) => <TextInput id={id} value={size} onChange={setSize} />}
            </Field>
          </Grid>
          <div className="kiln-write-inline">
            <Check label={t('Bold')} checked={bold} onChange={setBold} />
            <Check label={t('Italic')} checked={italic} onChange={setItalic} />
            <Select
              aria-label={t('Alignment')}
              value={align || 'keep'}
              onValueChange={(v) => setAlign(v === 'keep' ? '' : (v as TextAlign))}
              options={[
                { value: 'keep', label: t('Alignment as is') },
                { value: 'Left', label: t('Left') },
                { value: 'Center', label: t('Centered') },
                { value: 'Right', label: t('Right') },
                { value: 'Justify', label: t('Justified') },
              ]}
            />
          </div>
          <Check label={t('Add to the Styles gallery')} checked={quick} onChange={setQuick} />
        </>
      ) : (
        <p className="kiln-write-field__hint">{t('The new style takes the formatting of the selection.')}</p>
      )}
      {notice ? (
        <p className="kiln-write-status" role="alert">
          {notice}
        </p>
      ) : null}
    </WriteDialogFrame>
  );
}
