/**
 * @kiln/commands — the command system every app registers into (PLAN.md §1.3, §1.7, §2.1, §3.7):
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
  COMMAND_CATALOG,
  c,
  catalogEntry,
  type DeclOptions,
  deckRibbon,
  gridRibbon,
  isDeclared,
  notesRibbon,
  RIBBONS,
  section,
  writeRibbon,
} from './catalog/index.ts';
export * from './core/build.ts';
export * from './core/catalog.ts';
export * from './core/connect.ts';
export * from './keymap.ts';
export { shortcutsDocText } from './keymaps/doc.ts';
export * from './keymaps/index.ts';
export * from './keymaps/table.ts';
export * from './keys.ts';
export * from './keytips.ts';
export * from './recent.ts';
export * from './registry.ts';
export * from './ribbon.ts';
export * from './search.ts';
export * from './shortcutsDoc.ts';
export * from './storage.ts';
export * from './synonyms.ts';
export * from './tiers.ts';
export * from './types.ts';
