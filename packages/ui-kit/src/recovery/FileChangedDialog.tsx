/**
 * The question Office asks when another program changed the open file: reload it (the changes
 * made here are dropped) or keep working on this copy (the next save writes over the file).
 * Closing the question answers nothing: nothing is written to the file until it is answered.
 */

import { t } from '@kiln/i18n';
import { Message } from '@kiln/i18n/react';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';

export interface FileChangedDialogProps {
  open: boolean;
  /** The file's name (`Report.docx`). */
  name: string;
  appName: string;
  /** Whether the document has changes of its own that reloading drops. */
  dirty: boolean;
  onReload: () => void;
  onKeep: () => void;
  /** The question closed without an answer. */
  onDismiss?: () => void;
}

export function FileChangedDialog({
  open,
  name,
  appName,
  dirty,
  onReload,
  onKeep,
  onDismiss,
}: FileChangedDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) onDismiss?.();
      }}
    >
      <DialogContent
        title={appName}
        size="md"
        role="alertdialog"
        data-testid="file-changed"
        data-kiln-shortcuts="off"
        footer={
          <>
            <Button variant="primary" onClick={onReload} data-testid="file-changed-reload">
              {t('Reload')}
            </Button>
            <Button onClick={onKeep} data-testid="file-changed-keep">
              {t('Keep my version')}
            </Button>
          </>
        }
      >
        <p>
          <Message id="{name} was changed by another program." values={{ name: <strong>{name}</strong> }} />
        </p>
        <p>
          {dirty
            ? t('Reload it to see those changes? The changes you made here since the last save will be lost.')
            : t('Reload it to see those changes?')}
        </p>
        <p>{t("If you keep your version, saving it replaces the other program's changes.")}</p>
      </DialogContent>
    </Dialog>
  );
}
