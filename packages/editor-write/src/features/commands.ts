/**
 * The chrome side of Kiln Write's advanced features (WP-66): the shared Review, Arrange, Insert
 * and Clipboard ids of the ribbon (`suite.*`) mapped to Write's core commands, the panes and
 * dialogs the ribbon opens (`ui.write.*`, `write.review.reviewing_pane`), Format Painter's
 * double-click, and Paste Markdown Table from the clipboard. Presentation only: every change is a
 * core command (PLAN.md §3.7).
 */
import type { CommandContext, CommandDefinition, Disposable } from '@kiln/commands';
import type { WriteEditorModel } from '../model.ts';
import { addToDictionary } from './dictionary.ts';
import {
  type FeatureDialog,
  type FeaturePane,
  featureState,
  featuresOf,
  type WriteFeatures,
} from './model.ts';

/** Two clicks on Format Painter within this many ms keep it on (Word's double-click). */
const DOUBLE_CLICK_MS = 450;

/** The chrome ids that open a feature dialog. */
export const FEATURE_DIALOGS: readonly { id: string; title: string; dialog: FeatureDialog; tab?: string }[] =
  [
    { id: 'ui.write.toc_dialog', title: 'Custom Table of Contents…', dialog: 'toc' },
    { id: 'ui.write.caption_dialog', title: 'Insert Caption…', dialog: 'caption' },
    { id: 'ui.write.cross_reference_dialog', title: 'Cross-reference…', dialog: 'crossRef' },
    { id: 'ui.write.footnote_dialog', title: 'Footnote and Endnote…', dialog: 'footnote' },
    { id: 'ui.write.layout_dialog', title: 'More Layout Options…', dialog: 'layout' },
    { id: 'ui.write.properties_dialog', title: 'Properties…', dialog: 'properties' },
    { id: 'ui.write.word_goal_dialog', title: 'Set Word Goal…', dialog: 'wordGoal' },
  ];

/** Tells the ribbon that a feature state it shows changed (pane toggles). */
function bump(model: WriteEditorModel, features: WriteFeatures): void {
  model.registry.setContext({ writeFeaturePane: features.state.pane });
}

export function registerWriteFeatures(model: WriteEditorModel): Disposable {
  const features = featuresOf(model);
  const reg = model.registry;
  const run = (id: string, args?: unknown) => model.run(id, args);
  const pane = (p: FeaturePane) => () => {
    features.togglePane(p);
    bump(model, features);
  };
  const paneChecked = (p: FeaturePane) => () => features.state.pane === p;
  let lastPainter = 0;

  const defs: CommandDefinition<unknown>[] = [
    // ── Comments ───────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.review.new_comment',
      title: 'New Comment',
      run: () => {
        features.set({ composer: { parent: null } });
      },
    },
    {
      id: 'suite.review.delete_comment',
      title: 'Delete Comment',
      run: () => run('write.comment.delete', { id: featureState(model)?.comment ?? null }),
    },
    {
      id: 'suite.review.previous_comment',
      title: 'Previous Comment',
      run: () => run('write.find.go_to', { target: { Comment: { backward: true } } }),
    },
    {
      id: 'suite.review.next_comment',
      title: 'Next Comment',
      run: () => run('write.find.go_to', { target: { Comment: { backward: false } } }),
    },
    {
      id: 'suite.review.show_comments',
      title: 'Show Comments',
      isChecked: (ctx: CommandContext) =>
        (ctx.textFormat as { features?: { show_comments?: boolean } } | undefined)?.features?.show_comments ??
        true,
      run: () => run('write.review.show_markup', { kind: 'Comments' }),
    },
    {
      id: 'ui.write.comments_pane',
      title: 'Comments Pane',
      isChecked: paneChecked('comments'),
      run: pane('comments'),
    },
    // ── Tracking ───────────────────────────────────────────────────────────────────────────
    {
      id: 'write.review.reviewing_pane',
      title: 'Reviewing Pane',
      isChecked: paneChecked('review'),
      run: pane('review'),
    },
    // ── Proofing ───────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.review.spelling',
      title: 'Spelling & Grammar',
      run: async () => {
        features.set({ pane: 'spelling', spellingDone: false });
        bump(model, features);
        await features.nextMisspelling();
      },
    },
    {
      id: 'ui.write.spell_add',
      title: 'Add to Dictionary',
      run: async (_ctx: CommandContext, args: unknown) => {
        const word =
          typeof args === 'object' && args !== null && 'word' in args && typeof args.word === 'string'
            ? args.word
            : null;
        if (word) await addToDictionary(model, word);
      },
    },
    // ── Clipboard: Format Painter ──────────────────────────────────────────────────────────
    {
      id: 'suite.edit.format_painter',
      title: 'Format Painter',
      isChecked: (ctx: CommandContext) =>
        (ctx.textFormat as { features?: { painter?: boolean | null } } | undefined)?.features?.painter !=
        null,
      run: async () => {
        const now = Date.now();
        // The state of the first click may not be back from the core yet: the timing decides
        // (and is noted before the core is asked, so a quick second click sees it).
        const second = now - lastPainter < DOUBLE_CLICK_MS;
        lastPainter = second ? 0 : now;
        if (second) {
          // The second click of a double-click: keep painting until Esc.
          await run('write.format.cancel_painter');
          await run('write.format.painter', { sticky: true });
        } else {
          await run('write.format.painter', { sticky: false });
        }
      },
    },
    {
      id: 'suite.format.copy_formatting',
      title: 'Copy Formatting',
      run: () => run('write.format.copy_format'),
    },
    {
      id: 'suite.format.paste_formatting',
      title: 'Paste Formatting',
      run: () => run('write.format.paste_format'),
    },
    // ── Arrange ────────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.arrange.position',
      title: 'Position',
      run: () => features.openDialog('layout', 'position'),
    },
    {
      id: 'suite.arrange.wrap_text',
      title: 'Wrap Text',
      run: () => features.openDialog('layout', 'wrap'),
    },
    {
      id: 'suite.arrange.bring_forward',
      title: 'Bring Forward',
      run: () => run('write.arrange.order', { order: 'Forward' }),
    },
    {
      id: 'suite.arrange.send_backward',
      title: 'Send Backward',
      run: () => run('write.arrange.order', { order: 'Backward' }),
    },
    {
      id: 'suite.arrange.selection_pane',
      title: 'Selection Pane',
      isChecked: paneChecked('selection'),
      run: pane('selection'),
    },
    { id: 'suite.arrange.align', title: 'Align', run: () => features.openDialog('layout', 'position') },
    {
      id: 'suite.arrange.rotate',
      title: 'Rotate',
      run: () => run('write.arrange.rotate', { op: 'Right90' }),
    },
    // ── Insert ─────────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.insert.text_box',
      title: 'Text Box',
      run: () => run('write.insert.text_box', { text: null }),
    },
    {
      id: 'suite.insert.shapes',
      title: 'Shapes',
      run: () => run('write.insert.shape', { preset: 'roundRect' }),
    },
    {
      id: 'suite.insert.code_block',
      title: 'Code Block',
      run: () => run('write.insert.code_block', { language: null }),
    },
    {
      id: 'write.insert.paste_markdown_table',
      title: 'Paste Markdown Table',
      run: async (_ctx: CommandContext, args: unknown) => {
        const given =
          typeof args === 'object' && args !== null && 'text' in args && typeof args.text === 'string'
            ? args.text
            : null;
        let text = given;
        if (text === null) {
          try {
            text = await navigator.clipboard.readText();
          } catch {
            model.set({
              notice: 'Kiln could not read the clipboard: paste with Ctrl+V, then Paste as Markdown.',
            });
            return;
          }
        }
        if (!text.trim()) {
          model.set({ notice: 'The clipboard holds no text to paste as Markdown.' });
          return;
        }
        await run('write.insert.paste_markdown', { text });
      },
    },
  ];
  for (const d of FEATURE_DIALOGS) {
    defs.push({
      id: d.id,
      title: d.title,
      run: () => features.openDialog(d.dialog, d.tab),
    });
  }
  // A declaration implemented elsewhere already (another editor in the window) stays.
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  return {
    dispose() {
      for (const d of disposables) d.dispose();
      features.dispose();
    },
  };
}
