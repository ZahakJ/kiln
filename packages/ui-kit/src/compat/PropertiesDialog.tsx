/**
 * File ▸ Info ▸ Properties (Office's Advanced Properties ▸ Summary): the document's core
 * properties, read from and written to the core (`Query::Properties`, `suite.file.properties`),
 * which keeps them in `.docx`, `.xlsx`, `.pptx` and the native bundle. Kiln Write, Grid and Deck.
 */
import { formatDate, formatNumber, t } from '@kiln/i18n';
import type { DocPropertiesInfo, PropertiesEdit } from '@kiln/protocol';
import { CalendarClock, RefreshCw } from 'lucide-react';
import { useEffect, useId, useState } from 'react';
import { AppMark } from '../chrome/AppMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import type { CompatApp } from './types.ts';
import './compat.css';

/** The fields of the form, in Office's order. */
const FIELDS = [
  'title',
  'subject',
  'author',
  'manager',
  'company',
  'category',
  'keywords',
  'status',
] as const;
type FieldKey = (typeof FIELDS)[number] | 'description';

function label(key: FieldKey): string {
  switch (key) {
    case 'title':
      return t('Title');
    case 'subject':
      return t('Subject');
    case 'author':
      return t('Author');
    case 'manager':
      return t('Manager');
    case 'company':
      return t('Company');
    case 'category':
      return t('Category');
    case 'keywords':
      return t('Tags');
    case 'status':
      return t('Status');
    case 'description':
      return t('Comments');
  }
}

/** A W3CDTF stamp as the interface shows dates, or `null`. */
function when(stamp: string | null | undefined): string | null {
  if (!stamp) return null;
  const d = new Date(stamp);
  return Number.isNaN(d.getTime()) ? null : formatDate(d, { dateStyle: 'medium', timeStyle: 'short' });
}

export interface PropertiesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  app: CompatApp;
  /** The document's name (the title bar's). */
  name: string;
  load: () => Promise<DocPropertiesInfo>;
  /** Sends only the fields that changed. */
  save: (edit: PropertiesEdit) => Promise<void>;
  readOnly?: boolean;
}

export function PropertiesDialog({
  open,
  onOpenChange,
  app,
  name,
  load,
  save,
  readOnly,
}: PropertiesDialogProps) {
  const [info, setInfo] = useState<DocPropertiesInfo | null>(null);
  const [failed, setFailed] = useState(false);
  const [edit, setEdit] = useState<Partial<Record<FieldKey, string>>>({});
  const [busy, setBusy] = useState(false);
  const [saveFailed, setSaveFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    let live = true;
    setInfo(null);
    setFailed(false);
    setEdit({});
    setSaveFailed(false);
    load().then(
      (p) => live && setInfo(p),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [open, attempt]);

  const value = (key: FieldKey) => edit[key] ?? info?.[key] ?? '';
  const changed = Object.entries(edit).filter(([k, v]) => (info?.[k as FieldKey] ?? '') !== v);
  const ok = async () => {
    if (changed.length === 0) {
      onOpenChange(false);
      return;
    }
    setBusy(true);
    setSaveFailed(false);
    try {
      await save(Object.fromEntries(changed) as PropertiesEdit);
      onOpenChange(false);
    } catch {
      setSaveFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const stats: string[] = [];
  if (info) {
    if (app === 'write') {
      stats.push(t('{count, plural, one {# page} other {# pages}}', { count: info.pages }));
      stats.push(t('{count, plural, one {# word} other {# words}}', { count: info.words }));
      stats.push(t('{count, plural, one {# character} other {# characters}}', { count: info.characters }));
    } else if (app === 'grid') {
      stats.push(t('{count, plural, one {# worksheet} other {# worksheets}}', { count: info.sheets }));
    } else {
      stats.push(t('{count, plural, one {# slide} other {# slides}}', { count: info.slides }));
    }
  }
  const created = when(info?.created);
  const modified = when(info?.modified);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('Properties')}
        size="lg"
        className="kiln-compat kiln-props"
        data-testid="properties-dialog"
        footer={
          <>
            <Button onClick={() => onOpenChange(false)}>{t('Cancel')}</Button>
            <Button variant="primary" busy={busy} disabled={!info || readOnly} onClick={() => void ok()}>
              {t('OK')}
            </Button>
          </>
        }
      >
        <div className="kiln-props__head" data-app={app}>
          <span className="kiln-props__mark">
            <AppMark app={app} size={40} />
          </span>
          <div className="kiln-props__who">
            <strong className="kiln-props__name">{info?.title || name}</strong>
            {stats.length > 0 ? (
              <span className="kiln-props__stats">
                {stats.map((s) => (
                  <span key={s} className="kiln-compat__pill">
                    {s}
                  </span>
                ))}
              </span>
            ) : null}
          </div>
          {created || modified ? (
            <dl className="kiln-props__dates">
              <Icon icon={CalendarClock} size={16} />
              {created ? (
                <div>
                  <dt>{t('Created')}</dt>
                  <dd>{created}</dd>
                </div>
              ) : null}
              {modified ? (
                <div>
                  <dt>{t('Modified')}</dt>
                  <dd>{modified}</dd>
                </div>
              ) : null}
            </dl>
          ) : null}
        </div>
        {failed ? (
          <div className="kiln-compat__state" role="alert">
            <strong>{t('Kiln could not read the properties of this document.')}</strong>
            <span>{t('The document is still open and nothing was changed.')}</span>
            <Button icon={<Icon icon={RefreshCw} size={15} />} onClick={() => setAttempt((a) => a + 1)}>
              {t('Try again')}
            </Button>
          </div>
        ) : !info ? (
          <p className="kiln-compat__loading" role="status">
            {t('Reading the properties…')}
          </p>
        ) : (
          <div className="kiln-props__form">
            {FIELDS.map((key) => (
              <div key={key} className="kiln-props__field">
                <label htmlFor={`${id}-${key}`}>{label(key)}</label>
                <input
                  id={`${id}-${key}`}
                  className="kiln-compat__input"
                  data-field={key}
                  value={value(key)}
                  readOnly={readOnly}
                  maxLength={1024}
                  onChange={(e) => setEdit({ ...edit, [key]: e.target.value })}
                />
              </div>
            ))}
            <div className="kiln-props__field kiln-props__field--wide">
              <label htmlFor={`${id}-description`}>{label('description')}</label>
              <textarea
                id={`${id}-description`}
                className="kiln-compat__input kiln-compat__textarea"
                data-field="description"
                rows={3}
                value={value('description')}
                readOnly={readOnly}
                maxLength={1024}
                onChange={(e) => setEdit({ ...edit, description: e.target.value })}
              />
            </div>
            {info.last_modified_by || info.revision ? (
              <p className="kiln-props__foot">
                {info.last_modified_by ? t('Last modified by {name}', { name: info.last_modified_by }) : null}
                {info.last_modified_by && info.revision ? ' · ' : null}
                {info.revision
                  ? t('Revision {number}', { number: formatNumber(Number(info.revision) || 0) })
                  : null}
              </p>
            ) : null}
            {saveFailed ? (
              <p className="kiln-compat__warn" role="alert">
                {t('The properties were not saved. Check that the document can be edited, then try again.')}
              </p>
            ) : null}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
