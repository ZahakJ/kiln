/** The dialog the Grid editor's model has open (one at a time, like Excel's modal dialogs). */
import {
  GoToDialog,
  InsertDeleteDialog,
  InsertFunctionDialog,
  PasteSpecialDialog,
  SizeDialog,
  TabColorDialog,
} from './dialogs/CellDialogs.tsx';
import { FormatCellsDialog } from './dialogs/FormatCellsDialog.tsx';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

export function GridDialogs({ model }: { model: GridEditorModel }) {
  const dialog = useGrid(model, (s) => s.dialog);
  const active = useGrid(model, (s) => s.grid?.active_sheet ?? 0);
  if (!dialog) return null;
  const { id, tab, arg } = dialog;
  switch (id) {
    case 'formatCells':
      return <FormatCellsDialog model={model} tab={tab} onClose={() => model.closeDialog()} />;
    case 'goTo':
      return <GoToDialog model={model} />;
    case 'pasteSpecial':
      return <PasteSpecialDialog model={model} />;
    case 'insertCells':
      return <InsertDeleteDialog model={model} mode="insert" />;
    case 'deleteCells':
      return <InsertDeleteDialog model={model} mode="delete" />;
    case 'rowHeight':
      return <SizeDialog model={model} what="row" />;
    case 'columnWidth':
      return <SizeDialog model={model} what="column" />;
    case 'tabColor':
      return <TabColorDialog model={model} sheet={arg ?? active} />;
    case 'insertFunction':
      return <InsertFunctionDialog model={model} category={tab} />;
    case 'renameSheet':
      // The sheet tab strip edits the name in place (SheetTabs).
      return null;
  }
}
