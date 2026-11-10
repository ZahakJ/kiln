/**
 * Deck's Font, Paragraph and Symbol dialogs (Home ▸ Font ↘, Home ▸ Paragraph ↘, the context
 * menu, Insert ▸ Symbol). They show the selection's formatting as the core reports it and apply
 * what changed with the shared formatting commands, as one step each.
 */
import { t } from '@kiln/i18n';
import { Button, Combobox, NumberInput, Select } from '@kiln/ui-kit';
import { useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { ColorField, DeckDialogFrame } from './DeckDialogs.tsx';

const FAMILIES = [
  'Figtree',
  'Fraunces',
  'Calibri',
  'Calibri Light',
  'Cambria',
  'Arial',
  'Georgia',
  'Times New Roman',
  'Courier New',
  'Carlito',
  'Caladea',
].map((f) => ({ value: f, label: f, fontFamily: f }));

const SIZES = [
  '8',
  '10',
  '12',
  '14',
  '16',
  '18',
  '20',
  '24',
  '28',
  '32',
  '36',
  '40',
  '44',
  '48',
  '54',
  '60',
  '66',
  '72',
  '80',
  '96',
].map((s) => ({ value: s }));

export function FontDialog({ model }: { model: DeckEditorModel }) {
  const f = useDeck(model, (s) => s.deck?.format ?? null);
  const [family, setFamily] = useState(f?.font_family ?? 'Figtree');
  const [size, setSize] = useState(String(f ? Math.round(f.font_size * 2) / 2 : 18));
  const [style, setStyle] = useState(`${f?.bold ? 'b' : ''}${f?.italic ? 'i' : ''}` || 'r');
  const [underline, setUnderline] = useState(f?.underline ? 'single' : 'none');
  const [color, setColor] = useState(f?.color ?? '000000');
  const [strike, setStrike] = useState(f?.strike ?? false);
  const [sup, setSup] = useState(f?.superscript ?? false);
  const [sub, setSub] = useState(f?.subscript ?? false);
  const [spacing, setSpacing] = useState(f?.spacing ?? 0);
  const sizeNum = Number(size);
  const valid = Number.isFinite(sizeNum) && sizeNum >= 1 && sizeNum <= 4000 && family.trim() !== '';
  const ok = async () => {
    const run = (id: string, args?: unknown) => model.run(id, args);
    if (family.trim() !== f?.font_family) await run('suite.format.font_family', { family: family.trim() });
    if (sizeNum !== f?.font_size) await run('suite.format.font_size', { size: sizeNum });
    if (style.includes('b') !== (f?.bold ?? false)) await run('suite.format.bold');
    if (style.includes('i') !== (f?.italic ?? false)) await run('suite.format.italic');
    if (underline === 'double') await run('suite.format.double_underline');
    else if ((underline === 'single') !== (f?.underline ?? false)) await run('suite.format.underline');
    if (strike !== (f?.strike ?? false)) await run('suite.format.strikethrough');
    if (sup !== (f?.superscript ?? false)) await run('suite.format.superscript');
    if (sub !== (f?.subscript ?? false)) await run('suite.format.subscript');
    if (color !== f?.color)
      await run('suite.format.font_color', { color: { type: 'rgb', hex: `#${color}` } });
    if (Math.abs(spacing - (f?.spacing ?? 0)) > 0.01)
      await run('deck.font.character_spacing', { points: spacing });
  };
  return (
    <DeckDialogFrame
      model={model}
      title={t('Font')}
      testId="deck-font"
      size="md"
      okDisabled={!valid}
      onOk={ok}
    >
      <div className="kiln-deckdlg__row">
        <div className="kiln-deckdlg__field">
          <span>{t('Latin text font')}</span>
          <Combobox
            aria-label={t('Latin text font')}
            value={family}
            onValueChange={setFamily}
            options={FAMILIES}
            allowCustomValue
          />
        </div>
        <div className="kiln-deckdlg__field">
          <span>{t('Font style')}</span>
          <Select
            aria-label={t('Font style')}
            value={style}
            onValueChange={setStyle}
            options={[
              { value: 'r', label: t('Regular') },
              { value: 'i', label: t('Italic') },
              { value: 'b', label: t('Bold') },
              { value: 'bi', label: t('Bold Italic') },
            ]}
          />
        </div>
        <div className="kiln-deckdlg__field">
          <span>{t('Size')}</span>
          <Combobox
            aria-label={t('Size')}
            value={size}
            onValueChange={setSize}
            options={SIZES}
            allowCustomValue
          />
        </div>
      </div>
      <div className="kiln-deckdlg__row">
        <ColorField label={t('Font colour')} value={color} onChange={setColor} />
        <div className="kiln-deckdlg__field">
          <span>{t('Underline style')}</span>
          <Select
            aria-label={t('Underline style')}
            value={underline}
            onValueChange={setUnderline}
            options={[
              { value: 'none', label: t('(none)') },
              { value: 'single', label: t('Single') },
              { value: 'double', label: t('Double') },
            ]}
          />
        </div>
      </div>
      <fieldset className="kiln-deckdlg__radios">
        <legend>{t('Effects')}</legend>
        <label>
          <input type="checkbox" checked={strike} onChange={(e) => setStrike(e.target.checked)} />
          {t('Strikethrough')}
        </label>
        <label>
          <input
            type="checkbox"
            checked={sup}
            onChange={(e) => {
              setSup(e.target.checked);
              if (e.target.checked) setSub(false);
            }}
          />
          {t('Superscript')}
        </label>
        <label>
          <input
            type="checkbox"
            checked={sub}
            onChange={(e) => {
              setSub(e.target.checked);
              if (e.target.checked) setSup(false);
            }}
          />
          {t('Subscript')}
        </label>
      </fieldset>
      <div className="kiln-deckdlg__field">
        <span>{t('Character spacing (points; negative condenses)')}</span>
        <NumberInput
          aria-label={t('Character spacing')}
          value={spacing}
          unit="pt"
          min={-50}
          max={500}
          step={0.5}
          onValueChange={setSpacing}
        />
      </div>
      <p
        className="kiln-deckdlg__hint"
        style={{
          fontFamily: family,
          fontWeight: style.includes('b') ? 700 : 400,
          fontStyle: style.includes('i') ? 'italic' : undefined,
        }}
      >
        {t('The quick brown fox')}
      </p>
    </DeckDialogFrame>
  );
}

const LINE_SPACINGS = ['1', '1.15', '1.5', '2', '2.5', '3'];

export function ParagraphDialog({ model }: { model: DeckEditorModel }) {
  const f = useDeck(model, (s) => s.deck?.format ?? null);
  const [align, setAlign] = useState(f?.align ?? 'left');
  const [lines, setLines] = useState('1');
  const [level, setLevel] = useState(0);
  const ok = async () => {
    if (align !== f?.align) {
      const id =
        align === 'center'
          ? 'suite.format.align_center'
          : align === 'right'
            ? 'suite.format.align_right'
            : align === 'justify'
              ? 'suite.format.justify'
              : 'suite.format.align_left';
      await model.run(id);
    }
    await model.run('suite.format.line_spacing', { lineSpacing: Number(lines) });
    for (let i = 0; i < Math.abs(level); i++)
      await model.run(level > 0 ? 'suite.format.indent' : 'suite.format.outdent');
  };
  return (
    <DeckDialogFrame model={model} title={t('Paragraph')} testId="deck-paragraph" onOk={ok}>
      <div className="kiln-deckdlg__field">
        <span>{t('Alignment')}</span>
        <Select
          aria-label={t('Alignment')}
          value={align}
          onValueChange={setAlign}
          options={[
            { value: 'left', label: t('Left') },
            { value: 'center', label: t('Centered') },
            { value: 'right', label: t('Right') },
            { value: 'justify', label: t('Justified') },
          ]}
        />
      </div>
      <div className="kiln-deckdlg__field">
        <span>{t('Line spacing (multiple)')}</span>
        <Select
          aria-label={t('Line spacing')}
          value={lines}
          onValueChange={setLines}
          options={LINE_SPACINGS.map((v) => ({ value: v, label: v }))}
        />
      </div>
      <div className="kiln-deckdlg__field">
        <span>{t('List level (− outdents, + indents)')}</span>
        <NumberInput
          aria-label={t('List level change')}
          value={level}
          unit=""
          min={-8}
          max={8}
          precision={0}
          onValueChange={setLevel}
        />
      </div>
    </DeckDialogFrame>
  );
}

const SYMBOLS: readonly [string, string][] = [
  ['Common', '©®™°±×÷€£¥¢§¶†‡•…–—‘’“”«»‹›¿¡'],
  ['Arrows', '←↑→↓↔↕⇐⇒⇔↩↪⤴⤵'],
  ['Maths', '≈≠≤≥∞√∑∏∫∂∆∇∈∉∩∪⊂⊃∧∨¬∀∃πµΩαβγδθλσφ'],
  ['Shapes', '■□▪▫▲△▶▷▼▽◆◇○●◎★☆✓✔✗✘☐☑'],
];

/** Fonts the Symbol dialog offers; "(normal text)" keeps the text's own. */
const SYMBOL_FONTS = ['(normal text)', 'Segoe UI Symbol', 'Cambria Math', 'Arial Unicode MS'];

export function SymbolDialog({ model }: { model: DeckEditorModel }) {
  const [set, setSet] = useState(SYMBOLS[0]?.[0] ?? 'Common');
  const [font, setFont] = useState(SYMBOL_FONTS[0] ?? '');
  const [code, setCode] = useState('2192');
  const chars = SYMBOLS.find(([n]) => n === set)?.[1] ?? '';
  const insert = (text: string) =>
    model.core('deck.insert.symbol', { text, font: font === SYMBOL_FONTS[0] ? null : font });
  const cp = Number.parseInt(code, 16);
  const typed = Number.isFinite(cp) && cp > 0x1f && cp <= 0x10ffff ? String.fromCodePoint(cp) : null;
  const family = font === SYMBOL_FONTS[0] ? undefined : `'${font}'`;
  return (
    <DeckDialogFrame model={model} title={t('Symbol')} testId="deck-symbol" size="md">
      <div className="kiln-deckdlg__row">
        <Select
          aria-label={t('Font')}
          value={font}
          onValueChange={setFont}
          options={SYMBOL_FONTS.map((f) => ({ value: f, label: f }))}
        />
        <Select
          aria-label={t('Subset')}
          value={set}
          onValueChange={setSet}
          options={SYMBOLS.map(([n]) => ({ value: n, label: n }))}
        />
      </div>
      <fieldset className="kiln-decksymbols" aria-label={set} style={{ fontFamily: family }}>
        {[...chars].map((ch) => (
          <Button
            key={ch}
            className="kiln-decksymbols__item"
            aria-label={t('Insert {ch}', { ch })}
            onClick={() => void insert(ch)}
          >
            {ch}
          </Button>
        ))}
      </fieldset>
      <div className="kiln-decksymbols__code">
        <label>
          {t('Character code (Unicode hex)')}{' '}
          <input
            className="kiln-deckdlg__text"
            aria-label={t('Character code')}
            value={code}
            size={8}
            onChange={(e) => setCode(e.target.value.replace(/^(U\+|0x)/i, '').trim())}
          />
        </label>
        <span className="kiln-decksymbols__font" style={{ fontFamily: family }} aria-hidden="true">
          {typed ?? ''}
        </span>
        <Button disabled={!typed} onClick={() => (typed ? void insert(typed) : undefined)}>
          {t('Insert')}
        </Button>
      </div>
    </DeckDialogFrame>
  );
}
