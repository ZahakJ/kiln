/** Page Setup (Layout ↘), Insert Table, and Find and Replace with Go To. */
import { msg, t, tx } from '@kiln/i18n';
import type { CellVerticalAlign, FindQuery, GoToTarget, PageSetupSpec, TableAutofit } from '@kiln/protocol';
import { Button, NumberInput, Select, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import { EMPTY_FIND, type WriteEditorModel } from '../model.ts';
import { Check, cmOf, Field, Grid, LEN_UNIT, ptOf, Radio, Section, WriteDialogFrame } from './common.tsx';

const PAPERS: readonly [string, number, number][] = [
  ['Letter', 612, 792],
  ['Legal', 612, 1008],
  ['Executive', 522, 756],
  ['A3', 841.9, 1190.55],
  ['A4', 595.3, 841.9],
  ['A5', 419.55, 595.3],
  ['B5 (JIS)', 515.95, 728.6],
  ['Tabloid', 792, 1224],
];

/** Go To's targets and the prompts of the numbered ones. */
const GO_TO_KINDS = {
  Page: msg('Page'),
  Section: msg('Section'),
  Line: msg('Line'),
  Bookmark: msg('Bookmark'),
  Footnote: msg('Footnote'),
  Comment: msg('Comment'),
} as const;
const GO_TO_PROMPTS: Readonly<Record<string, string>> = {
  Page: msg('Enter page number'),
  Section: msg('Enter section number'),
  Line: msg('Enter line number'),
};

export function PageSetupDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const [spec, setSpec] = useState<PageSetupSpec | null>(null);
  useEffect(() => {
    let live = true;
    void model.query('PageSetup').then((r) => {
      if (live && r && 'PageSetup' in r) setSpec(r.PageSetup);
    });
    return () => {
      live = false;
    };
  }, [model]);
  const set = (patch: Partial<PageSetupSpec>) => setSpec((s) => (s ? { ...s, ...patch } : s));
  const len = (k: keyof PageSetupSpec, label: string) => (
    <Field label={label}>
      {(id) => (
        <NumberInput
          id={id}
          value={cmOf(Number(spec?.[k] ?? 0))}
          onValueChange={(v) => set({ [k]: ptOf(v) })}
          unit={LEN_UNIT}
          min={0}
          max={55}
          step={0.1}
        />
      )}
    </Field>
  );
  const w = spec?.width ?? 612;
  const h = spec?.height ?? 792;
  const portraitW = Math.min(w, h);
  const portraitH = Math.max(w, h);
  const paper =
    PAPERS.find(([, pw, ph]) => Math.abs(pw - portraitW) < 1.5 && Math.abs(ph - portraitH) < 1.5)?.[0] ??
    'Custom';
  return (
    <WriteDialogFrame
      model={model}
      title={t('Page Setup')}
      size="lg"
      okDisabled={!spec}
      onOk={async () => (spec ? !(await model.run('write.layout.page_setup', { setup: spec })) : true)}
      testId="write-page-setup-dialog"
    >
      {spec ? (
        <Tabs defaultValue={tab ?? 'margins'}>
          <TabsList aria-label={t('Page Setup')}>
            <TabsTrigger value="margins">{t('Margins')}</TabsTrigger>
            <TabsTrigger value="paper">{t('Paper')}</TabsTrigger>
            <TabsTrigger value="layout">{t('Layout')}</TabsTrigger>
          </TabsList>
          <TabsContent value="margins">
            <Section title={t('Margins')}>
              <Grid cols={2}>
                {len('top', 'Top')}
                {len('bottom', 'Bottom')}
                {len('left', 'Left')}
                {len('right', 'Right')}
                {len('gutter', 'Gutter')}
              </Grid>
            </Section>
            <Radio
              name="orientation"
              label={t('Orientation')}
              value={spec.landscape ? 'landscape' : 'portrait'}
              options={[
                ['portrait', t('Portrait')],
                ['landscape', t('Landscape')],
              ]}
              onChange={(v) => {
                const landscape = v === 'landscape';
                set({
                  landscape,
                  width: landscape ? portraitH : portraitW,
                  height: landscape ? portraitW : portraitH,
                });
              }}
            />
          </TabsContent>
          <TabsContent value="paper">
            <Field label={t('Paper size')}>
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={paper}
                  onValueChange={(v) => {
                    const p = PAPERS.find(([name]) => name === v);
                    if (!p) return;
                    const [, pw, ph] = p;
                    set(spec.landscape ? { width: ph, height: pw } : { width: pw, height: ph });
                  }}
                  options={[
                    ...PAPERS.map(([name]) => ({ value: name, label: name })),
                    ...(paper === 'Custom' ? [{ value: 'Custom', label: t('Custom size') }] : []),
                  ]}
                />
              )}
            </Field>
            <Grid cols={2}>
              {len('width', 'Width')}
              {len('height', 'Height')}
            </Grid>
          </TabsContent>
          <TabsContent value="layout">
            <Section title={t('Headers and footers (from edge)')}>
              <Grid cols={2}>
                {len('header', 'Header')}
                {len('footer', 'Footer')}
              </Grid>
            </Section>
            <Grid cols={2}>
              <Field label={t('Vertical alignment')}>
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={spec.vertical_align ?? 'Top'}
                    onValueChange={(v) => set({ vertical_align: v as CellVerticalAlign })}
                    options={[
                      { value: 'Top', label: t('Top') },
                      { value: 'Center', label: t('Center') },
                      { value: 'Bottom', label: t('Bottom') },
                    ]}
                  />
                )}
              </Field>
              <Field label={t('Columns')}>
                {(id) => (
                  <NumberInput
                    id={id}
                    value={spec.columns ?? 1}
                    onValueChange={(v) => set({ columns: Math.round(v) })}
                    min={1}
                    max={12}
                    step={1}
                    precision={0}
                  />
                )}
              </Field>
            </Grid>
          </TabsContent>
        </Tabs>
      ) : (
        <p>{t('Reading the page setup…')}</p>
      )}
    </WriteDialogFrame>
  );
}

export function InsertTableDialog({ model }: { model: WriteEditorModel }) {
  const [cols, setCols] = useState(5);
  const [rows, setRows] = useState(2);
  const [autofit, setAutofit] = useState<TableAutofit>('Fixed');
  return (
    <WriteDialogFrame
      model={model}
      title={t('Insert Table')}
      size="sm"
      onOk={async () => !(await model.run('write.table.insert', { rows, cols, autofit }))}
      testId="write-insert-table-dialog"
    >
      <Section title={t('Table size')}>
        <Grid cols={2}>
          <Field label={t('Number of columns')}>
            {(id) => (
              <NumberInput
                id={id}
                value={cols}
                onValueChange={(v) => setCols(Math.round(v))}
                min={1}
                max={63}
                precision={0}
              />
            )}
          </Field>
          <Field label={t('Number of rows')}>
            {(id) => (
              <NumberInput
                id={id}
                value={rows}
                onValueChange={(v) => setRows(Math.round(v))}
                min={1}
                max={32767}
                precision={0}
              />
            )}
          </Field>
        </Grid>
      </Section>
      <Radio
        name="autofit"
        label={t('AutoFit behavior')}
        value={autofit}
        options={[
          ['Fixed', t('Fixed column width')],
          ['Contents', t('AutoFit to contents')],
          ['Window', t('AutoFit to window')],
        ]}
        onChange={setAutofit}
      />
    </WriteDialogFrame>
  );
}

type GoKind = 'Page' | 'Section' | 'Line' | 'Bookmark' | 'Footnote' | 'Comment';

/** Find and Replace (Ctrl+H) and Go To (Ctrl+G, F5). */
export function FindReplaceDialog({ model, tab }: { model: WriteEditorModel; tab?: string }) {
  const [q, setQ] = useState<FindQuery>({ ...EMPTY_FIND, ...model.state.findQuery });
  const [replacement, setReplacement] = useState('');
  const [more, setMore] = useState(q.match_case || q.whole_word || q.wildcards || q.regex);
  const [status, setStatus] = useState<string | null>(null);
  const [kind, setKind] = useState<GoKind>('Page');
  const [target, setTarget] = useState('');
  const [bookmarks, setBookmarks] = useState<string[]>([]);

  useEffect(() => {
    void model.query('Bookmarks').then((r) => {
      if (r && 'Bookmarks' in r) setBookmarks(r.Bookmarks);
    });
  }, [model]);

  const count = async () => {
    const r = await model.query({ FindAll: { query: q } });
    if (!r || !('FindAll' in r)) return 0;
    if (r.FindAll.error) {
      setStatus(r.FindAll.error);
      return -1;
    }
    return r.FindAll.total;
  };
  const findNext = async () => {
    const n = await count();
    if (n === 0) setStatus(t("Kiln didn't find “{text}”.", { text: q.text }));
    else if (n > 0) {
      setStatus(null);
      await model.run('write.find.next', { query: q, backward: false });
    }
  };
  const replaceAll = async () => {
    const n = await count();
    if (n <= 0) {
      if (n === 0) setStatus(t("Kiln didn't find “{text}”.", { text: q.text }));
      return;
    }
    if (await model.run('write.find.replace_all', { query: q, replacement }))
      setStatus(t('write.replaceDone', { count: n }));
  };
  const goTo = async (direction: 0 | 1 | -1) => {
    let goal: GoToTarget | null = null;
    const n = Number.parseInt(target, 10);
    const rel = target.startsWith('+') || target.startsWith('-');
    switch (kind) {
      case 'Page':
        goal = Number.isFinite(n)
          ? { Page: { number: n, relative: rel } }
          : { Page: { number: direction === 0 ? 1 : direction, relative: direction !== 0 } };
        break;
      case 'Section':
        goal = { Section: { number: Number.isFinite(n) ? Math.max(1, n) : 1 } };
        break;
      case 'Line':
        goal = { Line: { number: Number.isFinite(n) ? Math.max(1, n) : 1 } };
        break;
      case 'Bookmark':
        goal = target ? { Bookmark: { name: target } } : null;
        break;
      case 'Footnote':
        goal = { Footnote: { backward: direction === -1 } };
        break;
      case 'Comment':
        goal = { Comment: { backward: direction === -1 } };
        break;
    }
    if (goal && !(await model.run('write.find.go_to', { target: goal })))
      setStatus(t('Kiln could not go there.'));
  };

  const findFields = (
    <>
      <Field label={t('Find what')}>
        {(id) => (
          <input
            id={id}
            className="kiln-write-input"
            value={q.text}
            // biome-ignore lint/a11y/noAutofocus: Find what takes focus when the dialog opens, as in Word
            autoFocus
            onChange={(e) => setQ({ ...q, text: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void findNext();
              }
            }}
          />
        )}
      </Field>
      {more ? (
        <Section title={t('Search options')}>
          <Grid cols={2}>
            <Check
              label={t('Match case')}
              checked={q.match_case}
              onChange={(v) => setQ({ ...q, match_case: v })}
            />
            <Check
              label={t('Find whole words only')}
              checked={q.whole_word}
              onChange={(v) => setQ({ ...q, whole_word: v })}
            />
            <Check
              label={t('Use wildcards')}
              checked={q.wildcards}
              onChange={(v) => setQ({ ...q, wildcards: v, regex: v ? false : q.regex })}
            />
            <Check
              label={t('Regular expressions')}
              checked={q.regex}
              onChange={(v) => setQ({ ...q, regex: v, wildcards: v ? false : q.wildcards })}
            />
          </Grid>
        </Section>
      ) : null}
    </>
  );

  return (
    <WriteDialogFrame model={model} title={t('Find and Replace')} size="lg" testId="write-find-dialog">
      <Tabs defaultValue={tab ?? 'find'} onValueChange={() => setStatus(null)}>
        <TabsList aria-label={t('Find and Replace')}>
          <TabsTrigger value="find">{t('Find')}</TabsTrigger>
          <TabsTrigger value="replace">{t('Replace')}</TabsTrigger>
          <TabsTrigger value="goto">{t('Go To')}</TabsTrigger>
        </TabsList>
        <TabsContent value="find">
          {findFields}
          <div className="kiln-write-inline kiln-write-actions">
            <Button onClick={() => setMore(!more)}>{more ? t('<< Less') : t('More >>')}</Button>
            <Button variant="primary" disabled={!q.text} onClick={() => void findNext()}>
              {t('Find Next')}
            </Button>
          </div>
        </TabsContent>
        <TabsContent value="replace">
          {findFields}
          <Field label={t('Replace with')}>
            {(id) => (
              <input
                id={id}
                className="kiln-write-input"
                value={replacement}
                onChange={(e) => setReplacement(e.target.value)}
              />
            )}
          </Field>
          <div className="kiln-write-inline kiln-write-actions">
            <Button onClick={() => setMore(!more)}>{more ? t('<< Less') : t('More >>')}</Button>
            <Button
              disabled={!q.text}
              onClick={() => void model.run('write.find.replace', { query: q, replacement })}
            >
              {t('Replace')}
            </Button>
            <Button disabled={!q.text} onClick={() => void replaceAll()}>
              {t('Replace All')}
            </Button>
            <Button variant="primary" disabled={!q.text} onClick={() => void findNext()}>
              {t('Find Next')}
            </Button>
          </div>
        </TabsContent>
        <TabsContent value="goto">
          <Grid cols={2}>
            <Field label={t('Go to what')}>
              {(id) => (
                <Select
                  labelId={`${id}-label`}
                  value={kind}
                  onValueChange={(v) => {
                    setKind(v as GoKind);
                    setTarget('');
                  }}
                  options={(['Page', 'Section', 'Line', 'Bookmark', 'Footnote', 'Comment'] as const).map(
                    (v) => ({
                      value: v,
                      label: tx(GO_TO_KINDS[v]),
                    }),
                  )}
                />
              )}
            </Field>
            {kind === 'Bookmark' ? (
              <Field label={t('Enter bookmark name')}>
                {(id) => (
                  <Select
                    labelId={`${id}-label`}
                    value={target}
                    onValueChange={setTarget}
                    placeholder={bookmarks.length ? t('Choose a bookmark') : t('No bookmarks')}
                    options={bookmarks.map((b) => ({ value: b, label: b }))}
                  />
                )}
              </Field>
            ) : kind === 'Footnote' || kind === 'Comment' ? (
              <p className="kiln-write-field__hint">{t('Previous and Next move between them.')}</p>
            ) : (
              <Field
                label={tx(GO_TO_PROMPTS[kind] ?? 'Enter page number')}
                hint={t('+ and − move relative to where you are')}
              >
                {(id) => (
                  <input
                    id={id}
                    className="kiln-write-input"
                    inputMode="numeric"
                    value={target}
                    onChange={(e) => setTarget(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        e.preventDefault();
                        void goTo(0);
                      }
                    }}
                  />
                )}
              </Field>
            )}
          </Grid>
          <div className="kiln-write-inline kiln-write-actions">
            <Button onClick={() => void goTo(-1)}>{t('Previous')}</Button>
            <Button onClick={() => void goTo(1)}>{t('Next')}</Button>
            <Button variant="primary" onClick={() => void goTo(0)}>
              {t('Go To')}
            </Button>
          </div>
        </TabsContent>
      </Tabs>
      {status ? (
        <p className="kiln-write-status" role="status">
          {status}
        </p>
      ) : null}
    </WriteDialogFrame>
  );
}
