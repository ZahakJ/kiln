/**
 * Deck's context menus (PowerPoint's): a slide in the rail, a section header, a shape, text being
 * edited and the empty slide. The editor sets `menu` on the model after selecting what is under
 * the pointer; every item is a command of the window's registry.
 */
import { msg, t } from '@kiln/i18n';
import type { DeckState } from '@kiln/protocol';
import { CommandMenuItems, Menu, MenuContent, type MenuEntry, MenuTrigger } from '@kiln/ui-kit';
import { useDeck } from './hooks.ts';
import type { DeckEditorModel, DeckMenu } from './model.ts';

const CLIPBOARD: MenuEntry[] = [
  { command: 'suite.edit.cut' },
  { command: 'suite.edit.copy' },
  { kind: 'heading', label: msg('Paste Options') },
  { command: 'suite.edit.paste' },
  { command: 'deck.clipboard.paste_picture', label: msg('Picture') },
  { command: 'suite.edit.paste_text_only', label: msg('Keep Text Only') },
  { kind: 'separator' },
];

/** A table's cells: merge and split (with the caret in a table). */
const TABLE: MenuEntry[] = [
  { command: 'suite.table.merge_cells', label: msg('Merge Cells') },
  { command: 'ui.deck.split_cells', label: msg('Split Cells…') },
  { kind: 'separator' },
];

/** The layouts of the presentation as menu entries running `id` with `{ layout }`. */
export function layoutEntries(deck: DeckState | null, id: string): MenuEntry[] {
  return (deck?.layouts ?? []).map((l) => ({ command: id, label: l.name, args: { layout: l.id } }));
}

/** The entries of a context menu of the given kind. */

/** A section's menu: rename, remove, move. */
function sectionEntries(index: number): MenuEntry[] {
  const at = { index };
  return [
    { command: 'ui.deck.rename_section', label: t('Rename Section…'), args: at },
    { command: 'ui.deck.remove_section', label: t('Remove Section'), args: at },
    {
      command: 'deck.slides.remove_section',
      label: t('Remove Section & Slides'),
      args: { ...at, with_slides: true },
    },
    { kind: 'separator' },
    { command: 'deck.slides.move_section', label: t('Move Section Up'), args: { ...at, delta: -1 } },
    { command: 'deck.slides.move_section', label: t('Move Section Down'), args: { ...at, delta: 1 } },
  ];
}

export function deckMenuEntries(
  kind: DeckMenu['kind'],
  deck: DeckState | null,
  section?: number,
): MenuEntry[] {
  switch (kind) {
    case 'rail':
      return [
        ...CLIPBOARD,
        { command: 'deck.slides.new_slide', label: t('New Slide') },
        { command: 'deck.slides.duplicate', label: t('Duplicate Slide') },
        { command: 'deck.slides.delete', label: t('Delete Slide') },
        { kind: 'separator' },
        { command: 'deck.slides.section', label: t('Add Section') },
        {
          kind: 'submenu',
          label: t('Layout'),
          icon: 'SlideLayout',
          items: layoutEntries(deck, 'deck.slides.layout'),
        },
        { command: 'deck.slides.reset_slide', label: t('Reset Slide') },
        { kind: 'separator' },
        { command: 'ui.deck.format_background', label: t('Format Background…') },
        { command: 'deck.show.hide_slide', label: t('Hide Slide') },
        // The slide's section (from the keyboard, Shift+F10 on the thumbnails).
        ...(section !== undefined
          ? [
              { kind: 'separator' } as const,
              {
                kind: 'submenu',
                label: t('Section'),
                icon: 'Section',
                items: sectionEntries(section),
              } as const,
            ]
          : []),
      ];
    case 'section':
      return sectionEntries(section ?? 0);
    case 'text':
      return [
        ...CLIPBOARD,
        ...(deck?.contexts.includes('table') ? TABLE : []),
        { command: 'suite.format.font_dialog', label: t('Font…') },
        { command: 'suite.format.paragraph_dialog', label: t('Paragraph…') },
        { command: 'suite.format.bullets', label: t('Bullets') },
        { command: 'suite.format.numbering', label: t('Numbering') },
        { kind: 'separator' },
        { command: 'suite.insert.link', label: t('Link…') },
      ];
    case 'shape':
      return [
        ...CLIPBOARD,
        { command: 'deck.slides.edit_text', label: t('Edit Text') },
        { kind: 'separator' },
        { command: 'deck.drawing.group', label: t('Group') },
        { command: 'deck.drawing.ungroup', label: t('Ungroup') },
        {
          kind: 'submenu',
          label: t('Bring to Front'),
          icon: 'BringForward',
          items: [
            { command: 'deck.arrange.order', label: t('Bring to Front'), args: { order: 'Front' } },
            { command: 'deck.arrange.order', label: t('Bring Forward'), args: { order: 'Forward' } },
          ],
        },
        {
          kind: 'submenu',
          label: t('Send to Back'),
          icon: 'SendBackward',
          items: [
            { command: 'deck.arrange.order', label: t('Send to Back'), args: { order: 'Back' } },
            { command: 'deck.arrange.order', label: t('Send Backward'), args: { order: 'Backward' } },
          ],
        },
        { kind: 'separator' },
        { command: 'suite.insert.link', label: t('Link…') },
        { command: 'ui.deck.alt_text', label: t('Edit Alt Text…') },
        { command: 'ui.deck.size_position', label: t('Size and Position…') },
        ...(deck?.contexts.includes('picture')
          ? [{ command: 'suite.picture.crop', label: t('Crop') }]
          : [{ command: 'ui.deck.format_shape', label: t('Format Shape…') }]),
        { command: 'deck.edit.delete', label: t('Delete') },
      ];
    default:
      return [
        { command: 'suite.edit.paste' },
        { kind: 'separator' },
        {
          kind: 'submenu',
          label: t('Layout'),
          icon: 'SlideLayout',
          items: layoutEntries(deck, 'deck.slides.layout'),
        },
        { command: 'deck.slides.reset_slide', label: t('Reset Slide') },
        { command: 'ui.deck.format_background', label: t('Format Background…') },
        { kind: 'separator' },
        {
          kind: 'submenu',
          label: t('Grid and Guides'),
          icon: 'Gridlines',
          items: [
            { command: 'deck.view.gridlines', label: t('Gridlines') },
            { command: 'deck.view.guides', label: t('Guides') },
            { kind: 'separator' },
            { command: 'deck.view.add_guide', label: t('Add Vertical Guide'), args: { horizontal: false } },
            { command: 'deck.view.add_guide', label: t('Add Horizontal Guide'), args: { horizontal: true } },
          ],
        },
      ];
  }
}

export function DeckMenus({ model }: { model: DeckEditorModel }) {
  const menu = useDeck(model, (s) => s.menu);
  const deck = useDeck(model, (s) => s.deck);
  if (!menu) return null;
  return (
    <Menu
      open
      modal
      onOpenChange={(o) => {
        if (!o) {
          model.set({ menu: null });
          if (menu.kind !== 'rail' && menu.kind !== 'section') model.stage?.focus();
        }
      }}
    >
      <MenuTrigger asChild>
        <span
          aria-hidden="true"
          tabIndex={-1}
          style={{ position: 'fixed', left: menu.x, top: menu.y, width: 0, height: 0 }}
        />
      </MenuTrigger>
      <MenuContent
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          if (menu.kind === 'rail' || menu.kind === 'section')
            (document.querySelector('[data-testid="deck-rail"]') as HTMLElement | null)?.focus();
          else model.stage?.focus();
        }}
        aria-label={t('Slide')}
        side="bottom"
        align="start"
        sideOffset={2}
        data-testid="deck-context-menu"
      >
        <CommandMenuItems entries={deckMenuEntries(menu.kind, deck, menu.section)} />
      </MenuContent>
    </Menu>
  );
}
