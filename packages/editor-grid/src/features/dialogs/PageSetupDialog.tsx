/**
 * Page Setup (Page Layout ▸ Page Setup launcher, Print Titles, Custom Margins, Insert ▸ Header &
 * Footer): the Page, Margins, Header/Footer and Sheet tabs with Excel's fields — orientation,
 * Adjust to % or Fit to N pages wide by M tall, paper size, first page number; margins in
 * inches with Center on page; header and footer codes with Excel's presets and a preview; print
 * area, rows and columns to repeat, gridlines, headings, black and white, page order.
 */
import { msg, t, tx, txPairs } from '@kiln/i18n';
import type { GridMargins, GridPageInfo, GridPageSetup } from '@kiln/protocol';
import { Button, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { useEffect, useState } from 'react';
import type { GridFeatures } from '../model.ts';
import { Check, Choice, FeatureFrame, Radios, TextField } from '../ui.tsx';

export const PAPERS: readonly (readonly [string, string])[] = [
  ['1', msg('Letter (8.5" × 11")')],
  ['5', msg('Legal (8.5" × 14")')],
  ['3', msg('Tabloid (11" × 17")')],
  ['7', msg('Executive (7.25" × 10.5")')],
  ['8', msg('A3 (297 × 420 mm)')],
  ['9', msg('A4 (210 × 297 mm)')],
  ['11', msg('A5 (148 × 210 mm)')],
  ['13', msg('B5 (182 × 257 mm)')],
];

/** Excel's margin presets (inches). */
export const MARGIN_PRESETS: Record<'normal' | 'wide' | 'narrow', GridMargins> = {
  normal: { top: 0.75, bottom: 0.75, left: 0.7, right: 0.7, header: 0.3, footer: 0.3 },
  wide: { top: 1, bottom: 1, left: 1, right: 1, header: 0.5, footer: 0.5 },
  narrow: { top: 0.75, bottom: 0.75, left: 0.25, right: 0.25, header: 0.3, footer: 0.3 },
};

const HEADER_PRESETS: readonly (readonly [string, string])[] = [
  ['', '(none)'],
  ['&CPage &P', msg('Page 1')],
  ['&CPage &P of &N', msg('Page 1 of ?')],
  ['&C&A', msg('Sheet name')],
  ['&C&F', msg('File name')],
  ['&C&A&RPage &P', msg('Sheet name, Page 1')],
  ['&L&F&C&A&R&D', msg('File name, sheet name, date')],
  ['&LPrepared by &C&D&RPage &P of &N', msg('Prepared by, date, Page 1 of ?')],
];

function num(v: string, fallback: number): number {
  const n = Number(v.replace(',', '.'));
  return Number.isFinite(n) ? n : fallback;
}

/** The margin boxes' labels. */
const MARGIN_LABELS = {
  top: msg('Top (inches)'),
  header: msg('Header (inches)'),
  left: msg('Left (inches)'),
  right: msg('Right (inches)'),
  bottom: msg('Bottom (inches)'),
  footer: msg('Footer (inches)'),
} as const;

export function PageSetupDialog({ features, tab }: { features: GridFeatures; tab: string }) {
  const [info, setInfo] = useState<GridPageInfo | null>(null);
  const [s, setS] = useState<GridPageSetup>({} as GridPageSetup);
  useEffect(() => {
    void features.ask('PageSetup').then((r) => {
      if (r && 'PageSetup' in r) setInfo(r.PageSetup);
    });
  }, [features]);
  if (!info)
    return (
      <FeatureFrame features={features} title={t('Page Setup')}>
        {null}
      </FeatureFrame>
    );
  const v = <K extends keyof GridPageSetup>(k: K, fallback: NonNullable<GridPageSetup[K]>) =>
    (s[k] ?? fallback) as NonNullable<GridPageSetup[K]>;
  const set = (p: Partial<GridPageSetup>) => setS((x) => ({ ...x, ...p }));
  const margins = v('margins', info.margins);
  const setMargin = (k: keyof GridMargins, text: string) =>
    set({ margins: { ...margins, [k]: num(text, margins[k]) } });
  const fit = v('fit', info.fit);
  const full = (): GridPageSetup => {
    const empty: GridPageSetup = {
      landscape: null,
      paper: null,
      margins: null,
      scale: null,
      fit_width: null,
      fit_height: null,
      fit: null,
      gridlines: null,
      headings: null,
      center_h: null,
      center_v: null,
      header: null,
      footer: null,
      first_page: null,
      over_then_down: null,
      black_and_white: null,
      print_area: null,
      title_rows: null,
      title_cols: null,
    };
    return Object.assign(empty, s);
  };
  return (
    <FeatureFrame
      features={features}
      title={t('Page Setup')}
      size="lg"
      testId="grid-page-setup"
      footerStart={
        <Button
          size="sm"
          onClick={async () => {
            const e = await features.tryExec({ PageSetup: { setup: full() } });
            if (!e) {
              features.closeDialog();
              await features.model.run('suite.file.print');
            }
          }}
        >
          {t('Print Preview')}
        </Button>
      }
      onOk={() => features.tryExec({ PageSetup: { setup: full() } })}
    >
      <Tabs defaultValue={tab}>
        <TabsList aria-label={t('Page Setup')}>
          <TabsTrigger value="page">{t('Page')}</TabsTrigger>
          <TabsTrigger value="margins">{t('Margins')}</TabsTrigger>
          <TabsTrigger value="header">{t('Header/Footer')}</TabsTrigger>
          <TabsTrigger value="sheet">{t('Sheet')}</TabsTrigger>
        </TabsList>
        <TabsContent value="page" className="kiln-gfx__tab">
          <Radios
            label={t('Orientation')}
            value={v('landscape', info.landscape) ? 'l' : 'p'}
            options={[
              ['p', t('Portrait')],
              ['l', t('Landscape')],
            ]}
            onChange={(o) => set({ landscape: o === 'l' })}
          />
          <fieldset className="kiln-griddlg__radios">
            <legend>{t('Scaling')}</legend>
            <label className="kiln-griddlg__check">
              <input type="radio" name="scaling" checked={!fit} onChange={() => set({ fit: false })} />
              {t('Adjust to')}
              <input
                className="kiln-griddlg__input kiln-gfx__num"
                aria-label={t('Adjust to percent of normal size')}
                value={String(v('scale', info.scale))}
                onChange={(e) => set({ scale: Math.round(num(e.target.value, 100)), fit: false })}
              />
              {t('% normal size')}
            </label>
            <label className="kiln-griddlg__check">
              <input type="radio" name="scaling" checked={fit} onChange={() => set({ fit: true })} />
              {t('Fit to')}
              <input
                className="kiln-griddlg__input kiln-gfx__num"
                aria-label={t('Pages wide')}
                value={String(v('fit_width', info.fit_width) || '')}
                onChange={(e) => set({ fit_width: Math.round(num(e.target.value, 0)), fit: true })}
              />
              {t('page(s) wide by')}
              <input
                className="kiln-griddlg__input kiln-gfx__num"
                aria-label={t('Pages tall')}
                value={String(v('fit_height', info.fit_height) || '')}
                onChange={(e) => set({ fit_height: Math.round(num(e.target.value, 0)), fit: true })}
              />
              {t('tall')}
            </label>
          </fieldset>
          <Choice
            label={t('Paper size')}
            value={String(v('paper', info.paper))}
            width={240}
            options={txPairs(PAPERS)}
            onChange={(p) => set({ paper: Number(p) })}
          />
          <TextField
            label={t('First page number')}
            value={
              v('first_page', info.first_page ?? 0) ? String(v('first_page', info.first_page ?? 0)) : 'Auto'
            }
            onChange={(t) =>
              set({ first_page: t.trim().toLowerCase() === 'auto' || !t.trim() ? 0 : Math.round(num(t, 0)) })
            }
          />
          <p className="kiln-gfx__hint">{t('grid.printsOn', { count: info.pages })}</p>
        </TabsContent>
        <TabsContent value="margins" className="kiln-gfx__tab">
          <div className="kiln-gfx__margins">
            {(['top', 'header', 'left', 'right', 'bottom', 'footer'] as const).map((k) => (
              <TextField
                key={k}
                label={tx(MARGIN_LABELS[k])}
                value={String(margins[k])}
                onChange={(t) => setMargin(k, t)}
              />
            ))}
          </div>
          <div className="kiln-gfx__row">
            <Check
              label={t('Center horizontally')}
              checked={v('center_h', info.center_h)}
              onChange={(center_h) => set({ center_h })}
            />
            <Check
              label={t('Center vertically')}
              checked={v('center_v', info.center_v)}
              onChange={(center_v) => set({ center_v })}
            />
          </div>
        </TabsContent>
        <TabsContent value="header" className="kiln-gfx__tab">
          <Choice
            label={t('Header')}
            value={
              HEADER_PRESETS.some(([c]) => c === v('header', info.header))
                ? v('header', info.header)
                : 'custom'
            }
            width={320}
            options={[...txPairs(HEADER_PRESETS), ['custom', t('Custom…')] as const]}
            onChange={(c) => c !== 'custom' && set({ header: c })}
          />
          <TextField
            label={t(
              'Header codes (&L left, &C centre, &R right, &P page, &N pages, &D date, &A sheet, &F file)',
            )}
            value={v('header', info.header)}
            onChange={(header) => set({ header })}
            wide
            testId="grid-header-code"
          />
          <Choice
            label={t('Footer')}
            value={
              HEADER_PRESETS.some(([c]) => c === v('footer', info.footer))
                ? v('footer', info.footer)
                : 'custom'
            }
            width={320}
            options={[...txPairs(HEADER_PRESETS), ['custom', t('Custom…')] as const]}
            onChange={(c) => c !== 'custom' && set({ footer: c })}
          />
          <TextField
            label={t('Footer codes')}
            value={v('footer', info.footer)}
            onChange={(footer) => set({ footer })}
            wide
          />
          <div className="kiln-gfx__hf-preview">
            {info.header_preview.map((t, i) => (
              <span key={i}>{t}</span>
            ))}
          </div>
        </TabsContent>
        <TabsContent value="sheet" className="kiln-gfx__tab">
          <TextField
            label={t('Print area')}
            value={v('print_area', info.print_area)}
            onChange={(print_area) => set({ print_area })}
            wide
            placeholder="$A$1:$F$40" // i18n-ignore: a range reference
          />
          <TextField
            label={t('Rows to repeat at top')}
            value={v('title_rows', info.title_rows)}
            onChange={(title_rows) => set({ title_rows })}
            wide
            placeholder="$1:$1"
            testId="grid-title-rows"
          />
          <TextField
            label={t('Columns to repeat at left')}
            value={v('title_cols', info.title_cols)}
            onChange={(title_cols) => set({ title_cols })}
            wide
            placeholder="$A:$A" // i18n-ignore: a range reference
          />
          <div className="kiln-gfx__row">
            <Check
              label={t('Gridlines')}
              checked={v('gridlines', info.gridlines)}
              onChange={(gridlines) => set({ gridlines })}
            />
            <Check
              label={t('Row and column headings')}
              checked={v('headings', info.headings)}
              onChange={(headings) => set({ headings })}
            />
            <Check
              label={t('Black and white')}
              checked={v('black_and_white', info.black_and_white)}
              onChange={(black_and_white) => set({ black_and_white })}
            />
          </div>
          <Radios
            label={t('Page order')}
            value={v('over_then_down', info.over_then_down) ? 'over' : 'down'}
            options={[
              ['down', t('Down, then over')],
              ['over', t('Over, then down')],
            ]}
            onChange={(o) => set({ over_then_down: o === 'over' })}
          />
        </TabsContent>
      </Tabs>
    </FeatureFrame>
  );
}
