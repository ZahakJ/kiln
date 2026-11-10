/**
 * The chrome side of the rest of Word's ribbon in Kiln Write: the dialogs (Zoom, Table
 * Properties, Sort, Language, Picture Corrections, Formula), Page Movement, Gridlines, the
 * Header & Footer distances, gradients from the shape's colour, the writing assistant's Rewrite,
 * Insert ▸ Screenshot, Insert Equation and Split. Presentation
 * only: every change of the document is a core command.
 */

import {
  type CommandContext,
  type CommandDefinition,
  type Disposable,
  EchoProvider,
  getAssistantProvider,
} from '@kiln/commands';
import { msg, t } from '@kiln/i18n';
import type { FeatureState, FormatState } from '@kiln/protocol';
import { loadOptions } from '@kiln/ui-kit';
import { featureState } from '../features/model.ts';
import type { WriteDialog, WriteEditorModel } from '../model.ts';

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;
const fmt = (ctx: CommandContext) => ctx.textFormat as FormatState | undefined;
const feat = (ctx: CommandContext) => fmt(ctx)?.features as FeatureState | undefined;

/** Points per centimetre or inch, for the Header & Footer distances. */
const PT_PER_CM = 72 / 2.54;

/** Mixes a colour with white (`t` > 0) or black (`t` < 0). */
export function mixHex(hex: string, t: number): string {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  const ch = (s: number) => {
    const v = (n >> s) & 0xff;
    const out = t >= 0 ? v + (255 - v) * t : v * (1 + t);
    return Math.round(out).toString(16).padStart(2, '0');
  };
  return `${ch(16)}${ch(8)}${ch(0)}`.toUpperCase();
}

/** A screenshot of a screen or window the user picks, as PNG bytes (`null`: cancelled). */
async function captureScreen(): Promise<Uint8Array | null> {
  const media = typeof navigator !== 'undefined' ? navigator.mediaDevices : undefined;
  if (!media?.getDisplayMedia) return null;
  const stream = await media.getDisplayMedia({ video: true, audio: false });
  try {
    const video = document.createElement('video');
    video.srcObject = stream;
    video.muted = true;
    await video.play();
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    canvas.getContext('2d')?.drawImage(video, 0, 0);
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, 'image/png'));
    return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
  } finally {
    for (const t of stream.getTracks()) t.stop();
  }
}

const DIALOGS: readonly { id: string; title: string; dialog: WriteDialog }[] = [
  { id: 'ui.write.zoom_dialog', title: msg('Zoom…'), dialog: 'zoom' },
  { id: 'ui.write.table_properties_dialog', title: msg('Table Properties…'), dialog: 'tableProperties' },
  { id: 'ui.write.sort_dialog', title: msg('Sort…'), dialog: 'sort' },
  { id: 'write.paragraph.sort', title: msg('Sort'), dialog: 'sort' },
  { id: 'ui.write.language_dialog', title: msg('Language…'), dialog: 'language' },
  { id: 'ui.write.corrections_dialog', title: msg('Picture Corrections Options…'), dialog: 'corrections' },
  { id: 'suite.insert.equation', title: msg('Equation'), dialog: 'equation' },
];

/** The Write editor's Office chrome commands; dispose to remove them. */
export function registerWriteOffice(model: WriteEditorModel): Disposable {
  const reg = model.registry;
  const run = (id: string, args?: unknown) => model.run(id, args);
  const distance = (side: 'header' | 'footer'): CommandDefinition<unknown> => ({
    id: `ui.write.${side}_distance`,
    title: side === 'header' ? msg('Header from Top') : msg('Footer from Bottom'),
    value: (ctx: CommandContext) => {
      const s = fmt(ctx)?.section;
      return s ? Math.round((s[side] / PT_PER_CM) * 100) / 100 : undefined;
    },
    run: (_ctx: CommandContext, args: unknown) => {
      const v = isRecord(args) && typeof args.value === 'number' ? args.value : null;
      if (v === null || !(v >= 0)) return;
      const pt = Math.round(v * PT_PER_CM * 100) / 100;
      return run('write.layout.page_setup', { setup: { [side]: pt } });
    },
  });
  const defs: CommandDefinition<unknown>[] = [
    ...DIALOGS.map(
      (d): CommandDefinition<unknown> => ({
        id: d.id,
        title: d.title,
        run: () => model.openDialog(d.dialog),
      }),
    ),
    // ── View ──────────────────────────────────────────────────────────────────────────────────
    {
      id: 'suite.view.vertical_pages',
      title: msg('Vertical'),
      isChecked: (ctx: CommandContext) => !(feat(ctx)?.side_to_side ?? false),
      run: () => run('write.view.page_movement', { side_to_side: false }),
    },
    {
      id: 'suite.view.side_to_side',
      title: msg('Side to Side'),
      isChecked: (ctx: CommandContext) => feat(ctx)?.side_to_side ?? false,
      run: () => run('write.view.page_movement', { side_to_side: true }),
    },
    {
      id: 'suite.view.gridlines',
      title: msg('Gridlines'),
      isChecked: () => model.state.gridlines,
      run: () => model.set({ gridlines: !model.state.gridlines }),
    },
    {
      id: 'suite.view.split',
      title: msg('Split'),
      isChecked: () => model.state.split,
      run: () => model.set({ split: !model.state.split }),
    },
    { id: 'ui.write.split_remove', title: msg('Remove Split'), run: () => model.set({ split: false }) },
    // ── Header & Footer ▸ Position ────────────────────────────────────────────────────────────
    distance('header'),
    distance('footer'),
    // ── Shape Format ──────────────────────────────────────────────────────────────────────────
    {
      id: 'ui.write.gradient',
      title: msg('Gradient'),
      run: (_ctx: CommandContext, args: unknown) => {
        const fill = featureState(model)?.object?.shape?.fill ?? '4472C4';
        const variant = isRecord(args) && args.variant === 'dark' ? 'dark' : 'light';
        const angle = isRecord(args) && typeof args.angle === 'number' ? args.angle : 90;
        const [from, to] = variant === 'light' ? [mixHex(fill, 0.6), fill] : [fill, mixHex(fill, -0.45)];
        return run('write.shape.gradient', { gradient: { from, to, angle } });
      },
    },
    // ── Layout ▸ Arrange ──────────────────────────────────────────────────────────────────────
    {
      id: 'ui.write.ungroup',
      title: msg('Ungroup'),
      run: () => run('write.arrange.group', { ungroup: true }),
    },
    // ── Home ▸ Kiln ▸ Rewrite (the writing assistant, off by default) ─────────────────────────
    {
      id: 'suite.help.assistant',
      title: msg('Rewrite'),
      run: async () => {
        const provider =
          loadOptions().privacy.assistant === 'echo' ? new EchoProvider() : getAssistantProvider();
        if (!provider.available) {
          model.set({
            notice: t(
              'No writing assistant is configured, so nothing leaves this device. Choose one in File ▸ Options ▸ Privacy.',
            ),
          });
          return;
        }
        const r = await model.client.query(model.doc, 'SelectedText');
        const text = isRecord(r) && 'Text' in r && typeof r.Text === 'string' ? r.Text : '';
        if (!text.trim()) {
          model.set({ notice: t('Select the text to rewrite first.') });
          return;
        }
        const out = await provider.rewrite({ text, tone: 'concise' });
        // The rewrite replaces the selection (a tracked change while Track Changes is on).
        return run('write.edit.insert_text', { text: out.text });
      },
    },
    // ── Insert ▸ Screenshot ───────────────────────────────────────────────────────────────────
    {
      id: 'suite.insert.screenshot',
      title: msg('Screenshot'),
      run: async () => {
        let bytes: Uint8Array | null = null;
        try {
          bytes = await captureScreen();
        } catch {
          bytes = null;
        }
        if (!bytes) {
          model.set({ notice: t('No screenshot was taken: choose a screen or window to capture.') });
          return;
        }
        return run('write.insert.picture', {
          name: 'Screenshot.png',
          bytes: Array.from(bytes),
          floating: false,
        });
      },
    },
  ];
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  return {
    dispose() {
      for (const d of disposables) d.dispose();
    },
  };
}

/** The ids `registerWriteOffice` implements (the chrome-implemented ribbon ids test reads it). */
export const WRITE_OFFICE_CHROME_IDS: readonly string[] = [
  ...DIALOGS.map((d) => d.id),
  'suite.view.vertical_pages',
  'suite.view.side_to_side',
  'suite.view.gridlines',
  'suite.view.split',
  'ui.write.split_remove',
  'ui.write.header_distance',
  'ui.write.footer_distance',
  'ui.write.gradient',
  'ui.write.ungroup',
  'suite.help.assistant',
  'suite.insert.screenshot',
];
