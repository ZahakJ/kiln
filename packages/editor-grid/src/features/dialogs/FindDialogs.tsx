/**
 * Find and Replace (Ctrl+H, Find & Select ▸ Find…): Find All with its list of cells, Find Next,
 * Replace and Replace All, and Excel's options (within sheet or workbook, by rows or columns,
 * look in formulas, values or notes, match case, match entire cell contents). Go To Special,
 * Insert Hyperlink (Ctrl+K) and the Symbol dialog.
 */
import { msg, t, txPairs } from '@kiln/i18n';
import type { GridFindHit, GridFindSpec, GridLink, GridLookIn, GridSpecial } from '@kiln/protocol';
import { Button, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useState } from 'react';
import type { GridFeatures } from '../model.ts';
import { gridFeatures } from '../model.ts';
import { Check, Choice, FeatureFrame, Radios, TextField } from '../ui.tsx';

/** What was looked for last (Find and Replace, the find bar): the dialog opens with it, as Excel's does. */
const last = { text: '', replace: '' };

export function FindDialog({ features, tab }: { features: GridFeatures; tab: 'find' | 'replace' }) {
  const [mode, setMode] = useState(tab);
  const [text, setTextState] = useState(() => last.text || features.model.state.find.text);
  const [replace, setReplaceState] = useState(() => last.replace);
  const setText = (v: string) => {
    last.text = v;
    setTextState(v);
  };
  const setReplace = (v: string) => {
    last.replace = v;
    setReplaceState(v);
  };
  const [options, setOptions] = useState(false);
  const [spec, setSpec] = useState<Omit<GridFindSpec, 'text'>>({
    match_case: false,
    whole_cell: false,
    look_in: 'Formulas',
    workbook: false,
    by_columns: false,
  });
  const [hits, setHits] = useState<GridFindHit[] | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const full = (): GridFindSpec => ({ ...spec, text });
  const findAll = async () => {
    const r = await features.ask({ FindAll: { spec: full() } });
    const list = r && 'Found' in r ? r.Found : [];
    setHits(list);
    setStatus(t('grid.cellsFound', { count: list.length }));
  };
  const findNext = async () => {
    const ok = await features.model.run('grid.editing.find_next', {
      text,
      match_case: spec.match_case,
      whole_cell: spec.whole_cell,
      backward: false,
      in_formulas: spec.look_in === 'Formulas',
    });
    setStatus(ok ? null : t("We couldn't find what you were looking for."));
  };
  const doReplace = async (all: boolean) => {
    const e = await features.tryExec({ Replace: { spec: full(), with: replace, all } });
    if (hits) {
      const r = await features.ask({ FindAll: { spec: full() } });
      setHits(r && 'Found' in r ? r.Found : []);
    }
    setStatus(e ?? gridFeatures(features.model)?.message ?? null);
  };
  const go = async (h: GridFindHit) => {
    if (h.sheet !== features.model.state.grid?.active_sheet)
      await features.model.run('grid.sheet.activate', { index: h.sheet });
    await features.model.core('grid.nav.select_cell', { row: h.row, col: h.col });
  };
  return (
    <FeatureFrame features={features} title={t('Find and Replace')} size="lg" testId="grid-find">
      <Tabs value={mode} onValueChange={(v) => setMode(v as 'find' | 'replace')}>
        <TabsList aria-label={t('Find and Replace')}>
          <TabsTrigger value="find">{t('Find')}</TabsTrigger>
          <TabsTrigger value="replace">{t('Replace')}</TabsTrigger>
        </TabsList>
        <TabsContent value={mode} className="kiln-gfx__tab">
          <TextField
            label={t('Find what')}
            value={text}
            onChange={setText}
            autoFocus
            wide
            testId="grid-find-what"
          />
          {mode === 'replace' ? (
            <TextField
              label={t('Replace with')}
              value={replace}
              onChange={setReplace}
              wide
              testId="grid-find-with"
            />
          ) : null}
          {options ? (
            <div className="kiln-gfx__grid2">
              <Choice
                label={t('Within')}
                value={spec.workbook ? 'wb' : 'sheet'}
                width={140}
                options={[
                  ['sheet', t('Sheet')],
                  ['wb', t('Workbook')],
                ]}
                onChange={(v) => setSpec({ ...spec, workbook: v === 'wb' })}
              />
              <Choice
                label={t('Search')}
                value={spec.by_columns ? 'cols' : 'rows'}
                width={140}
                options={[
                  ['rows', t('By Rows')],
                  ['cols', t('By Columns')],
                ]}
                onChange={(v) => setSpec({ ...spec, by_columns: v === 'cols' })}
              />
              <Choice<GridLookIn>
                label={t('Look in')}
                value={mode === 'replace' ? 'Formulas' : spec.look_in}
                width={140}
                options={
                  mode === 'replace'
                    ? [['Formulas', t('Formulas')]]
                    : [
                        ['Formulas', t('Formulas')],
                        ['Values', t('Values')],
                        ['Notes', t('Notes')],
                      ]
                }
                onChange={(look_in) => setSpec({ ...spec, look_in })}
              />
              <div className="kiln-gfx__col">
                <Check
                  label={t('Match case')}
                  checked={spec.match_case}
                  onChange={(match_case) => setSpec({ ...spec, match_case })}
                />
                <Check
                  label={t('Match entire cell contents')}
                  checked={spec.whole_cell}
                  onChange={(whole_cell) => setSpec({ ...spec, whole_cell })}
                />
              </div>
            </div>
          ) : null}
          <div className="kiln-gfx__toolbar">
            <Button size="sm" onClick={() => setOptions((o) => !o)} aria-expanded={options}>
              {options ? t('Options <<') : t('Options >>')}
            </Button>
            <span className="kiln-gfx__grow" />
            {mode === 'replace' ? (
              <>
                <Button
                  size="sm"
                  disabled={!text}
                  onClick={() => doReplace(true)}
                  data-testid="grid-replace-all"
                >
                  {t('Replace All')}
                </Button>
                <Button size="sm" disabled={!text} onClick={() => doReplace(false)}>
                  {t('Replace')}
                </Button>
              </>
            ) : null}
            <Button size="sm" disabled={!text} onClick={findAll} data-testid="grid-find-all">
              {t('Find All')}
            </Button>
            <Button size="sm" variant="primary" disabled={!text} onClick={findNext}>
              {t('Find Next')}
            </Button>
          </div>
          {status ? (
            <p className="kiln-gfx__hint" role="status">
              {status}
            </p>
          ) : null}
          {hits ? (
            <div className="kiln-gfx__table kiln-gfx__hits">
              <div className="kiln-gfx__thead">
                <span>{t('Sheet')}</span>
                <span>{t('Cell')}</span>
                <span>{t('Value')}</span>
                <span>{t('Formula')}</span>
              </div>
              {hits.slice(0, 500).map((h) => (
                <button
                  type="button"
                  key={`${h.sheet}-${h.row}-${h.col}`}
                  className="kiln-gfx__level"
                  onClick={() => go(h)}
                >
                  <span>{h.sheet_name}</span>
                  <span>{h.cell}</span>
                  <span>{h.value}</span>
                  <span>{h.formula ?? ''}</span>
                </button>
              ))}
            </div>
          ) : null}
        </TabsContent>
      </Tabs>
    </FeatureFrame>
  );
}

/** Go To Special's choices: the core's, and Objects (the sheet's charts and pictures). */
type Special = GridSpecial | 'Objects';

const SPECIALS: readonly (readonly [Special, string])[] = [
  ['Notes', msg('Notes')],
  ['Constants', msg('Constants')],
  ['Formulas', msg('Formulas')],
  ['Blanks', msg('Blanks')],
  ['CurrentRegion', msg('Current region')],
  ['CurrentArray', msg('Current array')],
  ['LastCell', msg('Last cell')],
  ['VisibleCells', msg('Visible cells only')],
  ['Errors', msg('Errors')],
  ['ConditionalFormats', msg('Conditional formats')],
  ['DataValidation', msg('Data validation')],
  ['Precedents', msg('Precedents (direct only)')],
  ['Dependents', msg('Dependents (direct only)')],
  ['Objects', msg('Objects')],
];

export function GoToSpecialDialog({ features, preset }: { features: GridFeatures; preset?: GridSpecial }) {
  const [what, setWhat] = useState<Special>(preset ?? 'Blanks');
  return (
    <FeatureFrame
      features={features}
      title={t('Go To Special')}
      testId="grid-go-to-special"
      onOk={() =>
        // Objects selects the first one; Tab and Shift+Tab then go from one to the next.
        what === 'Objects'
          ? features.tryExec({ SelectDrawing: { index: 0 } })
          : features.tryExec({ SelectSpecial: { special: what } })
      }
    >
      <Radios label={t('Select')} value={what} options={txPairs(SPECIALS)} onChange={setWhat} />
    </FeatureFrame>
  );
}

export function LinkDialog({ features }: { features: GridFeatures }) {
  const current = gridFeatures(features.model)?.link ?? null;
  const [link, setLink] = useState<GridLink>(
    current ?? { target: '', location: '', display: features.model.state.grid?.formula ?? '', tooltip: '' },
  );
  const [kind, setKind] = useState<'web' | 'place' | 'mail'>(
    current?.location ? 'place' : current?.target.startsWith('mailto:') ? 'mail' : 'web',
  );
  const set = (p: Partial<GridLink>) => setLink((l) => ({ ...l, ...p }));
  return (
    <FeatureFrame
      features={features}
      title={current ? t('Edit Hyperlink') : t('Insert Hyperlink')}
      size="md"
      testId="grid-link"
      footerStart={
        current ? (
          <button
            type="button"
            className="kiln-gfx__link"
            onClick={async () => {
              await features.exec({ SetLink: { link: null } });
              features.closeDialog();
            }}
          >
            {t('Remove Link')}
          </button>
        ) : undefined
      }
      onOk={() =>
        features.tryExec({
          SetLink: {
            link:
              kind === 'place'
                ? { ...link, target: '' }
                : kind === 'mail'
                  ? {
                      ...link,
                      location: '',
                      target: link.target.startsWith('mailto:') ? link.target : `mailto:${link.target}`,
                    }
                  : { ...link, location: '' },
          },
        })
      }
    >
      <Radios
        label={t('Link to')}
        value={kind}
        options={[
          ['web', t('Existing File or Web Page')],
          ['place', t('Place in This Document')],
          ['mail', t('E-mail Address')],
        ]}
        onChange={setKind}
      />
      <TextField
        label={t('Text to display')}
        value={link.display}
        onChange={(display) => set({ display })}
        wide
      />
      {kind === 'place' ? (
        <TextField
          label={t('Type the cell reference or a defined name')}
          value={link.location}
          placeholder="Sheet1!A1" // i18n-ignore: a cell reference
          onChange={(location) => set({ location })}
          wide
          autoFocus
        />
      ) : (
        <TextField
          label={kind === 'mail' ? t('E-mail address') : t('Address')}
          value={link.target.replace(/^mailto:/, '')}
          // i18n-ignore: example addresses
          placeholder={kind === 'mail' ? 'name@example.com' : 'https://'}
          onChange={(target) => set({ target })}
          wide
          autoFocus
          testId="grid-link-address"
        />
      )}
      <TextField label={t('ScreenTip')} value={link.tooltip} onChange={(tooltip) => set({ tooltip })} wide />
    </FeatureFrame>
  );
}

const SYMBOLS =
  '¡¿¦¨¯´¸ªº¹²³¼½¾ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõöøùúûüýþÿŒœŠšŸŽžƒ' +
  '₹₽₩₪₫₱₺₿℃℉№℗℠Ω℮⅛⅜⅝⅞∆∏∓∗∘∙∛∜∟∥∦∴∵∶∷∼≅≪≫⊥⋅⌀⌂⌘─│┌┐└┘├┤┬┴┼═║▀▄█░▒▓■□▪▫▲△▶▷▼▽◀◁◆◇○●◎◐◑☀☁☂☎☐☑☒☺☻♀♂♪♫✉✎✔✘✚✦❤➔➜' +
  '©®™°±×÷µ¶§€£¥¢¤←↑→↓↔↕⇐⇒⇔∀∂∃∅∇∈∉∑∏√∝∞∠∧∨∩∪∫≈≠≡≤≥⊂⊃⊆⊇⊕⊗αβγδεζηθικλμνξπρστυφχψωΓΔΘΛΞΠΣΦΨΩ•…‰′″‹›«»–—‘’“”✓✗★☆♠♣♥♦½¼¾⅓⅔';

/** Insert ▸ Symbol: the character goes into the active cell's entry. */
export function SymbolDialog({ features }: { features: GridFeatures }) {
  const [pick, setPick] = useState('©');
  const [code, setCode] = useState('00A9');
  const insert = async (ch: string) => {
    const m = features.model;
    if (m.state.grid?.mode === 'Ready') await m.core('grid.editing.edit_cell');
    const ed = (await m.fetch())?.editor;
    const text = ed?.text ?? '';
    const at = ed?.end ?? text.length;
    await m.edit('grid.editing.set_text', {
      text: text.slice(0, at) + ch + text.slice(at),
      start: at + 1,
      end: at + 1,
    });
  };
  return (
    <FeatureFrame
      features={features}
      title={t('Symbol')}
      size="md"
      testId="grid-symbol"
      okLabel={t('Insert')}
      onOk={() => insert(pick)}
    >
      <div className="kiln-gfx__symbols" role="listbox" aria-label={t('Symbols')}>
        {[...SYMBOLS].map((ch) => (
          <button
            type="button"
            key={ch}
            role="option"
            aria-selected={ch === pick}
            className={ch === pick ? 'kiln-gfx__symbol kiln-gfx__symbol--on' : 'kiln-gfx__symbol'}
            onClick={() => {
              setPick(ch);
              setCode((ch.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, '0'));
            }}
            onDoubleClick={async () => {
              await insert(ch);
              features.closeDialog();
            }}
          >
            {ch}
          </button>
        ))}
      </div>
      <div className="kiln-gfx__row">
        <TextField
          label={t('Character code (Unicode hex)')}
          value={code}
          testId="grid-symbol-code"
          onChange={(v) => {
            setCode(v);
            const n = Number.parseInt(v.replace(/^(U\+|0x)/i, ''), 16);
            if (Number.isFinite(n) && n >= 0x20 && n <= 0x10ffff) {
              try {
                setPick(String.fromCodePoint(n));
              } catch {
                // Not a character.
              }
            }
          }}
        />
        <span className="kiln-gfx__symbol-preview" title={t('Selected symbol')}>
          {pick}
        </span>
      </div>
    </FeatureFrame>
  );
}
