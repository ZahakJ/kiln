/**
 * @officina/commands — the command system every app registers into (PLAN.md §1.3, §1.7, §2.1):
 * registry, keymaps (Office shortcuts), Key Tips, tier gating, recent commands, Office synonyms,
 * fuzzy search for the palette, and the AssistantProvider hooks. UI-free; ui-kit renders it.
 */
export const PACKAGE_NAME = '@officina/commands' as const;

export * from './assistant.ts';
export { CATALOG_SIZE, type CreateRegistryOptions, createCommandRegistry } from './bootstrap.ts';
export { COMMAND_CATALOG, c, catalogEntry, type DeclOptions, isDeclared, section } from './catalog/index.ts';
export * from './keymap.ts';
export * from './keymaps/index.ts';
export * from './keys.ts';
export * from './keytips.ts';
export * from './recent.ts';
export * from './registry.ts';
export * from './search.ts';
export * from './shortcutsDoc.ts';
export * from './storage.ts';
export * from './synonyms.ts';
export * from './tiers.ts';
export * from './types.ts';
