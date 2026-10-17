/** The dialog the editor's model has open (one at a time, like Word's modal dialogs). */
import { FontDialog } from './dialogs/FontDialog.tsx';
import {
  BookmarkDialog,
  DateTimeDialog,
  LinkDialog,
  PasteSpecialDialog,
  StyleDialog,
  SymbolDialog,
  WordCountDialog,
} from './dialogs/InsertDialogs.tsx';
import { FindReplaceDialog, InsertTableDialog, PageSetupDialog } from './dialogs/PageDialogs.tsx';
import { ParagraphDialog, TabsDialog } from './dialogs/ParagraphDialog.tsx';
import { useWrite } from './hooks.ts';
import type { WriteEditorModel } from './model.ts';

export function WriteDialogs({ model }: { model: WriteEditorModel }) {
  const dialog = useWrite(model, (s) => s.dialog);
  if (!dialog) return null;
  const { id, tab, arg } = dialog;
  switch (id) {
    case 'font':
      return <FontDialog model={model} tab={tab} />;
    case 'paragraph':
      return <ParagraphDialog model={model} tab={tab} />;
    case 'tabs':
      return <TabsDialog model={model} />;
    case 'pageSetup':
      return <PageSetupDialog model={model} tab={tab} />;
    case 'insertTable':
      return <InsertTableDialog model={model} />;
    case 'find':
      return <FindReplaceDialog model={model} tab={tab} />;
    case 'link':
      return <LinkDialog model={model} />;
    case 'symbol':
      return <SymbolDialog model={model} />;
    case 'dateTime':
      return <DateTimeDialog model={model} tab={tab} />;
    case 'wordCount':
      return <WordCountDialog model={model} />;
    case 'pasteSpecial':
      return <PasteSpecialDialog model={model} />;
    case 'bookmark':
      return <BookmarkDialog model={model} />;
    case 'newStyle':
      return <StyleDialog model={model} />;
    case 'modifyStyle':
      return <StyleDialog model={model} styleId={arg} />;
  }
}
