/**
 * The Write editor's chrome commands: the ribbon ids that open a dialog or a pane, toggle the
 * ruler, or fit the zoom (PLAN.md §1.4). They are presentation — every change to the document is
 * a core command the dialogs and panes run — and they register on the window's command registry
 * while the editor is mounted.
 */
import type { CommandDefinition, Disposable } from '@kiln/commands';
import type { WriteDialog, WriteEditorModel } from './model.ts';

interface DialogSpec {
  id: string;
  title: string;
  dialog: WriteDialog;
  tab?: string;
}

/** Ribbon ids that open one of the editor's dialogs. */
export const DIALOG_COMMANDS: readonly DialogSpec[] = [
  { id: 'suite.format.font_dialog', title: 'Font…', dialog: 'font' },
  { id: 'suite.format.paragraph_dialog', title: 'Paragraph…', dialog: 'paragraph' },
  { id: 'suite.layout.page_setup', title: 'Page Setup…', dialog: 'pageSetup' },
  { id: 'suite.edit.replace', title: 'Replace', dialog: 'find', tab: 'replace' },
  { id: 'suite.edit.go_to', title: 'Go To', dialog: 'find', tab: 'goto' },
  { id: 'suite.edit.paste_special', title: 'Paste Special…', dialog: 'pasteSpecial' },
  { id: 'suite.insert.table', title: 'Insert Table…', dialog: 'insertTable' },
  { id: 'suite.insert.link', title: 'Link…', dialog: 'link' },
  { id: 'suite.insert.symbol', title: 'Symbol…', dialog: 'symbol' },
  { id: 'suite.insert.date_time', title: 'Date & Time…', dialog: 'dateTime' },
  { id: 'suite.review.word_count', title: 'Word Count', dialog: 'wordCount' },
  { id: 'write.insert.quick_parts', title: 'Quick Parts', dialog: 'dateTime', tab: 'field' },
  { id: 'ui.write.bookmark', title: 'Bookmark…', dialog: 'bookmark' },
];

/** Zoom that fits a page rectangle into the surface (presentation arithmetic only). */
export function fitZoom(
  kind: 'page_width' | 'one_page' | 'multiple_pages',
  page: { width: number; height: number },
  view: { width: number; height: number },
): number {
  const gutter = 48;
  const w = Math.max(1, view.width - gutter);
  const h = Math.max(1, view.height - gutter);
  let z: number;
  if (kind === 'page_width') z = w / page.width;
  else if (kind === 'one_page') z = Math.min(w / page.width, h / page.height);
  else z = Math.min(w / page.width, h / (2 * page.height + 32));
  return Math.round(Math.min(5, Math.max(0.1, z)) * 100) / 100;
}

/** Registers the editor's chrome commands; dispose to remove them. */
export function registerWriteUi(model: WriteEditorModel): Disposable {
  const reg = model.registry;
  const defs: CommandDefinition<unknown>[] = DIALOG_COMMANDS.map((d) => ({
    id: d.id,
    title: d.title,
    run: (_ctx: unknown, args: unknown) => {
      const arg =
        typeof args === 'object' && args !== null && 'tab' in args && typeof args.tab === 'string'
          ? args.tab
          : d.tab;
      model.openDialog(d.dialog, arg);
    },
  }));
  defs.push(
    {
      id: 'suite.edit.find',
      title: 'Find',
      run: () => {
        model.set({ navOpen: true, navTab: 'results' });
        // The pane focuses its search box when it sees the request.
        model.focusSearch();
      },
    },
    {
      id: 'suite.view.ruler',
      title: 'Ruler',
      isChecked: () => model.state.showRuler,
      run: () => model.set({ showRuler: !model.state.showRuler }),
    },
    {
      id: 'suite.view.navigation_pane',
      title: 'Navigation Pane',
      isChecked: () => model.state.navOpen,
      run: () => model.set({ navOpen: !model.state.navOpen }),
    },
    {
      id: 'write.styles.pane',
      title: 'Styles Pane',
      isChecked: () => model.state.stylesOpen,
      run: () => model.set({ stylesOpen: !model.state.stylesOpen }),
    },
    {
      // Esc in the document (§1.7: cancel, close the pane): leaves a header or footer, else
      // hides the mini toolbar and closes the task panes, and puts the focus in the document.
      id: 'ui.edit.cancel',
      title: 'Cancel',
      run: async () => {
        const f = model.state.format;
        if (f?.story === 'Header' || f?.story === 'Footer') {
          await model.run('write.header_footer.close');
        } else if (model.state.mini) {
          model.set({ mini: null });
        } else if (model.state.navOpen || model.state.stylesOpen) {
          model.set({ navOpen: false, stylesOpen: false });
        }
        model.controller?.focus();
      },
    },
    // Menu heads: they open their menu; the items run the core's commands.
    { id: 'suite.edit.select', title: 'Select', run: () => undefined },
    { id: 'suite.layout.breaks', title: 'Breaks', run: () => undefined },
  );
  for (const kind of ['page_width', 'one_page', 'multiple_pages'] as const) {
    defs.push({
      id: `suite.view.${kind}`,
      title: kind === 'page_width' ? 'Page Width' : kind === 'one_page' ? 'One Page' : 'Multiple Pages',
      run: async () => {
        const r = await model.query('Pages');
        const view = model.state.view;
        const page = r && 'Pages' in r ? (r.Pages.rects[r.Pages.current] ?? r.Pages.rects[0]) : undefined;
        if (!page || !view) return;
        await model.run('suite.view.zoom', { zoom: fitZoom(kind, page, view) });
      },
    });
  }
  // A declaration that was already implemented elsewhere (another editor in the window) stays.
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  model.registry.setContext({ editor: 'write' });
  return {
    dispose() {
      for (const d of disposables) d.dispose();
      model.registry.setContext({ editor: undefined });
    },
  };
}
