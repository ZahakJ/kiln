/**
 * Share from the phone's overflow menu: send a copy of the document through the
 * system's share sheet (the Android app; the Web Share API in a browser that can share files) or,
 * where the browser cannot, download a copy; and share the document for live editing
 * (`suite.file.share`). A copy that cannot be sent says why in a toast, in plain words.
 */
import { t } from '@kiln/i18n';
import { Download, FileText, type LucideIcon, Send, Share2 } from 'lucide-react';
import { useState } from 'react';
import { useCommandMeta, useCommandState, useExecute } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { useToast } from '../primitives/Toast.tsx';
import { BottomSheet } from './BottomSheet.tsx';

/** What a copy is: the file as last saved, or a PDF of the document as it is now. */
export type ShareCopyKind = 'file' | 'pdf';

export interface ShareCopy {
  /** `share`: the system's share sheet; `download`: the browser saves the copy. */
  mode: 'share' | 'download';
  /** The file's name ("Budget.xlsx"), shown on the first choice. */
  fileName?: string;
  /** Sends or downloads the copy; rejects with a message to show (save first, nothing took it). */
  send: (kind: ShareCopyKind) => Promise<void>;
}

export interface ShareSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  copy?: ShareCopy;
}

function Choice({
  icon,
  title,
  detail,
  onSelect,
  disabled,
  testId,
}: {
  icon: LucideIcon;
  title: string;
  detail?: string;
  onSelect: () => void;
  disabled?: boolean;
  testId?: string;
}) {
  return (
    <button
      type="button"
      className="kiln-share-sheet__choice"
      aria-disabled={disabled || undefined}
      data-testid={testId}
      onClick={() => !disabled && onSelect()}
    >
      <span className="kiln-share-sheet__icon" aria-hidden="true">
        <Icon icon={icon} size={20} />
      </span>
      <span className="kiln-share-sheet__text">
        <span className="kiln-share-sheet__title">{title}</span>
        {detail ? <span className="kiln-share-sheet__detail">{detail}</span> : null}
      </span>
    </button>
  );
}

export function ShareSheet({ open, onOpenChange, copy }: ShareSheetProps) {
  const live = useCommandState('suite.file.share');
  const liveMeta = useCommandMeta('suite.file.share');
  const execute = useExecute();
  const { toast } = useToast();
  const [busy, setBusy] = useState<ShareCopyKind | null>(null);

  const send = (kind: ShareCopyKind) => {
    if (!copy || busy) return;
    setBusy(kind);
    copy
      .send(kind)
      .then(
        () => onOpenChange(false),
        (error: unknown) => {
          const message = error instanceof Error && error.message ? error.message : '';
          toast({
            id: 'kiln-share',
            tone: 'warning',
            title: message || t('The copy could not be sent'),
            description: message ? undefined : t('Try again, or save the document and share the file.'),
            duration: 6000,
          });
        },
      )
      .finally(() => setBusy(null));
  };

  const download = copy?.mode === 'download';
  return (
    <BottomSheet
      open={open}
      onOpenChange={onOpenChange}
      title={t('Share')}
      className="kiln-share-sheet"
      testId="share-sheet"
    >
      <div className="kiln-share-sheet__choices">
        {copy ? (
          <>
            <Choice
              icon={download ? Download : Send}
              title={download ? t('Download a copy') : t('Send a copy')}
              detail={copy.fileName ? t('{name}, as last saved', { name: copy.fileName }) : undefined}
              disabled={busy !== null}
              testId="share-file"
              onSelect={() => send('file')}
            />
            <Choice
              icon={FileText}
              title={download ? t('Download as PDF') : t('Send as PDF')}
              detail={t('The document as it is now')}
              disabled={busy !== null}
              testId="share-pdf"
              onSelect={() => send('pdf')}
            />
          </>
        ) : null}
        {live.visible ? (
          <Choice
            icon={Share2}
            title={t('Share for live editing')}
            detail={liveMeta?.description}
            disabled={!live.enabled}
            testId="share-live"
            onSelect={() => {
              onOpenChange(false);
              execute('suite.file.share');
            }}
          />
        ) : null}
      </div>
    </BottomSheet>
  );
}
