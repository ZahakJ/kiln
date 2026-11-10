/**
 * Dialogs of the sheet's gestures: the error indicator's Help on this Error, and Excel's
 * question before dropped cells replace cells that hold data.
 */
import { t } from '@kiln/i18n';
import { Button, Dialog, DialogContent } from '@kiln/ui-kit';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';

/** Help on this Error: what the active cell's error means. */
export function ErrorHelpDialog({ model }: { model: GridEditorModel }) {
  const error = useGrid(model, (s) => s.grid?.surface?.error ?? null);
  const close = () => model.closeDialog();
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={error?.title ?? t('Error')}
        size="sm"
        data-testid="grid-error-help"
        data-kiln-shortcuts="off"
        onCloseAutoFocus={(e) => {
          // The keys go back to the sheet.
          e.preventDefault();
          model.surface?.focus();
        }}
        footer={
          <Button variant="primary" onClick={close}>
            {t('OK')}
          </Button>
        }
      >
        <p className="kiln-griddlg__text">
          {error ? (
            <>
              <strong>{error.error}</strong> {error.help}
            </>
          ) : (
            t('The active cell shows no error.')
          )}
        </p>
      </DialogContent>
    </Dialog>
  );
}

/** "There's already data here. Do you want to replace it?" before a drop overwrites cells. */
export function ReplaceCellsDialog({ model }: { model: GridEditorModel }) {
  const close = () => {
    model.pendingDrop = null;
    model.closeDialog();
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={t('Kiln Grid')}
        size="sm"
        data-testid="grid-replace-cells"
        data-kiln-shortcuts="off"
        onCloseAutoFocus={(e) => {
          // The keys go back to the sheet.
          e.preventDefault();
          model.surface?.focus();
        }}
        footer={
          <>
            <Button onClick={close}>{t('Cancel')}</Button>
            <Button
              variant="primary"
              autoFocus
              onClick={async () => {
                const args = model.pendingDrop;
                close();
                if (args) await model.run('grid.editing.drag_cells', args);
              }}
            >
              {t('OK')}
            </Button>
          </>
        }
      >
        <p className="kiln-griddlg__text">{t("There's already data here. Do you want to replace it?")}</p>
      </DialogContent>
    </Dialog>
  );
}
