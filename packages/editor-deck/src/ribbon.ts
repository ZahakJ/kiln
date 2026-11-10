/**
 * The Deck ribbon with the presentation's layouts: New Slide ▾ and Layout ▾ list the layouts of
 * the current master (the core's `DeckState.layouts`), as PowerPoint's galleries do.
 */

import type {
  RibbonData,
  RibbonDataGroup,
  RibbonDataLeaf,
  RibbonDataRow,
  RibbonMenuEntry,
} from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { DeckLayoutInfo } from '@kiln/protocol';

const NEW_SLIDE_IDS = new Set(['deck.new_slide', 'deck.insert_new_slide']);

/**
 * `spec` with the layout menus filled in and, when given, the themes' rendered samples (by theme
 * name) in Design ▸ Themes and the current theme's variants (by variant) in Design ▸ Variants.
 */
export function deckRibbonWithLayouts(
  spec: RibbonData,
  layouts: readonly DeckLayoutInfo[],
  themeImages: Readonly<Record<string, string>> = {},
  variantImages: Readonly<Record<string, string>> = {},
): RibbonData {
  if (
    layouts.length === 0 &&
    Object.keys(themeImages).length === 0 &&
    Object.keys(variantImages).length === 0
  )
    return spec;
  const entries = (id: string): RibbonMenuEntry[] =>
    layouts.map((l) => ({ command: id, label: l.name, args: { layout: l.id }, icon: 'SlideLayout' }));
  const leaf = (item: RibbonDataLeaf): RibbonDataLeaf => {
    if (item.kind === 'gallery' && item.command === 'deck.design.themes')
      return {
        ...item,
        items: item.items.map((e) => {
          const image = themeImages[e.label];
          return image ? { ...e, look: { ...e.look, image } } : e;
        }),
      };
    // The current theme's variants, drawn.
    if (item.kind === 'gallery' && item.command === 'deck.design.variants')
      return {
        ...item,
        items: item.items.map((e) => {
          const image = variantImages[e.id];
          return image ? { ...e, look: { ...e.look, image } } : e;
        }),
      };
    if (layouts.length === 0) return item;
    if (item.kind === 'dropdown' && item.id === 'deck.layout')
      return { ...item, menu: entries('deck.slides.layout') };
    if (item.kind === 'split' && item.id && NEW_SLIDE_IDS.has(item.id))
      return {
        ...item,
        menu: [
          { kind: 'heading', label: msg('Layouts') },
          ...entries('deck.slides.new_slide'),
          { kind: 'separator' },
          { command: 'deck.slides.duplicate', label: msg('Duplicate Selected Slides') },
        ],
      };
    return item;
  };
  const group = (g: RibbonDataGroup): RibbonDataGroup => ({
    ...g,
    items: g.items.map((i) =>
      i.kind === 'row' ? ({ ...i, items: i.items.map(leaf) } as RibbonDataRow) : leaf(i as RibbonDataLeaf),
    ),
  });
  return { ...spec, tabs: spec.tabs.map((t) => ({ ...t, groups: t.groups.map(group) })) };
}
