/**
 * The Deck editor's chrome commands: the shared ids that, in Deck, open one of the
 * editor's dialogs or address a Deck core command — Insert ▸ Shapes, Text Box, Table, Header &
 * Footer and Link, Find and Replace, Select All, the Format Painter, Fit to Window — and the
 * `ui.deck.*` buttons of the ribbon. Every change to the presentation is a core command; these
 * register on the window's command registry while the editor is mounted.
 */

import type { CommandContext, CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import { registerDesignUi } from './design/designCommands.ts';
import { registerFeatureUi } from './features/featureCommands.ts';
import { registerMotionUi } from './features/motionCommands.ts';
import { registerMasterUi } from './master/masterCommands.ts';
import type { DeckDialog, DeckEditorModel } from './model.ts';
import { registerReviewUi } from './review/reviewCommands.ts';
import { registerViewUi } from './views/viewCommands.ts';

const rec = (args: unknown): Record<string, unknown> =>
  typeof args === 'object' && args !== null && !Array.isArray(args) ? (args as Record<string, unknown>) : {};

/** Ids that open one of the editor's dialogs. */
export const DECK_DIALOG_COMMANDS: readonly { id: string; title: string; dialog: DeckDialog }[] = [
  { id: 'suite.insert.shapes', title: msg('Shapes'), dialog: 'shapes' },
  { id: 'suite.format.font_dialog', title: msg('Font…'), dialog: 'font' },
  { id: 'suite.format.paragraph_dialog', title: msg('Paragraph…'), dialog: 'paragraph' },
  { id: 'suite.insert.symbol', title: msg('Symbol…'), dialog: 'symbol' },
  { id: 'suite.insert.table', title: msg('Table'), dialog: 'insertTable' },
  { id: 'suite.insert.header_footer', title: msg('Header & Footer'), dialog: 'headerFooter' },
  { id: 'suite.insert.date_time', title: msg('Date & Time'), dialog: 'headerFooter' },
  { id: 'suite.insert.link', title: msg('Link'), dialog: 'link' },
  { id: 'ui.deck.hyperlink', title: msg('Link…'), dialog: 'link' },
  { id: 'suite.edit.find', title: msg('Find'), dialog: 'find' },
  { id: 'suite.edit.replace', title: msg('Replace'), dialog: 'replace' },
  { id: 'suite.edit.paste_special', title: msg('Paste Special…'), dialog: 'pasteSpecial' },
  { id: 'suite.arrange.selection_pane', title: msg('Selection Pane'), dialog: 'selectionPane' },
  { id: 'ui.deck.selection_pane', title: msg('Selection Pane…'), dialog: 'selectionPane' },
  { id: 'ui.deck.format_background', title: msg('Format Background…'), dialog: 'formatBackground' },
  { id: 'ui.deck.slide_size', title: msg('Custom Slide Size…'), dialog: 'slideSize' },
  { id: 'ui.deck.replace_fonts', title: msg('Replace Fonts…'), dialog: 'replaceFonts' },
  { id: 'ui.deck.alt_text', title: msg('Alt Text…'), dialog: 'altText' },
  { id: 'ui.deck.crop', title: msg('Crop Picture…'), dialog: 'crop' },
  { id: 'ui.deck.size_position', title: msg('Size and Position…'), dialog: 'sizePosition' },
  { id: 'ui.deck.change_shape', title: msg('Change Shape…'), dialog: 'changeShape' },
  { id: 'ui.deck.insert_chart', title: msg('Chart'), dialog: 'insertChart' },
  { id: 'ui.deck.edit_chart_data', title: msg('Edit Data'), dialog: 'chartData' },
  { id: 'suite.insert.icons', title: msg('Icons'), dialog: 'icons' },
  { id: 'suite.insert.code_block', title: msg('Code Block'), dialog: 'codeBlock' },
  { id: 'ui.deck.format_shape', title: msg('Format Shape…'), dialog: 'formatShape' },
  { id: 'ui.deck.split_cells', title: msg('Split Cells…'), dialog: 'splitCells' },
  { id: 'ui.deck.cell_margins', title: msg('Custom Margins…'), dialog: 'cellMargins' },
];

/** Registers the editor's chrome commands; dispose to remove them. */
export function registerDeckUi(model: DeckEditorModel): Disposable {
  const reg = model.registry;
  const st = () => model.state.deck;
  const hasSelection = () => (st()?.selection.length ?? 0) > 0;
  const defs: CommandDefinition<unknown>[] = DECK_DIALOG_COMMANDS.map((d) => ({
    id: d.id,
    title: d.title,
    run: () => model.openDialog(d.dialog),
  }));
  const box = (key: 'width' | 'height') => (ctx: CommandContext) => {
    const s = ctx.deckState as { selection_box?: { width: number; height: number } | null } | undefined;
    const v = s?.selection_box?.[key];
    return v === undefined ? undefined : Math.round(v * 100) / 100;
  };
  defs.push(
    {
      id: 'suite.insert.text_box',
      title: msg('Text Box'),
      run: () => model.core('deck.stage.arm_insert', { preset: 'textBox' }),
    },
    {
      id: 'suite.edit.select_all',
      title: msg('Select All'),
      run: () => model.core('deck.select.all'),
    },
    {
      id: 'suite.format.copy_formatting',
      title: msg('Copy Formatting'),
      run: () => model.core('deck.clipboard.copy_format'),
    },
    {
      id: 'suite.format.paste_formatting',
      title: msg('Paste Formatting'),
      run: () => model.core('deck.clipboard.paste_format'),
    },
    {
      // Format Painter: copies the formatting; the next click on the slide pastes it.
      id: 'suite.edit.format_painter',
      title: msg('Format Painter'),
      isChecked: () => model.state.painter,
      run: async () => {
        if (model.state.painter) {
          model.set({ painter: false });
          return;
        }
        if (await model.core('deck.clipboard.copy_format')) model.set({ painter: true });
        model.stage?.focus();
      },
    },
    {
      id: 'suite.format.text_shadow',
      title: msg('Text Shadow'),
      isChecked: () => st()?.format?.shadow === true,
      run: () => model.core('deck.font.text_shadow', { on: !(st()?.format?.shadow ?? false) }),
    },
    {
      // Character Spacing: Normal and Loose (3 pt), or the points given.
      id: 'suite.format.character_spacing',
      title: msg('Character Spacing'),
      run: (_c: CommandContext, args: unknown) => {
        const p = rec(args).points;
        const now = st()?.format?.spacing ?? 0;
        return model.core('deck.font.character_spacing', {
          points: typeof p === 'number' ? p : Math.abs(now) > 0.01 ? 0 : 3,
        });
      },
    },
    {
      id: 'deck.view.fit_to_window',
      title: msg('Fit to Window'),
      isChecked: () => model.state.fit,
      run: () => {
        model.set({ fit: true });
        model.stage?.fitToWindow();
      },
    },
    {
      // Esc in the window: closes a menu, else ends the text edit or clears the selection.
      id: 'ui.edit.cancel',
      title: msg('Cancel'),
      run: async () => {
        if (model.state.painter) model.set({ painter: false });
        else if (model.state.animPainter) model.set({ animPainter: false });
        else await model.core('deck.select.escape');
        model.stage?.focus();
      },
    },
    {
      id: 'ui.deck.rename_section',
      title: msg('Rename Section…'),
      run: (_c: CommandContext, args: unknown) => {
        const i = rec(args).index;
        model.openDialog('renameSection', typeof i === 'number' ? i : sectionOf(model));
      },
    },
    {
      id: 'ui.deck.remove_section',
      title: msg('Remove Section'),
      run: (_c: CommandContext, args: unknown) => {
        const i = rec(args).index;
        const index = typeof i === 'number' ? i : sectionOf(model);
        if (index >= 0) return model.run('deck.slides.remove_section', { index, with_slides: false });
        return undefined;
      },
    },
    {
      // Picture Format ▸ Crop: the crop handles on the slide, again to end cropping.
      id: 'suite.picture.crop',
      title: msg('Crop'),
      isChecked: () => st()?.cropping === true,
      run: () => model.core('deck.picture.crop_mode', { on: !(st()?.cropping ?? false) }),
    },
    {
      id: 'ui.deck.change_picture',
      title: msg('Change Picture…'),
      run: () => model.run('suite.insert.pictures'),
    },
    {
      id: 'ui.deck.shape_height',
      title: msg('Shape Height'),
      value: box('height'),
      isEnabled: hasSelection,
      run: (_c: CommandContext, args: unknown) => {
        const v = rec(args).value;
        if (typeof v === 'number') return model.run('deck.format.size_position', { height: v });
        return undefined;
      },
    },
    {
      id: 'ui.deck.shape_width',
      title: msg('Shape Width'),
      value: box('width'),
      isEnabled: hasSelection,
      run: (_c: CommandContext, args: unknown) => {
        const v = rec(args).value;
        if (typeof v === 'number') return model.run('deck.format.size_position', { width: v });
        return undefined;
      },
    },
  );
  // A declaration another editor in the window already implements stays.
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  const motion = registerMotionUi(model);
  const features = registerFeatureUi(model);
  const review = registerReviewUi(model);
  const design = registerDesignUi(model);
  const master = registerMasterUi(model);
  const views = registerViewUi(model);
  reg.setContext({ editor: 'deck' });
  // A ribbon command leaves the keyboard on the slide, as PowerPoint does (a dialog or menu it
  // opens keeps it; they give it back when they close).
  const unsubscribe = reg.subscribe((ev) => {
    if (ev.type !== 'executed' || ev.result.status !== 'ran') return;
    setTimeout(() => {
      const active = typeof document === 'undefined' ? null : document.activeElement;
      if (active?.closest?.('.kiln-ribbon') && !document.querySelector('[role="dialog"], [role="menu"]'))
        model.stage?.focus();
    }, 0);
  });
  return {
    dispose() {
      unsubscribe();
      motion.dispose();
      features.dispose();
      review.dispose();
      design.dispose();
      master.dispose();
      views.dispose();
      for (const d of disposables) d.dispose();
      reg.setContext({ editor: undefined });
    },
  };
}

/** The section of the current slide (-1 without sections). */
export function sectionOf(model: DeckEditorModel): number {
  const s = model.state.deck;
  if (!s) return -1;
  return s.sections.findIndex((x) => s.current >= x.start && s.current < x.start + x.count);
}
