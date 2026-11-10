/**
 * File ▸ Export ▸ PDF: the PDF's options (pages, accessibility tags, bookmarks, markup, hidden
 * text, grayscale; for a presentation, what to publish — slides, handouts, notes pages or the
 * outline — Frame Slides and Include Hidden Slides) and the Export button. After an export the pane says whether the file came
 * out as PDF/UA-1 (accessible) and, if not, what in the document prevented it — as the core
 * reports it.
 */
import { t, tx } from '@kiln/i18n';
import type { PrintSettings } from '@kiln/protocol';
import { Accessibility, FileDown } from 'lucide-react';
import { useId, useState } from 'react';
import { errorText } from '../../errorText.ts';
import { Icon } from '../../icons/Icon.tsx';
import { Button } from '../../primitives/Button.tsx';
import { Select } from '../../primitives/Select.tsx';
import {
  DEFAULT_PRINT_SETTINGS,
  SLIDE_LAYOUTS,
  slideLayoutOf,
  slidesOf,
  withSlideLayout,
} from './settings.ts';
import './print.css';

/** What an export reported. */
export interface PdfExportOutcome {
  /** The file name or path it was saved as. */
  name: string;
  /** It is PDF/UA-1. */
  pdfUa: boolean;
  /** Why not. */
  uaIssues: readonly string[];
}

export interface ExportPdfPaneProps {
  /** Exports with these settings; resolves to the outcome, or `null` when the user cancelled. */
  onExport: (settings: PrintSettings) => Promise<PdfExportOutcome | null>;
  /** Show the Write options (markup, hidden text). */
  textOptions?: boolean;
  /** Kiln Deck: slides instead of pages, Publish What and the slide options. */
  slideOptions?: boolean;
}

function Option({
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

export function ExportPdfPane({ onExport, textOptions = true, slideOptions = false }: ExportPdfPaneProps) {
  const ids = useId();
  const [s, setS] = useState<PrintSettings>(DEFAULT_PRINT_SETTINGS);
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<PdfExportOutcome | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const set = (patch: Partial<PrintSettings>) => {
    setS((v) => ({ ...v, ...patch }));
    setOutcome(null);
  };

  const run = async () => {
    setBusy(true);
    setProblem(null);
    try {
      const r = await onExport(s);
      if (r) setOutcome(r);
    } catch (error) {
      setProblem(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="kiln-pdf-export" data-testid="pdf-export">
      <section className="kiln-pdf-export__card" aria-labelledby={`${ids}-h`}>
        <h2 id={`${ids}-h`} className="kiln-bs-h2">
          {t('Export as PDF')}
        </h2>
        <p className="kiln-bs-muted">
          {slideOptions
            ? t(
                'Exactly as it prints: real, searchable text with the fonts embedded and a bookmark for every slide.',
              )
            : t(
                'Exactly as it prints: real, searchable text with the fonts embedded, links that work and bookmarks from your headings.',
              )}
        </p>
        {slideOptions ? (
          <div className="kiln-print__field">
            <span className="kiln-print__label" id={`${ids}-what`}>
              {t('Publish what')}
            </span>
            <Select
              labelId={`${ids}-what`}
              value={slideLayoutOf(s)}
              onValueChange={(v) => set(withSlideLayout(s, v))}
              options={(['Print Layout', t('Handouts')] as const).map((group) => ({
                label: group === 'Print Layout' ? t('Publish') : t('Handouts'),
                options: SLIDE_LAYOUTS.filter((l) => l.group === group).map((l) => ({
                  value: l.value,
                  label: l.value === 'full' ? t('Slides') : tx(l.label),
                })),
              }))}
              width="100%"
            />
          </div>
        ) : null}
        <div className="kiln-print__field">
          <label className="kiln-print__label" htmlFor={`${ids}-pages`}>
            {slideOptions ? t('Slides') : t('Pages')}
          </label>
          <input
            id={`${ids}-pages`}
            className="kiln-field kiln-print__input"
            placeholder={t('All')}
            value={s.pages}
            inputMode="numeric"
            onChange={(e) => set({ pages: e.currentTarget.value })}
          />
          <span className="kiln-print__hint">{t('For example 1-3, 5, 8-')}</span>
        </div>
        <div className="kiln-print__checks">
          <Option
            checked={s.tagged}
            onChange={(v) => set({ tagged: v })}
            label={t('Document structure tags for accessibility')}
            detail={t('Headings, tables, alt text and reading order for screen readers (PDF/UA)')}
          />
          <Option
            checked={s.bookmarks}
            onChange={(v) => set({ bookmarks: v })}
            label={slideOptions ? t('Create bookmarks for slides') : t('Create bookmarks from headings')}
          />
          {textOptions ? (
            <>
              <Option checked={s.markup} onChange={(v) => set({ markup: v })} label={t('Include markup')} />
              <Option
                checked={s.hidden_text}
                onChange={(v) => set({ hidden_text: v })}
                label={t('Include hidden text')}
              />
            </>
          ) : null}
          {slideOptions ? (
            <>
              <Option
                checked={slidesOf(s).frame}
                onChange={(v) => set({ slides: { ...slidesOf(s), frame: v } })}
                label={t('Frame slides')}
              />
              <Option
                checked={slidesOf(s).hidden}
                onChange={(v) => set({ slides: { ...slidesOf(s), hidden: v } })}
                label={t('Include hidden slides')}
              />
            </>
          ) : null}
          <Option checked={s.grayscale} onChange={(v) => set({ grayscale: v })} label={t('Grayscale')} />
        </div>
        <div>
          <Button
            variant="primary"
            icon={<Icon icon={FileDown} size={16} />}
            onClick={() => void run()}
            busy={busy}
            disabled={busy}
            data-testid="pdf-export-go"
            // File ▸ Export ▸ PDF is Save as PDF (the desktop shell writes the file itself).
            data-command="suite.file.save_pdf"
          >
            {t('Export PDF')}
          </Button>
        </div>
        {problem ? (
          <p className="kiln-print__error" role="alert">
            {problem}
          </p>
        ) : null}
      </section>
      {outcome ? (
        <section className="kiln-pdf-export__card" aria-live="polite" data-testid="pdf-export-result">
          <h2 className="kiln-bs-h2">{t('Saved {name}', { name: outcome.name })}</h2>
          {outcome.pdfUa ? (
            <p className="kiln-pdf-export__badge">
              <Icon icon={Accessibility} size={16} /> {t('Accessible PDF (PDF/UA-1)')}
            </p>
          ) : s.tagged ? (
            <>
              <p className="kiln-bs-muted">
                {t('Tagged for screen readers. To make it fully accessible (PDF/UA-1):')}
              </p>
              <ul className="kiln-pdf-export__issues">
                {outcome.uaIssues.map((i) => (
                  <li key={i}>{i}</li>
                ))}
              </ul>
            </>
          ) : null}
        </section>
      ) : null}
    </div>
  );
}
