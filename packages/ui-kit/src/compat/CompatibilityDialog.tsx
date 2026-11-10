/**
 * File ▸ Info ▸ Check for Issues ▸ Check Compatibility: what saving in a format will lose or
 * change, and what was kept as it was read (`Query::CompatReport` per target format).
 */
import { formatNumber, t } from '@kiln/i18n';
import type { CompatIssue, CompatSeverity, FileFormat } from '@kiln/protocol';
import { CircleX, Info, RefreshCw, ShieldCheck, TriangleAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { cx } from '../primitives/cx.ts';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import './compat.css';

export interface CompatFormat {
  format: FileFormat;
  /** "Word document". */
  label: string;
}

export interface CompatibilityDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The formats the document can be saved in (the first is checked first, unless `initial`). */
  formats: readonly CompatFormat[];
  initial?: FileFormat;
  check: (format: FileFormat) => Promise<CompatIssue[]>;
}

const GROUPS: readonly { severity: CompatSeverity; icon: typeof Info }[] = [
  { severity: 'Loss', icon: CircleX },
  { severity: 'Change', icon: TriangleAlert },
  { severity: 'Info', icon: Info },
];

function groupTitle(s: CompatSeverity): string {
  switch (s) {
    case 'Loss':
      return t('Will be lost');
    case 'Change':
      return t('Will be saved differently');
    case 'Info':
      return t('Kept as it was read');
  }
}

export function CompatibilityDialog({
  open,
  onOpenChange,
  formats,
  initial,
  check,
}: CompatibilityDialogProps) {
  const [format, setFormat] = useState<FileFormat | undefined>(initial ?? formats[0]?.format);
  const [issues, setIssues] = useState<CompatIssue[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (open) setFormat(initial ?? formats[0]?.format);
  }, [open, initial, formats]);

  useEffect(() => {
    if (!open || !format) return;
    let live = true;
    setIssues(null);
    setFailed(false);
    check(format).then(
      (r) => live && setIssues(r),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [open, format, attempt]);

  const ext = format ? `.${format.toLowerCase()}` : '';
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('Compatibility Checker')}
        size="lg"
        className="kiln-compat"
        data-testid="compat-dialog"
        footer={
          <>
            <Button icon={<Icon icon={RefreshCw} size={15} />} onClick={() => setAttempt((a) => a + 1)}>
              {t('Check again')}
            </Button>
            <Button variant="primary" onClick={() => onOpenChange(false)}>
              {t('Done')}
            </Button>
          </>
        }
      >
        <p className="kiln-compat__intro">
          {t('Choose the format you will share in. Kiln lists what that format cannot hold.')}
        </p>
        <div className="kiln-compat__formats" role="radiogroup" aria-label={t('Check for')}>
          {formats.map((f) => (
            // biome-ignore lint/a11y/useSemanticElements: a segmented control of buttons, as the ribbon's
            <button
              key={f.format}
              type="button"
              role="radio"
              aria-checked={f.format === format}
              className={cx('kiln-compat__format', f.format === format && 'is-on')}
              onClick={() => setFormat(f.format)}
            >
              <span className="kiln-compat__ext">.{f.format.toLowerCase()}</span>
              <span>{f.label}</span>
            </button>
          ))}
        </div>
        <div className="kiln-compat__results" aria-live="polite" aria-busy={issues === null && !failed}>
          {failed ? (
            <div className="kiln-compat__state" role="alert">
              <strong>{t('The check did not finish.')}</strong>
              <span>{t('Nothing in the document changed. Check again, or save a copy first.')}</span>
            </div>
          ) : issues === null ? (
            <p className="kiln-compat__loading" role="status">
              {t('Checking…')}
            </p>
          ) : issues.length === 0 ? (
            <div className="kiln-compat__clear">
              <span className="kiln-compat__clear-icon">
                <Icon icon={ShieldCheck} size={28} />
              </span>
              <strong>{t('Nothing will be lost in {ext}', { ext })}</strong>
              <span>{t('Everything in this document can be saved in this format as it is.')}</span>
            </div>
          ) : (
            GROUPS.map(({ severity, icon }) => {
              const list = issues.filter((i) => i.severity === severity);
              if (list.length === 0) return null;
              return (
                <section key={severity} className="kiln-compat__group" data-severity={severity}>
                  <h3 className="kiln-compat__group-title">
                    <Icon icon={icon} size={16} />
                    {groupTitle(severity)}
                  </h3>
                  <ul>
                    {list.map((i, n) => (
                      <li key={`${i.code}-${n}`} data-code={i.code}>
                        <span className="kiln-compat__msg">{i.message}</span>
                        <span className="kiln-compat__meta">
                          {i.origin === 'Open' ? t('When the file was opened') : t('When saving')}
                        </span>
                        {i.count > 1 ? (
                          <span className="kiln-compat__pill" title={t('Occurrences')}>
                            × {formatNumber(i.count)}
                          </span>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                </section>
              );
            })
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
