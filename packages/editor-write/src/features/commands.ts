/**
 * The chrome side of Kiln Write's advanced features: the shared Review, Arrange, Insert
 * and Clipboard ids of the ribbon (`suite.*`) mapped to Write's core commands, the panes and
 * dialogs the ribbon opens (`ui.write.*`, `write.review.reviewing_pane`), Format Painter's
 * double-click, and Paste Markdown Table from the clipboard. Presentation only: every change is a
 * core command.
 */

import type { CommandContext, CommandDefinition, Disposable } from '@kiln/commands';
import { msg, t } from '@kiln/i18n';
import type { WriteEditorModel } from '../model.ts';
import { addToDictionary } from './dictionary.ts';
import {
  type FeatureDialog,
  type FeaturePane,
  featureState,
  featuresOf,
  type WriteFeatures,
} from './model.ts';

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;

/** The browser's file picker for one picture (Change Picture). */
function pickPicture(): Promise<File | null> {
  return new Promise((resolve) => {
    if (typeof document === 'undefined') {
      resolve(null);
      return;
    }
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/png,image/jpeg,image/gif,image/bmp,image/webp';
    input.addEventListener('change', () => resolve(input.files?.[0] ?? null), { once: true });
    input.addEventListener('cancel', () => resolve(null), { once: true });
    input.click();
  });
}

/** Two clicks on Format Painter within this many ms keep it on (Word's double-click). */
const DOUBLE_CLICK_MS = 450;

/** The chrome ids that open a feature dialog. */
export const FEATURE_DIALOGS: readonly { id: string; title: string; dialog: FeatureDialog; tab?: string }[] =
  [
    { id: 'ui.write.toc_dialog', title: msg('Custom Table of Contents…'), dialog: 'toc' },
    { id: 'ui.write.caption_dialog', title: msg('Insert Caption…'), dialog: 'caption' },
    { id: 'ui.write.cross_reference_dialog', title: msg('Cross-reference…'), dialog: 'crossRef' },
    { id: 'ui.write.footnote_dialog', title: msg('Footnote and Endnote…'), dialog: 'footnote' },
    { id: 'ui.write.layout_dialog', title: msg('More Layout Options…'), dialog: 'layout' },
    { id: 'ui.write.properties_dialog', title: msg('Properties…'), dialog: 'properties' },
    { id: 'ui.write.word_goal_dialog', title: msg('Set Word Goal…'), dialog: 'wordGoal' },
    { id: 'ui.write.numbering_value_dialog', title: msg('Set Numbering Value…'), dialog: 'numberingValue' },
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
  // Any other command in between (the brush painting, a selection made through a command)
  // ends the double-click window: two separate clicks around a paint are two single clicks.
  const painterIds = new Set([
    'suite.edit.format_painter',
    'write.format.painter',
    'write.format.cancel_painter',
  ]);
  const unsubscribe = reg.subscribe((e) => {
    if (e.type === 'executed' && !painterIds.has(e.id)) lastPainter = 0;
  });

  const defs: CommandDefinition<unknown>[] = [
    // ── Comments ───────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.review.new_comment',
      title: msg('New Comment'),
      run: () => {
        features.set({ composer: { parent: null } });
      },
    },
    {
      id: 'suite.review.delete_comment',
      title: msg('Delete Comment'),
      run: () => run('write.comment.delete', { id: featureState(model)?.comment ?? null }),
    },
    {
      id: 'suite.review.previous_comment',
      title: msg('Previous Comment'),
      run: () => run('write.find.go_to', { target: { Comment: { backward: true } } }),
    },
    {
      id: 'suite.review.next_comment',
      title: msg('Next Comment'),
      run: () => run('write.find.go_to', { target: { Comment: { backward: false } } }),
    },
    {
      id: 'suite.review.show_comments',
      title: msg('Show Comments'),
      isChecked: (ctx: CommandContext) =>
        (ctx.textFormat as { features?: { show_comments?: boolean } } | undefined)?.features?.show_comments ??
        true,
      run: () => run('write.review.show_markup', { kind: 'Comments' }),
    },
    {
      id: 'ui.write.comments_pane',
      title: msg('Comments Pane'),
      isChecked: paneChecked('comments'),
      run: pane('comments'),
    },
    // ── Tracking ───────────────────────────────────────────────────────────────────────────
    {
      id: 'write.review.reviewing_pane',
      title: msg('Reviewing Pane'),
      isChecked: paneChecked('review'),
      run: pane('review'),
    },
    // ── Proofing ───────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.review.spelling',
      title: msg('Spelling & Grammar'),
      run: async () => {
        features.set({ pane: 'spelling', spellingDone: false });
        bump(model, features);
        await features.nextMisspelling();
      },
    },
    {
      id: 'ui.write.spell_add',
      title: msg('Add to Dictionary'),
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
      title: msg('Format Painter'),
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
      title: msg('Copy Formatting'),
      run: () => run('write.format.copy_format'),
    },
    {
      id: 'suite.format.paste_formatting',
      title: msg('Paste Formatting'),
      run: () => run('write.format.paste_format'),
    },
    // ── Arrange ────────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.arrange.position',
      title: msg('Position'),
      run: () => features.openDialog('layout', 'position'),
    },
    {
      id: 'suite.arrange.wrap_text',
      title: msg('Wrap Text'),
      run: () => features.openDialog('layout', 'wrap'),
    },
    {
      id: 'suite.arrange.bring_forward',
      title: msg('Bring Forward'),
      run: () => run('write.arrange.order', { order: 'Forward' }),
    },
    {
      id: 'suite.arrange.send_backward',
      title: msg('Send Backward'),
      run: () => run('write.arrange.order', { order: 'Backward' }),
    },
    {
      id: 'suite.arrange.selection_pane',
      title: msg('Selection Pane'),
      isChecked: paneChecked('selection'),
      run: pane('selection'),
    },
    { id: 'suite.arrange.align', title: msg('Align'), run: () => features.openDialog('layout', 'position') },
    {
      id: 'suite.arrange.rotate',
      title: msg('Rotate'),
      run: () => run('write.arrange.rotate', { op: 'Right90' }),
    },
    {
      id: 'ui.write.align',
      title: msg('Align Object'),
      run: (_ctx: CommandContext, args: unknown) => {
        const align = isRecord(args) && typeof args.align === 'string' ? args.align : 'Left';
        // Align Selected Objects (the default) lines several objects up with each other and a
        // single one with the margins; Align to Page or Margin aligns each with that.
        const to = features.state.alignTo;
        const relative_to = to === 'Page' ? 'Page' : to === 'Margin' ? 'Margin' : null;
        return run('write.arrange.align', { align, relative_to });
      },
    },
    {
      id: 'ui.write.align_to',
      title: msg('Align to Page or Margin'),
      run: (_ctx: CommandContext, args: unknown) => {
        const to = isRecord(args) ? args.to : null;
        features.set({ alignTo: to === 'Page' ? 'Page' : to === 'Margin' ? 'Margin' : 'Selected' });
        bump(model, features);
      },
    },
    // ── Insert ─────────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.insert.text_box',
      title: msg('Text Box'),
      run: () => run('write.insert.text_box', { text: null, design: 'Simple' }),
    },
    {
      id: 'suite.insert.shapes',
      title: msg('Shapes'),
      run: () => features.set({ draw: { shape: 'rect' } }),
    },
    {
      id: 'ui.write.draw_shape',
      title: msg('Draw Shape'),
      run: (_ctx: CommandContext, args: unknown) => {
        const preset = isRecord(args) && typeof args.preset === 'string' ? args.preset : 'rect';
        features.set({ draw: { shape: preset } });
      },
    },
    {
      id: 'ui.write.draw_text_box',
      title: msg('Draw Text Box'),
      run: () => features.set({ draw: { textBox: 'Simple' } }),
    },
    {
      id: 'suite.insert.icons',
      title: msg('Icons'),
      run: () => features.openDialog('icons'),
    },
    {
      id: 'ui.write.icons_dialog',
      title: msg('Icons…'),
      run: () => features.openDialog('icons'),
    },
    // ── Picture Format ─────────────────────────────────────────────────────────────────────
    {
      id: 'ui.write.change_picture',
      title: msg('Change Picture…'),
      run: async () => {
        const file = await pickPicture();
        if (!file) return;
        await run('write.picture.change', {
          name: file.name,
          bytes: new Uint8Array(await file.arrayBuffer()),
        });
      },
    },
    {
      id: 'ui.write.crop_dialog',
      title: msg('Crop…'),
      run: () => features.openDialog('layout', 'crop'),
    },
    {
      id: 'suite.picture.crop',
      title: msg('Crop'),
      run: () => features.openDialog('layout', 'crop'),
    },
    {
      id: 'suite.picture.reset',
      title: msg('Reset Picture'),
      run: () => run('write.picture.reset', { size: false }),
    },
    {
      id: 'ui.write.text_box_options',
      title: msg('Text Box Options…'),
      run: () => features.openDialog('layout', 'text'),
    },
    // Table Layout ▸ Cell Size: the selected cells' row height and column width (points).
    ...(['height', 'width'] as const).map(
      (side): CommandDefinition<unknown> => ({
        id: side === 'height' ? 'ui.write.table_row_height' : 'ui.write.table_column_width',
        title: side === 'height' ? msg('Table Row Height') : msg('Table Column Width'),
        run: (_ctx: CommandContext, args: unknown) => {
          const v = isRecord(args) && typeof args.value === 'number' ? args.value : null;
          if (v === null || !(v > 0)) return;
          return run('write.table.cell_size', side === 'height' ? { height: v } : { width: v });
        },
      }),
    ),
    // Picture Format ▸ Size: Height and Width (points), keeping a picture's proportions.
    ...(['height', 'width'] as const).map(
      (side): CommandDefinition<unknown> => ({
        id: `ui.write.object_${side}`,
        title: side === 'height' ? msg('Shape Height') : msg('Shape Width'),
        value: (ctx: CommandContext) => {
          const o = (
            ctx.textFormat as { features?: { object?: { width: number; height: number } | null } } | undefined
          )?.features?.object;
          return o ? Math.round(o[side] * 100) / 100 : undefined;
        },
        run: (_ctx: CommandContext, args: unknown) => {
          const o = featureState(model)?.object;
          const v = isRecord(args) && typeof args.value === 'number' ? args.value : null;
          if (!o || v === null || !(v > 0)) return;
          const ratio = o.height > 0 ? o.width / o.height : 1;
          const keep = o.kind === 'Picture';
          const width = side === 'width' ? v : keep ? v * ratio : o.width;
          const height = side === 'height' ? v : keep ? v / ratio : o.height;
          return run('write.arrange.layout', {
            id: o.id,
            spec: {
              wrap: null,
              x: null,
              x_from: null,
              y: null,
              y_from: null,
              width,
              height,
              rotation: null,
              distance: null,
              move_with_text: null,
              allow_overlap: null,
              name: null,
              alt_text: null,
              decorative: null,
            },
          });
        },
      }),
    ),
    {
      id: 'suite.picture.alt_text',
      title: msg('Alt Text'),
      run: () => features.openDialog('layout', 'alt'),
    },
    // ── Review ▸ Show Markup ▸ Specific People ──────────────────────────────────────────────
    {
      id: 'ui.write.reviewers',
      title: msg('Choose Reviewers…'),
      run: () => {
        features.set({ pane: 'review' });
        bump(model, features);
      },
    },
    {
      id: 'suite.insert.code_block',
      title: msg('Code Block'),
      run: () => run('write.insert.code_block', { language: null }),
    },
    {
      id: 'write.insert.paste_markdown_table',
      title: msg('Paste Markdown Table'),
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
              notice: t('Kiln could not read the clipboard: paste with Ctrl+V, then Paste as Markdown.'),
            });
            return;
          }
        }
        if (!text.trim()) {
          model.set({ notice: t('The clipboard holds no text to paste as Markdown.') });
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
      unsubscribe();
      for (const d of disposables) d.dispose();
      features.dispose();
    },
  };
}
