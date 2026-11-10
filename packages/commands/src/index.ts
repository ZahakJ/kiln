/**
 * @kiln/commands — the command system every app registers into:
 * the registry over the core's generated `catalog.json` (dispatch = `KilnClient.execute` with the
 * typed `Command`, availability from `Event::Changed`), keymaps (Office shortcuts), ribbon
 * definitions as data, Key Tips, tier gating, recent commands, Office synonyms, fuzzy search for
 * the palette, and the AssistantProvider hooks. UI-free; ui-kit renders it.
 */
export const PACKAGE_NAME = '@kiln/commands' as const;

export * from './assistant.ts';
export {
  CATALOG_SIZE,
  type CreateRegistryOptions,
  createCommandRegistry,
  REGISTRY_CATALOG,
  withCoreMeta,
} from './bootstrap.ts';
export {
  COLLAB_CHROME_COMMANDS,
  COMMAND_CATALOG,
  c,
  catalogEntry,
  DECK_CHROME_COMMANDS,
  DECK_SLIDE_MASTER_TAB,
  type DeclOptions,
  deckRibbon,
  FRAME_CHROME_COMMANDS,
  GRID_CHROME_COMMANDS,
  GRID_THEMES,
  gridRibbon,
  isDeclared,
  notesRibbon,
  RIBBONS,
  section,
  WRITE_CHROME_COMMANDS,
  writeRibbon,
} from './catalog/index.ts';
export * from './core/build.ts';
export * from './core/catalog.ts';
export * from './core/connect.ts';
export * from './keymap.ts';
export { manualShortcutsText, shortcutsDocText } from './keymaps/doc.ts';
export * from './keymaps/index.ts';
export * from './keymaps/table.ts';
export * from './keys.ts';
export * from './keytips.ts';
export { commandMessages } from './messages.ts';
export * from './recent.ts';
export * from './registry.ts';
export * from './ribbon.ts';
export * from './search.ts';
export * from './shortcutsDoc.ts';
export * from './storage.ts';
export * from './synonyms.ts';
export * from './tiers.ts';
export * from './types.ts';
