/**
 * Data dialogs: Data Validation (Settings, Input Message, Error Alert), Create Table and Resize
 * Table, Remove Duplicates, Text to Columns (delimited or fixed width, with a preview), and the
 * import dialog of From Text/CSV and From JSON (file origin, delimiter, data type detection, a
 * preview of the first rows, load to a new sheet or the active cell).
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type {
  GridCompare,
  GridDataRange,
  GridErrorStyle,
  GridImport,
  GridImportFormat,
  GridImportPreview,
  GridValidation,
  GridValidationKind,
} from '@kiln/protocol';
import { errorText, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useEffect, useMemo, useState } from 'react';
import type { GridFeatures } from '../model.ts';
import { selectionRanges } from '../model.ts';
import { Check, Choice, columnLabel, FeatureFrame, rangeA1, TextField } from '../ui.tsx';

const KINDS: readonly (readonly [GridValidationKind, string])[] = [
  ['Any', msg('Any value')],
  ['Whole', msg('Whole number')],
  ['Decimal', msg('Decimal')],
  ['List', msg('List')],
  ['Date', msg('Date')],
  ['Time', msg('Time')],
  ['TextLength', msg('Text length')],
  ['Custom', msg('Custom')],
];

const OPS: readonly (readonly [GridCompare, string])[] = [
  ['Between', 'between'],
  ['NotBetween', msg('not between')],
  ['Equal', msg('equal to')],
  ['NotEqual', msg('not equal to')],
  ['GreaterThan', msg('greater than')],
  ['LessThan', msg('less than')],
  ['GreaterThanOrEqual', msg('greater than or equal to')],
  ['LessThanOrEqual', msg('less than or equal to')],
];

const EMPTY_VALIDATION: GridValidation = {
  kind: 'Any',
  op: 'Between',
  formula1: '',
  formula2: '',
  allow_blank: true,
  dropdown: true,
  show_input: true,
  input_title: '',
  input: '',
  show_error: true,
  style: 'Stop',
  error_title: '',
  error: '',
};

export function ValidationDialog({ features }: { features: GridFeatures }) {
  const [v, setV] = useState<GridValidation>(EMPTY_VALIDATION);
  useEffect(() => {
    void features.ask('Validation').then((r) => {
      if (r && 'Validation' in r && r.Validation) setV(r.Validation);
    });
  }, [features]);
  const set = (p: Partial<GridValidation>) => setV((x) => ({ ...x, ...p }));
  const two = (v.op === 'Between' || v.op === 'NotBetween') && !['Any', 'List', 'Custom'].includes(v.kind);
  const label1 =
    v.kind === 'List'
      ? 'Source'
      : v.kind === 'Custom'
        ? 'Formula'
        : two
          ? 'Minimum'
          : v.op === 'Equal' || v.op === 'NotEqual'
            ? 'Value'
            : v.op.startsWith('Greater')
              ? 'Minimum'
              : 'Maximum';
  return (
    <FeatureFrame
      features={features}
      title={t('Data Validation')}
      size="md"
      testId="grid-validation"
      footerStart={
        <button type="button" className="kiln-gfx__link" onClick={() => setV(EMPTY_VALIDATION)}>
          {t('Clear All')}
        </button>
      }
      onOk={() =>
        features.tryExec({
          SetValidation: {
            rule: v.kind === 'Any' && !v.input && !v.input_title ? null : v,
          },
        })
      }
    >
      <Tabs defaultValue="settings">
        <TabsList aria-label={t('Data Validation')}>
          <TabsTrigger value="settings">{t('Settings')}</TabsTrigger>
          <TabsTrigger value="input">{t('Input Message')}</TabsTrigger>
          <TabsTrigger value="error">{t('Error Alert')}</TabsTrigger>
        </TabsList>
        <TabsContent value="settings" className="kiln-gfx__tab">
          <div className="kiln-gfx__row">
            <Choice
              label={t('Allow')}
              value={v.kind}
              options={txPairs(KINDS)}
              width={170}
              onChange={(kind) => set({ kind })}
            />
            <Check
              label={t('Ignore blank')}
              checked={v.allow_blank}
              onChange={(allow_blank) => set({ allow_blank })}
            />
            {v.kind === 'List' ? (
              <Check
                label={t('In-cell dropdown')}
                checked={v.dropdown}
                onChange={(dropdown) => set({ dropdown })}
              />
            ) : null}
          </div>
          {!['Any', 'List', 'Custom'].includes(v.kind) ? (
            <Choice
              label={t('Data')}
              value={v.op}
              options={txPairs(OPS)}
              width={220}
              onChange={(op) => set({ op })}
            />
          ) : null}
          {v.kind !== 'Any' ? (
            <TextField
              label={label1}
              value={v.formula1}
              wide
              testId="grid-validation-source"
              placeholder={
                v.kind === 'List' ? t('Low,Medium,High or =$A$1:$A$5') : v.kind === 'Date' ? '1/1/2026' : ''
              }
              onChange={(formula1) => set({ formula1 })}
            />
          ) : null}
          {two ? (
            <TextField
              label={t('Maximum')}
              value={v.formula2}
              wide
              onChange={(formula2) => set({ formula2 })}
            />
          ) : null}
        </TabsContent>
        <TabsContent value="input" className="kiln-gfx__tab">
          <Check
            label={t('Show input message when cell is selected')}
            checked={v.show_input}
            onChange={(show_input) => set({ show_input })}
          />
          <TextField
            label={t('Title')}
            value={v.input_title}
            wide
            onChange={(input_title) => set({ input_title })}
          />
          <Field2 label={t('Input message')} value={v.input} onChange={(input) => set({ input })} />
        </TabsContent>
        <TabsContent value="error" className="kiln-gfx__tab">
          <Check
            label={t('Show error alert after invalid data is entered')}
            checked={v.show_error}
            onChange={(show_error) => set({ show_error })}
          />
          <Choice<GridErrorStyle>
            label={t('Style')}
            value={v.style}
            width={160}
            options={[
              ['Stop', t('Stop')],
              ['Warning', t('Warning')],
              ['Information', t('Information')],
            ]}
            onChange={(style) => set({ style })}
          />
          <TextField
            label={t('Title')}
            value={v.error_title}
            wide
            onChange={(error_title) => set({ error_title })}
          />
          <Field2 label={t('Error message')} value={v.error} onChange={(error) => set({ error })} />
        </TabsContent>
      </Tabs>
    </FeatureFrame>
  );
}

function Field2({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="kiln-griddlg__field kiln-gfx__wide">
      <span className="kiln-griddlg__label">{label}</span>
      <textarea
        className="kiln-griddlg__input kiln-gfx__area"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}

function useDataRange(features: GridFeatures): GridDataRange | null {
  const [info, setInfo] = useState<GridDataRange | null>(null);
  useEffect(() => {
    void features.ask('DataRange').then((r) => {
      if (r && 'DataRange' in r) setInfo(r.DataRange);
    });
  }, [features]);
  return info;
}

/** Parses `$A$1:$D$9` (or `A1:D9`). */
export function parseRange(text: string): { r0: number; c0: number; r1: number; c1: number } | null {
  const m = text
    .trim()
    .replace(/^=/, '')
    .replace(/^.*!/, '')
    .toUpperCase()
    .match(/^\$?([A-Z]{1,3})\$?(\d+)(?::\$?([A-Z]{1,3})\$?(\d+))?$/);
  if (!m) return null;
  const col = (s: string) => [...s].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0) - 1;
  const a = { r: Number(m[2]) - 1, c: col(m[1] ?? 'A') };
  const b = m[3] ? { r: Number(m[4]) - 1, c: col(m[3]) } : a;
  return { r0: Math.min(a.r, b.r), c0: Math.min(a.c, b.c), r1: Math.max(a.r, b.r), c1: Math.max(a.c, b.c) };
}

/** Insert ▸ Table and Format as Table: where is the data, does it have headers. */
export function CreateTableDialog({ features, style }: { features: GridFeatures; style: string }) {
  const info = useDataRange(features);
  const [text, setText] = useState('');
  const [header, setHeader] = useState(true);
  useEffect(() => {
    if (info) {
      setText(`=${rangeA1(info.range)}`);
      setHeader(info.header);
    }
  }, [info]);
  const range = parseRange(text);
  return (
    <FeatureFrame
      features={features}
      title={style ? t('Format As Table') : t('Create Table')}
      testId="grid-create-table"
      okDisabled={!range}
      onOk={() => features.tryExec({ CreateTable: { range, header, style: style || 'TableStyleMedium2' } })}
    >
      <TextField
        label={t('Where is the data for your table?')}
        value={text}
        onChange={setText}
        autoFocus
        wide
      />
      <Check
        label={t('My table has headers')}
        checked={header}
        onChange={setHeader}
        testId="grid-table-headers"
      />
    </FeatureFrame>
  );
}

export function ResizeTableDialog({ features }: { features: GridFeatures }) {
  const table = features.model.state.grid?.features.table;
  const [text, setText] = useState(table ? `=${table.reference}` : '');
  const range = parseRange(text);
  return (
    <FeatureFrame
      features={features}
      title={t('Resize Table')}
      testId="grid-resize-table"
      okDisabled={!range}
      onOk={() => (range ? features.tryExec({ ResizeTable: { range } }) : undefined)}
    >
      <TextField
        label={t('Select the new data range for your table')}
        value={text}
        onChange={setText}
        autoFocus
        wide
      />
      <p className="kiln-gfx__hint">
        {t('The headers must remain in the same row, and the new range must overlap the table.')}
      </p>
    </FeatureFrame>
  );
}

export function RemoveDuplicatesDialog({ features }: { features: GridFeatures }) {
  const info = useDataRange(features);
  const [header, setHeader] = useState(true);
  const [cols, setCols] = useState<Set<number>>(new Set());
  useEffect(() => {
    if (!info) return;
    setHeader(info.header);
    const all = new Set<number>();
    for (let c = info.range.c0; c <= info.range.c1; c++) all.add(c);
    setCols(all);
  }, [info]);
  if (!info)
    return (
      <FeatureFrame features={features} title={t('Remove Duplicates')}>
        {null}
      </FeatureFrame>
    );
  const list: number[] = [];
  for (let c = info.range.c0; c <= info.range.c1 && list.length < 200; c++) list.push(c);
  return (
    <FeatureFrame
      features={features}
      title={t('Remove Duplicates')}
      size="md"
      testId="grid-remove-duplicates"
      okDisabled={cols.size === 0}
      onOk={() =>
        features.tryExec({ RemoveDuplicates: { columns: [...cols].sort((a, b) => a - b), header } })
      }
    >
      <p className="kiln-gfx__prompt">
        {t('To delete duplicate values, select one or more columns that contain duplicates.')}
      </p>
      <div className="kiln-gfx__row">
        <button type="button" className="kiln-gfx__link" onClick={() => setCols(new Set(list))}>
          {t('Select All')}
        </button>
        <button type="button" className="kiln-gfx__link" onClick={() => setCols(new Set())}>
          {t('Unselect All')}
        </button>
        <span className="kiln-gfx__grow" />
        <Check label={t('My data has headers')} checked={header} onChange={setHeader} />
      </div>
      <div className="kiln-griddlg__list">
        {list.map((c) => (
          <Check
            key={c}
            label={columnLabel(header ? info.headers[c - info.range.c0] : undefined, c)}
            checked={cols.has(c)}
            onChange={(on) => {
              const n = new Set(cols);
              if (on) n.add(c);
              else n.delete(c);
              setCols(n);
            }}
          />
        ))}
      </div>
    </FeatureFrame>
  );
}

const DELIMS: readonly (readonly [string, string])[] = [
  ['\t', msg('Tab')],
  [';', msg('Semicolon')],
  [',', msg('Comma')],
  [' ', msg('Space')],
];

/** Text to Columns over the selected column. */
export function TextToColumnsDialog({ features }: { features: GridFeatures }) {
  const [delimited, setDelimited] = useState(true);
  const [delims, setDelims] = useState<Set<string>>(new Set(['\t']));
  const [other, setOther] = useState('');
  const [consecutive, setConsecutive] = useState(false);
  const [qualifier, setQualifier] = useState('"');
  const [widths, setWidths] = useState('');
  const [sample, setSample] = useState<string[]>([]);
  const [textCols, setTextCols] = useState<Set<number>>(new Set());
  useEffect(() => {
    void features.model.client.query(features.model.doc, 'SelectedText').then((r) => {
      const t =
        typeof r === 'object' && r !== null && 'Text' in r ? String((r as { Text: string }).Text) : '';
      setSample(
        t
          .split(/\r?\n/)
          .filter((l, i) => l || i === 0)
          .slice(0, 8),
      );
    });
  }, [features]);
  const chars = [...delims].join('') + other;
  const cut = (line: string): string[] => {
    if (!delimited) {
      const at = widths
        .split(/[\s,]+/)
        .map(Number)
        .filter((n) => n > 0)
        .sort((a, b) => a - b);
      const out: string[] = [];
      let last = 0;
      for (const p of at) {
        out.push(line.slice(last, p));
        last = p;
      }
      out.push(line.slice(last));
      return out;
    }
    if (!chars) return [line];
    const parts: string[] = [];
    let cur = '';
    let quoted = false;
    for (const ch of line) {
      if (qualifier && ch === qualifier) {
        quoted = !quoted;
        continue;
      }
      if (!quoted && chars.includes(ch)) {
        if (!(consecutive && cur === '' && parts.length > 0 && parts[parts.length - 1] === ''))
          parts.push(cur);
        cur = '';
        continue;
      }
      cur += ch;
    }
    parts.push(cur);
    return consecutive ? parts.filter((p, i) => p !== '' || i === 0) : parts;
  };
  const rows = useMemo(() => sample.map(cut), [sample, chars, consecutive, qualifier, delimited, widths]); // eslint-disable-line react-hooks/exhaustive-deps
  const width = Math.max(1, ...rows.map((r) => r.length));
  return (
    <FeatureFrame
      features={features}
      title={t('Convert Text to Columns')}
      size="lg"
      testId="grid-text-to-columns"
      okLabel={t('Finish')}
      onOk={() =>
        features.tryExec({
          TextToColumns: {
            options: {
              delimited,
              delimiters: chars,
              consecutive,
              qualifier,
              widths: widths
                .split(/[\s,]+/)
                .map(Number)
                .filter((n) => n > 0),
              text_columns: [...textCols],
              skip_columns: [],
            },
          },
        })
      }
    >
      <div className="kiln-gfx__row">
        <Choice
          label={t('Original data type')}
          value={delimited ? 'd' : 'f'}
          width={180}
          options={[
            ['d', t('Delimited')],
            ['f', t('Fixed width')],
          ]}
          onChange={(v) => setDelimited(v === 'd')}
        />
        {delimited ? (
          <Choice
            label={t('Text qualifier')}
            value={qualifier || 'none'}
            width={110}
            options={[
              ['"', '"'],
              ["'", "'"],
              ['none', '{none}'],
            ]}
            onChange={(v) => setQualifier(v === 'none' ? '' : v)}
          />
        ) : (
          <TextField label={t('Break after characters (e.g. 4, 10)')} value={widths} onChange={setWidths} />
        )}
      </div>
      {delimited ? (
        <div className="kiln-gfx__row">
          {DELIMS.map(([ch, label]) => (
            <Check
              key={tx(label)}
              label={tx(label)}
              checked={delims.has(ch)}
              onChange={(on) => {
                const n = new Set(delims);
                if (on) n.add(ch);
                else n.delete(ch);
                setDelims(n);
              }}
            />
          ))}
          <TextField
            label={t('Other')}
            value={other}
            onChange={(v) => setOther(v.slice(0, 1))}
            testId="grid-ttc-other"
          />
          <Check
            label={t('Treat consecutive delimiters as one')}
            checked={consecutive}
            onChange={setConsecutive}
          />
        </div>
      ) : null}
      <div className="kiln-griddlg__label">
        {t('Data preview (click a column header to import it as text)')}
      </div>
      <div className="kiln-gfx__preview-table">
        <div className="kiln-gfx__preview-row kiln-gfx__preview-head">
          {Array.from({ length: width }, (_, i) => (
            <button
              type="button"
              key={i}
              className={textCols.has(i) ? 'kiln-gfx__col-on' : ''}
              onClick={() => {
                const n = new Set(textCols);
                if (n.has(i)) n.delete(i);
                else n.add(i);
                setTextCols(n);
              }}
            >
              {textCols.has(i) ? t('Text') : t('General')}
            </button>
          ))}
        </div>
        {rows.map((r, i) => (
          <div key={i} className="kiln-gfx__preview-row">
            {Array.from({ length: width }, (_, j) => (
              <span key={j}>{r[j] ?? ''}</span>
            ))}
          </div>
        ))}
      </div>
    </FeatureFrame>
  );
}

/** From Text/CSV and From JSON: the file, its preview and where to load it. */
export function ImportDialog({
  features,
  file,
}: {
  features: GridFeatures;
  file: { name: string; bytes: number[]; format: GridImportFormat };
}) {
  const [delimiter, setDelimiter] = useState<string>('auto');
  const [encoding, setEncoding] = useState<string>('auto');
  const [asText, setAsText] = useState(false);
  const [atCell, setAtCell] = useState(false);
  const [decimalComma, setDecimalComma] = useState(false);
  const [preview, setPreview] = useState<GridImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const spec = (): GridImport => ({
    bytes: file.bytes,
    name: file.name,
    format: file.format,
    delimiter: delimiter === 'auto' ? null : delimiter,
    encoding: encoding === 'auto' ? null : encoding,
    as_text: asText,
    at_cell: atCell,
    decimal_comma: decimalComma,
  });
  useEffect(() => {
    let live = true;
    void features.model.client
      .query(features.model.doc, { Grid: { ImportPreview: { import: spec() } } })
      .then(
        (r) => {
          const g =
            r && typeof r === 'object' && 'Grid' in r
              ? (r as { Grid: { ImportPreview?: GridImportPreview } }).Grid
              : null;
          if (live && g?.ImportPreview) {
            setPreview(g.ImportPreview);
            setError(null);
          }
        },
        (e: unknown) => live && setError(errorText(e)),
      );
    return () => {
      live = false;
    };
  }, [features, delimiter, encoding, asText, decimalComma]); // eslint-disable-line react-hooks/exhaustive-deps
  const target = selectionRanges(features.model).at(-1);
  return (
    <FeatureFrame
      features={features}
      title={file.name}
      size="lg"
      testId="grid-import"
      okLabel={t('Load')}
      okDisabled={!preview}
      onOk={() => features.tryExec({ Import: { import: spec() } })}
    >
      {file.format === 'Text' ? (
        <div className="kiln-gfx__row">
          <Choice
            label={t('File origin')}
            value={encoding}
            width={190}
            options={[
              ['auto', `Detect (${preview?.encoding ?? '…'})`],
              ['utf-8', t('Unicode (UTF-8)')],
              ['utf-16le', t('Unicode (UTF-16 LE)')],
              ['windows-1252', t('Western European (Windows)')],
            ]}
            onChange={setEncoding}
          />
          <Choice
            label={t('Delimiter')}
            value={delimiter}
            width={160}
            options={[
              ['auto', `Detect (${preview?.delimiter === '\\t' ? 'Tab' : (preview?.delimiter ?? '…')})`],
              [',', t('Comma')],
              [';', t('Semicolon')],
              ['\\t', t('Tab')],
              ['|', t('Pipe')],
              [' ', t('Space')],
            ]}
            onChange={setDelimiter}
          />
          <Choice
            label={t('Data type detection')}
            value={asText ? 'none' : 'auto'}
            width={210}
            options={[
              ['auto', t('Based on the values')],
              ['none', t('Do not detect data types')],
            ]}
            onChange={(v) => setAsText(v === 'none')}
          />
          <Choice
            label={t('Numbers')}
            value={decimalComma ? 'comma' : 'point'}
            width={180}
            options={[
              ['point', t('1,234.56 (decimal point)')],
              ['comma', t('1.234,56 (decimal comma)')],
            ]}
            onChange={(v) => setDecimalComma(v === 'comma')}
          />
        </div>
      ) : null}
      {error ? (
        <p className="kiln-gfx__error" role="alert">
          {error}
        </p>
      ) : null}
      {preview ? (
        <>
          <div className="kiln-gfx__preview-table kiln-gfx__preview-import">
            {preview.rows.slice(0, 20).map((r, i) => (
              <div
                key={i}
                className={i === 0 ? 'kiln-gfx__preview-row kiln-gfx__preview-head' : 'kiln-gfx__preview-row'}
              >
                {r.slice(0, 12).map((c, j) => (
                  <span key={j}>{c}</span>
                ))}
              </div>
            ))}
          </div>
          <p className="kiln-gfx__hint">
            {t('grid.importSize', { rows: preview.total_rows, cols: preview.total_cols })}
            {preview.warnings.length ? ` ${preview.warnings[0]}` : ''}
          </p>
        </>
      ) : null}
      <Choice
        label={t('Load to')}
        value={atCell ? 'cell' : 'sheet'}
        width={240}
        options={[
          ['sheet', t('A new sheet')],
          [
            'cell',
            `The active cell${target ? ` (${rangeA1({ ...target, r1: target.r0, c1: target.c0 }, false)})` : ''}`,
          ],
        ]}
        onChange={(v) => setAtCell(v === 'cell')}
      />
    </FeatureFrame>
  );
}
