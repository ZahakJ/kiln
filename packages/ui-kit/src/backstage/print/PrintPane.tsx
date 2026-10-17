/**
 * File ▸ Print (PLAN.md §1.5): settings on the left — copies and the Print button, the printer,
 * pages, orientation, paper, margins, pages per sheet, scaling and the Write options — and the
 * live preview of the sheet on the right, painted from the core's Scene. Every change asks the
 * core for a new preview (debounced); page boxes the core cannot read are reported under the
 * field. Printing and saving are callbacks the frame turns into the platform's print path.
 */
import type { PrintPreview as PrintPreviewData, PrintSettings } from '@kiln/protocol';
import { ChevronLeft, ChevronRight, FileDown, Printer } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from 'react';
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
  sheetLabel,
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
          setProblem(error instanceof Error ? error.message : String(error));
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
        setDone('PDF saved.');
      } else {
        await onPrint(settings);
        setDone('Sent to the print dialog.');
      }
      setProblem(null);
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error));
    } finally {
      setWorking(false);
    }
  };

  const sheets = preview?.sheets ?? 0;
  const label = preview
    ? sheetLabel(preview.sheet, preview.sheets, preview.pages, settings.per_sheet)
    : 'Preparing the preview';
  const pagesProblem = problem && /page/i.test(problem) ? problem : null;

  return (
    <div className={cx('kiln-print', className)} data-testid="print-pane">
      <section className="kiln-print__settings" aria-label="Print settings">
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
            {destination === 'pdf' ? 'Save PDF' : 'Print'}
          </Button>
          <Field label="Copies" htmlFor={`${ids}-copies`}>
            <NumberInput
              id={`${ids}-copies`}
              value={settings.copies}
              onValueChange={(v) => update({ copies: Math.round(v) })}
              min={1}
              max={999}
              precision={0}
              width={84}
              aria-label="Copies"
            />
          </Field>
        </div>
        {done ? (
          <p className="kiln-print__done" role="status">
            {done}
          </p>
        ) : null}

        <h2 className="kiln-print__h2">Printer</h2>
        <Select
          aria-label="Printer"
          value={destination}
          onValueChange={(v) => setDestination(v as PrintDestination)}
          options={[
            { value: 'printer', label: 'System printer (print dialog)', textValue: 'System printer' },
            { value: 'pdf', label: 'Save as PDF', textValue: 'Save as PDF' },
          ]}
          width="100%"
        />

        <h2 className="kiln-print__h2">Settings</h2>
        <Select
          aria-label="Which pages"
          value={mode}
          onValueChange={(v) => setMode(v as PageMode)}
          options={[
            { value: 'all', label: 'Print All Pages' },
            { value: 'custom', label: 'Custom Print' },
            { value: 'odd', label: 'Only Odd Pages' },
            { value: 'even', label: 'Only Even Pages' },
          ]}
          width="100%"
        />
        <Field
          label="Pages"
          htmlFor={`${ids}-pages`}
          hint={
            pagesProblem ? (
              <span className="kiln-print__error" role="alert">
                {pagesProblem}
              </span>
            ) : (
              <span className="kiln-print__hint">For example 1-3, 5, 8-</span>
            )
          }
        >
          <input
            id={`${ids}-pages`}
            className={cx('kiln-field kiln-print__input', pagesProblem && 'is-invalid')}
            value={pagesText}
            placeholder="All"
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
        <Select
          aria-label="Orientation"
          value={settings.orientation ?? 'doc'}
          onValueChange={(v) => update({ orientation: v === 'doc' ? null : (v as 'Portrait' | 'Landscape') })}
          options={[
            { value: 'doc', label: 'Orientation as in the document', textValue: 'As in the document' },
            { value: 'Portrait', label: 'Portrait Orientation' },
            { value: 'Landscape', label: 'Landscape Orientation' },
          ]}
          width="100%"
        />
        <Select
          aria-label="Paper size"
          value={settings.paper ?? 'doc'}
          onValueChange={(v) => update({ paper: v === 'doc' ? null : (v as PrintSettings['paper']) })}
          options={[
            { value: 'doc', label: 'Paper as in the document', textValue: 'As in the document' },
            ...PAPERS.map((p) => ({
              value: p.value,
              textValue: p.label,
              label: (
                <span className="kiln-print__opt">
                  Scale to {p.label}
                  <small>{p.size}</small>
                </span>
              ),
            })),
          ]}
          width="100%"
        />
        <Select
          aria-label="Margins"
          value={settings.margins ?? 'doc'}
          onValueChange={(v) => update({ margins: v === 'doc' ? null : (v as PrintSettings['margins']) })}
          options={[
            { value: 'doc', label: 'Margins as in the document', textValue: 'As in the document' },
            ...MARGINS.map((m) => ({
              value: m.value,
              textValue: `${m.label} margins`,
              label: (
                <span className="kiln-print__opt">
                  {m.label} margins
                  <small>{m.detail}</small>
                </span>
              ),
            })),
          ]}
          width="100%"
        />
        <Select
          aria-label="Pages per sheet"
          value={String(settings.per_sheet)}
          onValueChange={(v) => update({ per_sheet: Number(v) })}
          options={PER_SHEET.map((n) => ({
            value: String(n),
            label: n === 1 ? '1 Page Per Sheet' : `${n} Pages Per Sheet`,
          }))}
          width="100%"
        />
        <Field label="Scale" htmlFor={`${ids}-scale`}>
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
            aria-label="Scale"
          />
        </Field>
        <div className="kiln-print__checks">
          <Check
            checked={settings.grayscale}
            onChange={(v) => update({ grayscale: v })}
            label="Print in grayscale"
          />
          {textOptions ? (
            <>
              <Check
                checked={settings.markup}
                onChange={(v) => update({ markup: v })}
                label="Print markup"
                detail="Tracked changes as they show on screen"
              />
              <Check
                checked={settings.hidden_text}
                onChange={(v) => update({ hidden_text: v })}
                label="Print hidden text"
              />
            </>
          ) : null}
        </div>
      </section>

      <section className="kiln-print__preview" aria-label="Print preview">
        <div ref={box} className="kiln-print__stage">
          <PrintPreview scene={preview?.scene ?? null} label={label} busy={loading} />
          {problem && !pagesProblem ? (
            <p className="kiln-print__error kiln-print__stage-msg" role="alert">
              {problem}
            </p>
          ) : null}
        </div>
        <nav className="kiln-print__pager" aria-label="Preview pages">
          <IconButton
            label="Previous page"
            icon={<Icon icon={ChevronLeft} size={18} />}
            disabled={!preview || sheet <= 0}
            onClick={() => setSheet((s) => Math.max(0, s - 1))}
          />
          <span className="kiln-print__pageno" aria-live="polite" data-testid="print-sheet">
            {preview ? label : '…'}
          </span>
          <IconButton
            label="Next page"
            icon={<Icon icon={ChevronRight} size={18} />}
            disabled={!preview || sheet >= sheets - 1}
            onClick={() => setSheet((s) => Math.min(sheets - 1, s + 1))}
          />
        </nav>
      </section>
    </div>
  );
}
