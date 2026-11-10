/**
 * Document Recovery (Office's Document Recovery pane): the documents Kiln kept when it closed
 * before they were saved, newest first. Each can be opened (it opens marked as changed, so the
 * next save keeps it), opened and saved under a name, or deleted. A copy Kiln cannot open (one a
 * newer version wrote) says why and can only be deleted. The pane lists what the core's
 * `Event::Recovery` says; the frame shows it docked on the left when there is anything to recover.
 */
import { t } from '@kiln/i18n';
import type { DocKind, FileFormat, RecoveryEntry } from '@kiln/protocol';
import { FolderOpen, Save, Trash2 } from 'lucide-react';
import { when } from '../collab/VersionHistoryPane.tsx';
import { AppTile, type BrandApp } from '../icons/brand/BrandMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import './recovery.css';

export interface DocumentRecoveryPaneProps {
  entries: readonly RecoveryEntry[];
  onOpen: (entry: RecoveryEntry) => void;
  onSaveAs: (entry: RecoveryEntry) => void;
  onDelete: (entry: RecoveryEntry) => void;
  /** Closes the pane (the documents stay for next time). */
  onClose: () => void;
  /** The clock, for "Today" and "Yesterday". */
  now?: number;
}

const APP: Record<DocKind, BrandApp> = {
  Scratch: 'write',
  Write: 'write',
  Grid: 'grid',
  Deck: 'deck',
  Notes: 'notes',
};

const DEFAULT_EXT: Partial<Record<DocKind, string>> = { Write: 'docx', Grid: 'xlsx', Deck: 'pptx' };

/**
 * The file name shown: the file's own; for a document never saved, its untitled name ("Book1"),
 * as the title bar shows it, with no extension it never had.
 */
function fileName(e: RecoveryEntry): { stem: string; ext: string } {
  const name = e.path?.split(/[/\\]/).pop();
  if (name?.includes('.')) {
    const dot = name.lastIndexOf('.');
    return { stem: name.slice(0, dot), ext: name.slice(dot) };
  }
  if (!e.path) return { stem: e.title, ext: '' };
  const ext = (e.format as FileFormat | null)?.toLowerCase() ?? DEFAULT_EXT[e.kind] ?? '';
  return { stem: e.title, ext: ext ? `.${ext}` : '' };
}

/** Where the document's file is: its folders, "This device" for a file picked in the browser,
 * "This browser" for one saved in the browser's own storage, or none yet. */
function placeOf(path: string | null): string {
  if (!path) return t('Never saved to a file');
  if (path.startsWith('handle:')) return t('This device');
  if (path.startsWith('opfs:')) return t('This browser');
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts.slice(-3, -1).join(' › ') || t('This device');
}

/**
 * Why a copy cannot be opened, in written words: the core's reason says what the journal holds
 * (a format number, a parser's complaint), which is not the user's to read.
 */
export function problemOf(problem: string): string {
  if (/newer version of Kiln/i.test(problem))
    return t('A newer version of Kiln kept this copy. Update Kiln to open it.');
  if (/could not be kept/i.test(problem))
    return t('Kiln closed before it could keep a copy, so the changes are lost. You can delete it.');
  return t('This copy is damaged and cannot be opened. You can delete it.');
}

export function DocumentRecoveryPane({
  entries,
  onOpen,
  onSaveAs,
  onDelete,
  onClose,
  now = Date.now(),
}: DocumentRecoveryPaneProps) {
  return (
    <div className="kiln-recovery" data-testid="document-recovery">
      <p className="kiln-recovery__intro">
        {t(
          'Kiln kept these documents when it closed before they were saved. Open the ones you want to keep and save them.',
        )}
      </p>
      <h3 className="kiln-recovery__label" id="kiln-recovery-files">
        {t('Available files')}
      </h3>
      <ul className="kiln-recovery__list" aria-labelledby="kiln-recovery-files">
        {entries.map((e) => {
          const { stem, ext } = fileName(e);
          const label = `${stem}${ext}`;
          return (
            <li key={e.key} className="kiln-recovery__item" data-recovery={e.key}>
              <div className="kiln-recovery__head">
                <AppTile app={APP[e.kind]} size={32} />
                <div className="kiln-recovery__name">
                  <strong>
                    {stem}
                    <span className="kiln-recovery__ext">{ext}</span>
                  </strong>
                  <span className="kiln-recovery__meta">
                    {e.problem
                      ? problemOf(e.problem)
                      : e.saved_at_ms > 0
                        ? t('Recovery copy from {when}', { when: when(e.saved_at_ms, now) })
                        : t('Recovery copy')}
                  </span>
                  <span className="kiln-recovery__meta">{placeOf(e.path)}</span>
                </div>
              </div>
              <div className="kiln-recovery__actions">
                {e.problem ? null : (
                  <>
                    <Button
                      size="sm"
                      variant="primary"
                      icon={<Icon icon={FolderOpen} size={14} />}
                      onClick={() => onOpen(e)}
                      aria-label={t('Open {label}', { label })}
                      data-testid="recovery-open"
                    >
                      {t('Open')}
                    </Button>
                    <Button
                      size="sm"
                      icon={<Icon icon={Save} size={14} />}
                      onClick={() => onSaveAs(e)}
                      aria-label={t('Save As… {label}', { label })}
                    >
                      {t('Save As…')}
                    </Button>
                  </>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Icon icon={Trash2} size={14} />}
                  onClick={() => onDelete(e)}
                  aria-label={t('Delete {label}', { label })}
                  data-testid="recovery-delete"
                >
                  {t('Delete')}
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
      <div className="kiln-recovery__foot">
        <p>{t('Documents you do not open or delete are kept for the next time Kiln starts.')}</p>
        <Button onClick={onClose}>{t('Close')}</Button>
      </div>
    </div>
  );
}
