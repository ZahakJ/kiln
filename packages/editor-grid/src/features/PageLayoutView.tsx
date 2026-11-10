/**
 * View ▸ Page Layout: the sheet's printed pages one under another — margins, print titles,
 * scaling, headers and footers exactly as File ▸ Print renders them (the core's preview scenes).
 * The cells are selected and edited on the page (`PageLayoutCells`), as in Excel.
 */
import { t } from '@kiln/i18n';
import type { PrintPreview as PrintPreviewData } from '@kiln/protocol';
import { DEFAULT_PRINT_SETTINGS, PrintPreview, sceneCssSize } from '@kiln/ui-kit';
import { useCallback, useEffect, useState } from 'react';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { featuresOf } from './model.ts';
import { AddDataPage, PageLayoutCells, type PaperInset } from './PageLayoutCells.tsx';

/** A ruler along one side of a page: a tick every quarter inch, numbered every inch. */
function Ruler({ inches, vertical }: { inches: number; vertical?: boolean }) {
  const ticks = Math.floor(inches * 4);
  return (
    <div
      className={vertical ? 'kiln-pagelayout__ruler kiln-pagelayout__ruler--v' : 'kiln-pagelayout__ruler'}
      aria-hidden="true"
    >
      {Array.from({ length: ticks + 1 }, (_, i) => (
        <span
          key={i}
          className={
            i % 4 === 0 ? 'kiln-pagelayout__tick kiln-pagelayout__tick--inch' : 'kiln-pagelayout__tick'
          }
          style={
            vertical ? { top: `${(i / (inches * 4)) * 100}%` } : { left: `${(i / (inches * 4)) * 100}%` }
          }
        >
          {i % 4 === 0 && i > 0 ? i / 4 : null}
        </span>
      ))}
    </div>
  );
}

export function PageLayoutView({ model }: { model: GridEditorModel }) {
  const view = useGrid(model, (s) => s.status?.view);
  const grid = useGrid(model, (s) => s.grid);
  const ruler = useGrid(model, (s) => s.showRuler);
  const [pages, setPages] = useState<PrintPreviewData[]>([]);
  // The preview's margin round each paper: the view shows the paper alone, its rulers on its edges.
  const [insets, setInsets] = useState<Record<number, PaperInset>>({});
  const onPaper = useCallback(
    (i: number, inset: PaperInset) =>
      setInsets((prev) => {
        const p = prev[i];
        const same =
          p &&
          Math.abs(p.top - inset.top) < 0.5 &&
          Math.abs(p.left - inset.left) < 0.5 &&
          Math.abs(p.right - inset.right) < 0.5 &&
          Math.abs(p.bottom - inset.bottom) < 0.5;
        return same ? prev : { ...prev, [i]: inset };
      }),
    [],
  );
  const [count, setCount] = useState(0);
  const active = view === 'PageLayout';
  // The pages are drawn again after every change to the workbook (an entry, a format).
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!active) return;
    return model.client.subscribe((e) => {
      if (typeof e === 'object' && 'Changed' in e && e.Changed.summary.doc === model.doc)
        setRevision(e.Changed.summary.revision);
    });
  }, [active, model]);
  useEffect(() => {
    if (!active) return;
    let live = true;
    void (async () => {
      const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
      const out: PrintPreviewData[] = [];
      try {
        const first = await model.client.query(model.doc, {
          PrintPreview: { settings: DEFAULT_PRINT_SETTINGS, sheet: 0, scale: dpr * 0.75 },
        });
        if (!('PrintPreview' in first)) return;
        out.push(first.PrintPreview);
        const n = Math.min(first.PrintPreview.sheets, 40);
        // Each page is replaced as it is drawn again (no flash of the others while typing).
        const put = (i: number, p: PrintPreviewData) =>
          setPages((prev) => {
            const next = prev.slice(0, n);
            next[i] = p;
            return next;
          });
        if (live) {
          setCount(first.PrintPreview.sheets);
          put(0, first.PrintPreview);
        }
        for (let i = 1; i < n && live; i++) {
          const r = await model.client.query(model.doc, {
            PrintPreview: { settings: DEFAULT_PRINT_SETTINGS, sheet: i, scale: dpr * 0.75 },
          });
          if ('PrintPreview' in r) {
            out.push(r.PrintPreview);
            if (live) put(i, r.PrintPreview);
          }
        }
      } catch {
        // The view stays empty; Normal view still edits.
      }
    })();
    return () => {
      live = false;
    };
  }, [active, model, grid?.active_sheet, grid?.features.pages, revision]);
  if (!active) return null;
  return (
    <section className="kiln-pagelayout" data-testid="grid-page-layout" aria-label={t('Page Layout view')}>
      <div className="kiln-pagelayout__bar">{t('grid.pageLayoutPages', { count })}</div>
      {pages.map((p, i) => {
        const inset = insets[i];
        // The page's box keeps the preview's size; negative margins and a clip leave the paper.
        const paper = inset
          ? {
              margin: `${-inset.top}px ${-inset.right}px ${-inset.bottom}px ${-inset.left}px`,
              clipPath: `inset(${inset.top}px ${inset.right}px ${inset.bottom}px ${inset.left}px)`,
            }
          : undefined;
        const hf = (top: boolean) =>
          inset
            ? {
                left: `calc(${inset.left}px + 6%)`,
                right: `calc(${inset.right}px + 6%)`,
                ...(top ? { top: inset.top + 6 } : { bottom: inset.bottom + 6 }),
              }
            : undefined;
        return (
          <div
            key={i}
            className={
              ruler ? 'kiln-pagelayout__frame' : 'kiln-pagelayout__frame kiln-pagelayout__frame--bare'
            }
          >
            {ruler ? (
              <>
                <span />
                <Ruler inches={p.width_pt / 72} />
                <Ruler inches={p.height_pt / 72} vertical />
              </>
            ) : null}
            <div className="kiln-pagelayout__paper">
              <div className="kiln-pagelayout__page" style={paper}>
                <PrintPreview
                  scene={p.scene}
                  label={t('Page {page, number} of {count, number}', { page: i + 1, count })}
                />
                <PageLayoutCells
                  model={model}
                  page={i}
                  revision={revision}
                  extent={p.scene.extent}
                  onPaper={(r) => onPaper(i, r)}
                />
                <button
                  type="button"
                  className="kiln-pagelayout__hf kiln-pagelayout__hf--top"
                  data-testid="grid-page-header"
                  style={hf(true)}
                  onClick={() => featuresOf(model).openDialog('pageSetup', 'header')}
                >
                  {t('Click to add header')}
                </button>
                <button
                  type="button"
                  className="kiln-pagelayout__hf kiln-pagelayout__hf--bottom"
                  style={hf(false)}
                  onClick={() => featuresOf(model).openDialog('pageSetup', 'header')}
                >
                  {t('Click to add footer')}
                </button>
              </div>
            </div>
          </div>
        );
      })}
      {pages.length > 0 && pages.length === Math.min(count, 40) ? (
        <div
          className={ruler ? 'kiln-pagelayout__frame' : 'kiln-pagelayout__frame kiln-pagelayout__frame--bare'}
        >
          {ruler ? (
            <>
              <span />
              <span />
              <span />
            </>
          ) : null}
          <div className="kiln-pagelayout__paper">
            <AddDataPage
              model={model}
              last={pages.length - 1}
              {...paperSize(pages[pages.length - 1] as PrintPreviewData, insets[pages.length - 1])}
            />
          </div>
        </div>
      ) : null}
    </section>
  );
}

/** A paper's size on screen (its preview's CSS size less the margin round the paper). */
function paperSize(p: PrintPreviewData, inset: PaperInset | undefined): { width: number; height: number } {
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const box = sceneCssSize(p.scene, dpr);
  return {
    width: box.width - (inset ? inset.left + inset.right : 0),
    height: box.height - (inset ? inset.top + inset.bottom : 0),
  };
}
