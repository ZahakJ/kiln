/**
 * Design ▸ Themes ▸ Browse for Themes: an Office theme (`.thmx`), presentation or template
 * picked from the device, applied by the core (`deck.design.browse_themes`); and the Variants
 * gallery's pictures of the current theme's variants (`DeckQuery::VariantThumbnails`).
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import { pickFile } from '../files.ts';
import { type DeckEditorModel, messageOf } from '../model.ts';
import { paintScene } from '../thumbs.ts';

/** The ids the Deck chrome implements for the Design tab. */
export const DECK_DESIGN_CHROME: readonly string[] = ['ui.deck.browse_themes'];

/** The files Browse for Themes takes. */
export const THEME_ACCEPT = '.thmx,.pptx,.potx,.pptm,.ppsx';

/** Applies a theme file (picked when none is given). */
export async function browseThemes(model: DeckEditorModel, given?: File): Promise<boolean> {
  const file = given ?? (await pickFile(THEME_ACCEPT, 'deck-browse-themes-file'));
  if (!file) return false;
  try {
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    return await model.core('deck.design.browse_themes', { bytes });
  } catch (error) {
    model.set({ notice: messageOf(error) });
    return false;
  }
}

/** The current theme's variants as pictures (`'0'`… by variant), for the Variants gallery. */
export async function variantImages(
  model: DeckEditorModel,
  theme: 'light' | 'dark',
): Promise<Record<string, string>> {
  if (typeof document === 'undefined') return {};
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const r = await model.query({ VariantThumbnails: { width: 64, dpr } });
  const scene = r && 'Scene' in r ? r.Scene : null;
  if (!scene) return {};
  const out: Record<string, string> = {};
  for (const [index, canvas] of paintScene(scene, theme)) {
    const el = document.createElement('canvas');
    el.width = canvas.width;
    el.height = canvas.height;
    const ctx = el.getContext('2d');
    if (!ctx) continue;
    ctx.drawImage(canvas, 0, 0);
    try {
      out[String(index)] = el.toDataURL('image/png');
    } catch {
      // A canvas the browser will not export: the swatch stays.
    }
  }
  return out;
}

/** Registers the Design commands; dispose to remove them. */
export function registerDesignUi(model: DeckEditorModel): Disposable {
  const defs: CommandDefinition<unknown>[] = [
    { id: 'ui.deck.browse_themes', title: msg('Browse for Themes…'), run: () => browseThemes(model) },
  ];
  const reg = model.registry;
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
