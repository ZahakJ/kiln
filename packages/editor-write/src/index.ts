/**
 * @kiln/editor-write — Kiln Write's editor views (PLAN.md §1.3, §1.4): the document surface with
 * rulers, the Navigation and Styles panes, the Font, Paragraph, Tabs, Page Setup, Find and Replace,
 * Insert Table, Link, Symbol, Date & Time, Word Count, Paste Special, Bookmark and Style dialogs,
 * Insert ▸ Table's grid picker, the status bar, the mini toolbar and the context menu.
 *
 * Presentation only (§3.7): every edit is a core command run through the window's registry, and
 * every figure shown (formatting, styles, headings, pages, rulers, Find results, counts) is an
 * answer of the core's `Query::Write`.
 */
import type { CommandRegistry } from '@kiln/commands';
import type { DocId, KilnClient } from '@kiln/protocol';
import { useEffect, useState } from 'react';
import { registerWriteUi } from './commands.ts';
import { registerWriteFeatures } from './features/commands.ts';
import { WriteEditorModel } from './model.ts';

export const PACKAGE_NAME = '@kiln/editor-write' as const;

export { DIALOG_COMMANDS, fitZoom, registerWriteUi } from './commands.ts';
export * from './features/index.ts';
export { useWrite } from './hooks.ts';
export {
  contextsOf,
  EMPTY_FIND,
  type NavTab,
  type SurfaceView,
  type WriteDialog,
  WriteEditorModel,
  type WriteModelDeps,
  type WriteUiState,
  writeResult,
} from './model.ts';
export { CommentsPane } from './panes/CommentsPane.tsx';
export { FeaturePanes } from './panes/FeaturePanes.tsx';
export { NavigationPane } from './panes/NavigationPane.tsx';
export { ReviewingPane } from './panes/ReviewingPane.tsx';
export { SelectionPane } from './panes/SelectionPane.tsx';
export { SpellingPane } from './panes/SpellingPane.tsx';
export { StylesPane } from './panes/StylesPane.tsx';
export { HorizontalRuler, RULER_SIZE, VerticalRuler } from './Rulers.tsx';
export { TableGridPicker, withTablePicker } from './TableGridPicker.tsx';
export { WriteDialogs } from './WriteDialogs.tsx';
export { contextMenuEntries, miniToolbarItems, WriteEditor, type WriteEditorProps } from './WriteEditor.tsx';
export { WriteStatusBar, writeStatusItems } from './WriteStatusBar.tsx';

/**
 * The editor model of a Write document, with the editor's chrome commands registered on the
 * window's registry for as long as the component is mounted (`null` until a document is shown).
 */
export function useWriteEditorModel(
  deps: { client: KilnClient; doc: DocId; registry: CommandRegistry } | null,
): WriteEditorModel | null {
  const client = deps?.client ?? null;
  const doc = deps?.doc ?? null;
  const registry = deps?.registry ?? null;
  const [model, setModel] = useState<WriteEditorModel | null>(null);
  useEffect(() => {
    if (!client || doc === null || !registry) {
      setModel(null);
      return;
    }
    const m = new WriteEditorModel({ client, doc, registry });
    const ui = registerWriteUi(m);
    // WP-66: comments, track changes, references, objects, Format Painter, spelling.
    const features = registerWriteFeatures(m);
    setModel(m);
    return () => {
      features.dispose();
      ui.dispose();
      m.dispose();
      setModel(null);
    };
  }, [client, doc, registry]);
  return model;
}
