/**
 * File ▸ Print: settings on the left — copies and the Print button, the printer,
 * pages, orientation, paper, margins, pages per sheet, scaling and the Write options (for a
 * presentation: slides, the print layout, Frame Slides and Print Hidden Slides) — and the
 * live preview of the sheet on the right, painted from the core's Scene. Every change asks the
 * core for a new preview (debounced); page boxes the core cannot read are reported under the
 * field. Printing and saving are callbacks the frame turns into the platform's print path.
 */
import { t, tx } from '@kiln/i18n';
import type { PrintPreview as PrintPreviewData, PrintSettings } from '@kiln/protocol';
import { ChevronLeft, ChevronRight, FileDown, Printer } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from 'react';
import { errorText } from '../../errorText.ts';
import { Icon } from '../../icons/Icon.tsx';
import { Button } from '../../primitives/Button.tsx';
import { cx } from '../../primitives/cx.ts';
import { IconButton } from '../../primitives/IconButton.tsx';
import { NumberInput } from '../../primitives/NumberInput.tsx';
import { Select } from '../../primitives/Select.tsx';
import { PrintPreview } from '../../print/PrintPreview.tsx';
import {
  DEFAULT_PRINT_SETTINGS,
  MARGINS,
  PAPERS,
  type PageMode,
  PER_SHEET,
  pageMode,
  SLIDE_LAYOUTS,
  sheetLabel,
  slideLayoutOf,
  slidesOf,
  withSlideLayout,
} from './settings.ts';
import './print.css';

/** Where the Print button sends the document. */
export type PrintDestination = 'printer' | 'pdf';

export interface PrintPaneProps {
  /** The sheet `sheet` of the document printed with `settings`, rendered for `scale` device px per preview px. */
  requestPreview: (settings: PrintSettings, sheet: number, scale: number) => Promise<PrintPreviewData>;
  /** Prints (the system print dialog). */
  onPrint: (settings: PrintSettings) => Promise<void>;
  /** Saves the PDF instead (Print to PDF). */
  onSavePdf: (settings: PrintSettings) => Promise<void>;
  /** Show the Write options (markup, hidden text). */
  textOptions?: boolean;
  /** Kiln Deck: slides instead of pages, the print layout and the slide options. */
  slideOptions?: boolean;
  /** Initial settings (tests, a remembered choice). */
  initial?: PrintSettings;
  className?: string;
}

/** Preview px (CSS px at 100 %) of a sheet in points, with the core's margin for the shadow. */
const PREVIEW_MARGIN = 24;
const PX_PER_PT = 96 / 72;

function Field({
  label,
  htmlFor,
  children,
  hint,
}: {
  label: string;
  htmlFor?: string;
  children: ReactNode;
  hint?: ReactNode;
}) {
  return (
    <div className="kiln-print__field">
      <label className="kiln-print__label" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
      {hint}
    </div>
  );
}

function Check({
  checked,
  onChange,
  label,
  detail,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  detail?: string;
}) {
  return (
    <label className="kiln-print__check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.currentTarget.checked)} />
      <span className="kiln-print__switch" aria-hidden="true" />
      <span>
        {label}
        {detail ? <small>{detail}</small> : null}
      </span>
    </label>
  );
}

export function PrintPane({
  requestPreview,
  onPrint,
  onSavePdf,
  textOptions = true,
  slideOptions = false,
  initial = DEFAULT_PRINT_SETTINGS,
  className,
}: PrintPaneProps) {
  const ids = useId();
  const [settings, setSettings] = useState<PrintSettings>(initial);
  const [pagesText, setPagesText] = useState(initial.pages);
  const [destination, setDestination] = useState<PrintDestination>('printer');
  const [sheet, setSheet] = useState(0);
  const [preview, setPreview] = useState<PrintPreviewData | null>(null);
  const [loading, setLoading] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const [boxSize, setBoxSize] = useState({ w: 640, h: 760 });
  const request = useRef(0);

  const update = useCallback((patch: Partial<PrintSettings>) => {
    setSettings((s) => ({ ...s, ...patch }));
    setSheet(0);
    setDone(null);
  }, []);

  // The preview fits the box: measure it.
  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setBoxSize((b) =>
        Math.abs(b.w - width) > 8 || Math.abs(b.h - height) > 8 ? { w: width, h: height } : b,
      );
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Ask the core for the sheet (debounced; late answers to older requests are dropped).
  const sheetPt = preview ? { w: preview.width_pt, h: preview.height_pt } : { w: 612, h: 792 };
  const fit = Math.min(
    Math.max(boxSize.w - 16, 120) / (sheetPt.w * PX_PER_PT + 2 * PREVIEW_MARGIN),
    Math.max(boxSize.h - 16, 160) / (sheetPt.h * PX_PER_PT + 2 * PREVIEW_MARGIN),
  );
  const cssScale = Math.min(Math.max(fit, 0.1), 2);
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const scale = Math.round(cssScale * dpr * 100) / 100;
  useEffect(() => {
    const id = ++request.current;
    setLoading(true);
    const timer = setTimeout(() => {
      requestPreview(settings, sheet, scale).then(
        (p) => {
          if (id !== request.current) return;
          setPreview(p);
          setProblem(null);
          setLoading(false);
          if (p.sheet !== sheet) setSheet(p.sheet);
        },
        (error: unknown) => {
          if (id !== request.current) return;
          setProblem(errorText(error));
          setLoading(false);
        },
      );
    }, 120);
    return () => clearTimeout(timer);
  }, [requestPreview, settings, sheet, scale]);

  const mode = pageMode(settings);
  const setMode = (m: PageMode) => {
    if (m === 'all') update({ pages: '', parity: 'All' });
    else if (m === 'odd') update({ parity: 'Odd' });
    else if (m === 'even') update({ parity: 'Even' });
    else update({ parity: 'All', pages: pagesText.trim() || '1' });
    if (m === 'custom' && !pagesText.trim()) setPagesText('1');
  };

  const go = async () => {
    setWorking(true);
    setDone(null);
    try {
      if (destination === 'pdf') {
        await onSavePdf(settings);
        setDone(t('PDF saved.'));
      } else {
        await onPrint(settings);
        setDone(t('Sent to the print dialog.'));
      }
      setProblem(null);
    } catch (error) {
      setProblem(errorText(error));
    } finally {
      setWorking(false);
    }
  };

  const sheets = preview?.sheets ?? 0;
  const label = preview
    ? sheetLabel(preview.sheet, preview.sheets, preview.pages, settings.per_sheet)
    : t('Preparing the preview');
  const pagesProblem = problem && /page/i.test(problem) ? problem : null;

  return (
    <div className={cx('kiln-print', className)} data-testid="print-pane">
      <section className="kiln-print__settings" aria-label={t('Print settings')}>
        <div className="kiln-print__go">
          <Button
            variant="primary"
            size="lg"
            icon={<Icon icon={destination === 'pdf' ? FileDown : Printer} size={20} />}
            onClick={() => void go()}
            busy={working}
            disabled={working}
            data-testid="print-go"
          >
            {destination === 'pdf' ? t('Save PDF') : t('Print')}
          </Button>
          <Field label={t('Copies')} htmlFor={`${ids}-copies`}>
            <NumberInput
              id={`${ids}-copies`}
              value={settings.copies}
              onValueChange={(v) => update({ copies: Math.round(v) })}
              min={1}
              max={999}
              precision={0}
              width={84}
              aria-label={t('Copies')}
            />
          </Field>
        </div>
        {done ? (
          <p className="kiln-print__done" role="status">
            {done}
          </p>
        ) : null}

        <h2 className="kiln-print__h2">{t('Printer')}</h2>
        <Select
          aria-label={t('Printer')}
          value={destination}
          onValueChange={(v) => setDestination(v as PrintDestination)}
          options={[
            { value: 'printer', label: t('System printer (print dialog)'), textValue: t('System printer') },
            { value: 'pdf', label: t('Save as PDF'), textValue: t('Save as PDF') },
          ]}
          width="100%"
        />

        <h2 className="kiln-print__h2">{t('Settings')}</h2>
        <Select
          aria-label={slideOptions ? t('Which slides') : t('Which pages')}
          value={mode}
          onValueChange={(v) => setMode(v as PageMode)}
          options={
            slideOptions
              ? [
                  { value: 'all', label: t('Print All Slides') },
                  { value: 'custom', label: t('Custom Range') },
                ]
              : [
                  { value: 'all', label: t('Print All Pages') },
                  { value: 'custom', label: t('Custom Print') },
                  { value: 'odd', label: t('Only Odd Pages') },
                  { value: 'even', label: t('Only Even Pages') },
                ]
          }
          width="100%"
        />
        <Field
          label={slideOptions ? t('Slides') : t('Pages')}
          htmlFor={`${ids}-pages`}
          hint={
            pagesProblem ? (
              <span className="kiln-print__error" role="alert">
                {pagesProblem}
              </span>
            ) : (
              <span className="kiln-print__hint">{t('For example 1-3, 5, 8-')}</span>
            )
          }
        >
          <input
            id={`${ids}-pages`}
            className={cx('kiln-field kiln-print__input', pagesProblem && 'is-invalid')}
            value={pagesText}
            placeholder={t('All')}
            inputMode="numeric"
            aria-invalid={pagesProblem ? true : undefined}
            data-testid="print-pages"
            onChange={(e) => setPagesText(e.currentTarget.value)}
            onBlur={() => update({ pages: pagesText, parity: settings.parity })}
            onKeyDown={(e) => {
              if (e.key === 'Enter') update({ pages: pagesText, parity: settings.parity });
            }}
          />
        </Field>
        {slideOptions ? (
          <Select
            aria-label={t('Print layout')}
            value={slideLayoutOf(settings)}
            onValueChange={(v) => update(withSlideLayout(settings, v))}
            options={(['Print Layout', t('Handouts')] as const).map((group) => ({
              label: group === 'Print Layout' ? t('Print Layout') : t('Handouts'),
              options: SLIDE_LAYOUTS.filter((l) => l.group === group).map((l) => ({
                value: l.value,
                label: tx(l.label),
              })),
            }))}
            width="100%"
          />
        ) : (
          <Select
            aria-label={t('Orientation')}
            value={settings.orientation ?? 'doc'}
            onValueChange={(v) =>
              update({ orientation: v === 'doc' ? null : (v as 'Portrait' | 'Landscape') })
            }
            options={[
              {
                value: 'doc',
                label: t('Orientation as in the document'),
                textValue: t('As in the document'),
              },
              { value: 'Portrait', label: t('Portrait Orientation') },
              { value: 'Landscape', label: t('Landscape Orientation') },
            ]}
            width="100%"
          />
        )}
        <Select
          aria-label={t('Paper size')}
          value={settings.paper ?? 'doc'}
          onValueChange={(v) => update({ paper: v === 'doc' ? null : (v as PrintSettings['paper']) })}
          options={[
            { value: 'doc', label: t('Paper as in the document'), textValue: t('As in the document') },
            ...PAPERS.map((p) => ({
              value: p.value,
              textValue: tx(p.label),
              label: (
                <span className="kiln-print__opt">
                  {t('Scale to {label}', { label: tx(p.label) })}
                  <small>{tx(p.size)}</small>
                </span>
              ),
            })),
          ]}
          width="100%"
        />
        {slideOptions ? null : (
          <>
            <Select
              aria-label={t('Margins')}
              value={settings.margins ?? 'doc'}
              onValueChange={(v) => update({ margins: v === 'doc' ? null : (v as PrintSettings['margins']) })}
              options={[
                { value: 'doc', label: t('Margins as in the document'), textValue: t('As in the document') },
                ...MARGINS.map((m) => ({
                  value: m.value,
                  textValue: t('{label} margins', { label: tx(m.label) }),
                  label: (
                    <span className="kiln-print__opt">
                      {t('{label} margins', { label: tx(m.label) })}
                      <small>{tx(m.detail)}</small>
                    </span>
                  ),
                })),
              ]}
              width="100%"
            />
            <Select
              aria-label={t('Pages per sheet')}
              value={String(settings.per_sheet)}
              onValueChange={(v) => update({ per_sheet: Number(v) })}
              options={PER_SHEET.map((n) => ({
                value: String(n),
                label: t('print.pagesPerSheet', { count: n }),
              }))}
              width="100%"
            />
            <Field label={t('Scale')} htmlFor={`${ids}-scale`}>
              <NumberInput
                id={`${ids}-scale`}
                value={Math.round(settings.scale * 100)}
                onValueChange={(v) => update({ scale: v / 100 })}
                unit="%"
                min={10}
                max={400}
                step={5}
                precision={0}
                width={110}
                aria-label={t('Scale')}
              />
            </Field>
          </>
        )}
        <div className="kiln-print__checks">
          <Check
            checked={settings.grayscale}
            onChange={(v) => update({ grayscale: v })}
            label={t('Print in grayscale')}
          />
          {slideOptions ? (
            <>
              <Check
                checked={slidesOf(settings).frame}
                onChange={(v) => update({ slides: { ...slidesOf(settings), frame: v } })}
                label={t('Frame Slides')}
              />
              <Check
                checked={slidesOf(settings).hidden}
                onChange={(v) => update({ slides: { ...slidesOf(settings), hidden: v } })}
                label={t('Print Hidden Slides')}
              />
            </>
          ) : null}
          {textOptions ? (
            <>
              <Check
                checked={settings.markup}
                onChange={(v) => update({ markup: v })}
                label={t('Print markup')}
                detail={t('Tracked changes as they show on screen')}
              />
              <Check
                checked={settings.hidden_text}
                onChange={(v) => update({ hidden_text: v })}
                label={t('Print hidden text')}
              />
            </>
          ) : null}
        </div>
      </section>

      <section className="kiln-print__preview" aria-label={t('Print preview')}>
        <div ref={box} className="kiln-print__stage">
          <PrintPreview scene={preview?.scene ?? null} label={label} busy={loading} />
          {problem && !pagesProblem ? (
            <p className="kiln-print__error kiln-print__stage-msg" role="alert">
              {problem}
            </p>
          ) : null}
        </div>
        <nav className="kiln-print__pager" aria-label={t('Preview pages')}>
          <IconButton
            label={t('Previous page')}
            icon={<Icon icon={ChevronLeft} size={18} />}
            disabled={!preview || sheet <= 0}
            onClick={() => setSheet((s) => Math.max(0, s - 1))}
          />
          <span className="kiln-print__pageno" aria-live="polite" data-testid="print-sheet">
            {preview ? label : '…'}
          </span>
          <IconButton
            label={t('Next page')}
            icon={<Icon icon={ChevronRight} size={18} />}
            disabled={!preview || sheet >= sheets - 1}
            onClick={() => setSheet((s) => Math.min(sheets - 1, s + 1))}
          />
        </nav>
      </section>
    </div>
  );
}
