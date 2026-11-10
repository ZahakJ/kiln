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
import { InsertSheetDialog, MoveCopySheetDialog, UnhideSheetDialog } from './dialogs/SheetDialogs.tsx';
import { ErrorHelpDialog, ReplaceCellsDialog } from './dialogs/SurfaceDialogs.tsx';
import { RecommendedChartsForSelection } from './GridCharts.tsx';
import { CustomListsDialog, SeriesDialog } from './home/HomeDialogs.tsx';
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
    case 'recommendedCharts':
      return <RecommendedChartsForSelection model={model} />;
    case 'fillSeries':
      return <SeriesDialog model={model} />;
    case 'customLists':
      return <CustomListsDialog model={model} />;
    case 'errorHelp':
      return <ErrorHelpDialog model={model} />;
    case 'replaceCells':
      return <ReplaceCellsDialog model={model} />;
    case 'insertSheet':
      return <InsertSheetDialog model={model} />;
    case 'moveCopySheet':
      return <MoveCopySheetDialog model={model} sheet={arg ?? active} />;
    case 'unhideSheet':
      return <UnhideSheetDialog model={model} />;
    case 'renameSheet':
      // The sheet tab strip edits the name in place (SheetTabs).
      return null;
  }
}
