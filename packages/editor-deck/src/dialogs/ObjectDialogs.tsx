/**
 * Format Shape (fills), Code Block, Split Cells and Cell Margins. Each ends in one core
 * command; the code's colours are the core's (Write's highlighter).
 */
import { t } from '@kiln/i18n';
import { NumberInput, Select } from '@kiln/ui-kit';
import { useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { ColorField, DeckDialogFrame, Field, PATTERNS } from './DeckDialogs.tsx';

type FillKind = 'none' | 'solid' | 'gradient' | 'picture' | 'pattern';

const isFillKind = (k: string): k is FillKind =>
  k === 'none' || k === 'solid' || k === 'gradient' || k === 'picture' || k === 'pattern';

/** Format Shape: no fill, solid, gradient, picture or pattern fill of the selected shapes. */
export function FormatShapeDialog({ model }: { model: DeckEditorModel }) {
  const current = useDeck(model, (s) => s.deck?.format?.fill ?? null);
  // The dialog opens on the selection's own kind of fill, as PowerPoint's pane does.
  const own = useDeck(model, (s) => s.deck?.format?.fill_kind ?? 'solid');
  const [kind, setKind] = useState<FillKind>(() => (isFillKind(own) ? own : 'solid'));
  const [color, setColor] = useState(current ?? 'accent1');
  const [to, setTo] = useState('FFFFFF');
  const [angle, setAngle] = useState(90);
  const [transparency, setTransparency] = useState(0);
  const [pattern, setPattern] = useState('pct25');
  const [file, setFile] = useState<{ name: string; bytes: number[] } | null>(null);
  const apply = () => {
    switch (kind) {
      case 'none':
        return model.run('deck.drawing.shape_fill', { none: true });
      case 'gradient':
        return model.run('deck.drawing.shape_fill', { gradient: { from: color, to, angle } });
      case 'picture':
        return file
          ? model.run('deck.drawing.shape_fill', { name: file.name, bytes: file.bytes })
          : undefined;
      case 'pattern':
        return model.run('deck.drawing.shape_fill', {
          fill: { Pattern: { preset: pattern, fg: color, bg: to } },
        });
      default:
        return model.run('deck.drawing.shape_fill', {
          fill: { Solid: { color, brightness: null, transparency } },
        });
    }
  };
  return (
    <DeckDialogFrame
      model={model}
      title={t('Format Shape')}
      testId="deck-format-shape"
      okLabel={t('Apply')}
      okDisabled={kind === 'picture' && !file}
      onOk={apply}
    >
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('Fill')}</legend>
        {(
          [
            ['none', t('No fill')],
            ['solid', t('Solid fill')],
            ['gradient', t('Gradient fill')],
            ['picture', t('Picture fill')],
            ['pattern', t('Pattern fill')],
          ] as const
        ).map(([k, label]) => (
          <label key={k}>
            <input type="radio" name="shape-fill" checked={kind === k} onChange={() => setKind(k)} />
            {label}
          </label>
        ))}
      </fieldset>
      {kind === 'solid' || kind === 'gradient' || kind === 'pattern' ? (
        <ColorField
          label={kind === 'pattern' ? t('Foreground') : kind === 'gradient' ? t('From') : t('Colour')}
          value={color}
          onChange={setColor}
        />
      ) : null}
      {kind === 'gradient' || kind === 'pattern' ? (
        <ColorField label={kind === 'pattern' ? t('Background') : t('To')} value={to} onChange={setTo} />
      ) : null}
      {kind === 'gradient' ? (
        <Field label={t('Angle')}>
          {(id) => <NumberInput id={id} value={angle} unit="°" min={0} max={359} onValueChange={setAngle} />}
        </Field>
      ) : null}
      {kind === 'solid' ? (
        <Field label={t('Transparency')}>
          {(id) => (
            <NumberInput
              id={id}
              value={transparency}
              unit="%"
              min={0}
              max={100}
              onValueChange={setTransparency}
            />
          )}
        </Field>
      ) : null}
      {kind === 'pattern' ? (
        <Field label={t('Pattern')}>
          {(id) => (
            <Select
              id={id}
              value={pattern}
              onValueChange={setPattern}
              options={PATTERNS.map((p) => ({ value: p, label: p }))}
            />
          )}
        </Field>
      ) : null}
      {kind === 'picture' ? (
        <Field label={t('Picture')}>
          {(id) => (
            <input
              id={id}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/bmp,image/webp"
              onChange={async (e) => {
                const f = e.target.files?.[0];
                if (!f) return;
                setFile({ name: f.name, bytes: Array.from(new Uint8Array(await f.arrayBuffer())) });
              }}
            />
          )}
        </Field>
      ) : null}
    </DeckDialogFrame>
  );
}

const LANGUAGES = [
  'Plain text',
  'Bash',
  'C',
  'C#',
  'C++',
  'CSS',
  'Go',
  'HTML',
  'Java',
  'JavaScript',
  'JSON',
  'Kotlin',
  'Python',
  'Rust',
  'SQL',
  'Swift',
  'TypeScript',
  'YAML',
];

/** Insert ▸ Code Block: the code in a monospaced, highlighted text box. */
export function CodeBlockDialog({ model }: { model: DeckEditorModel }) {
  const [code, setCode] = useState('');
  const [language, setLanguage] = useState('Plain text');
  return (
    <DeckDialogFrame
      model={model}
      title={t('Code Block')}
      testId="deck-code-block"
      size="md"
      okLabel={t('Insert')}
      okDisabled={code.trim() === ''}
      onOk={() =>
        model.core('deck.insert.code_block', {
          code,
          language: language === 'Plain text' ? null : language,
        })
      }
    >
      <Field label={t('Language')}>
        {(id) => (
          <Select
            id={id}
            value={language}
            onValueChange={setLanguage}
            options={LANGUAGES.map((l) => ({ value: l, label: l }))}
          />
        )}
      </Field>
      <textarea
        aria-label={t('Code')}
        className="kiln-deckdlg__code"
        spellCheck={false}
        rows={12}
        value={code}
        onChange={(e) => setCode(e.target.value)}
        onKeyDown={(e) => {
          // Tab indents inside the code instead of leaving the box.
          if (e.key !== 'Tab' || e.shiftKey) return;
          e.preventDefault();
          const t = e.currentTarget;
          const { selectionStart: s, selectionEnd: end } = t;
          setCode(`${code.slice(0, s)}    ${code.slice(end)}`);
          requestAnimationFrame(() => t.setSelectionRange(s + 4, s + 4));
        }}
      />
    </DeckDialogFrame>
  );
}

/** Table Layout ▸ Split Cells: columns and rows the cell becomes. */
export function SplitCellsDialog({ model }: { model: DeckEditorModel }) {
  const [cols, setCols] = useState(2);
  const [rows, setRows] = useState(1);
  return (
    <DeckDialogFrame
      model={model}
      title={t('Split Cells')}
      testId="deck-split-cells"
      onOk={() => model.run('suite.table.split_cells', { rows, cols })}
    >
      <div className="kiln-deckdlg__row">
        <Field label={t('Number of columns')}>
          {(id) => (
            <NumberInput
              id={id}
              value={cols}
              unit=""
              min={1}
              max={20}
              precision={0}
              onValueChange={setCols}
            />
          )}
        </Field>
        <Field label={t('Number of rows')}>
          {(id) => (
            <NumberInput
              id={id}
              value={rows}
              unit=""
              min={1}
              max={20}
              precision={0}
              onValueChange={setRows}
            />
          )}
        </Field>
      </div>
    </DeckDialogFrame>
  );
}

/** Table Layout ▸ Cell Margins ▸ Custom Margins: the selected cells' insets in points. */
export function CellMarginsDialog({ model }: { model: DeckEditorModel }) {
  const [v, setV] = useState({ left: 7.2, right: 7.2, top: 3.6, bottom: 3.6 });
  return (
    <DeckDialogFrame
      model={model}
      title={t('Cell Margins')}
      testId="deck-cell-margins"
      onOk={() => model.core('deck.table.cell_margins', v)}
    >
      <div className="kiln-deckdlg__row">
        {(['left', 'right', 'top', 'bottom'] as const).map((k) => (
          <Field key={k} label={k[0]?.toUpperCase() + k.slice(1)}>
            {(id) => (
              <NumberInput
                id={id}
                value={v[k]}
                unit="pt"
                min={0}
                max={400}
                step={0.1}
                onValueChange={(n) => setV((o) => ({ ...o, [k]: n }))}
              />
            )}
          </Field>
        ))}
      </div>
    </DeckDialogFrame>
  );
}
