/**
 * The question Office asks before a document with unsaved changes closes: Save, Don't Save
 * (the changes and their recovery copy go), or Cancel (the document stays open).
 */

import { t } from '@kiln/i18n';
import { Message } from '@kiln/i18n/react';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import './recovery.css';

export interface SaveChangesDialogProps {
  open: boolean;
  /** The document's name (`Report.docx`, or `Document1` before its first save). */
  name: string;
  appName: string;
  onSave: () => void;
  onDontSave: () => void;
  onCancel: () => void;
}

export function SaveChangesDialog({
  open,
  name,
  appName,
  onSave,
  onDontSave,
  onCancel,
}: SaveChangesDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
    >
      <DialogContent
        title={appName}
        size="md"
        role="alertdialog"
        data-testid="save-changes"
        data-kiln-shortcuts="off"
        footer={
          <>
            <Button variant="primary" onClick={onSave} data-testid="save-changes-save">
              {t('Save')}
            </Button>
            <Button onClick={onDontSave} data-testid="save-changes-discard">
              {t("Don't Save")}
            </Button>
            <Button onClick={onCancel}>{t('Cancel')}</Button>
          </>
        }
      >
        <p>
          <Message
            id="Do you want to save the changes you made to {name}?"
            values={{ name: <strong>{name}</strong> }}
          />
        </p>
        <p>{t("Your changes will be lost if you don't save them.")}</p>
      </DialogContent>
    </Dialog>
  );
}
