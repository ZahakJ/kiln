import { COMMAND_CATALOG } from './catalog/index.ts';
import { RecentCommands } from './recent.ts';
import { CommandRegistry, type CommandRegistryOptions } from './registry.ts';
import { createOfficeSynonyms } from './synonyms.ts';
import { resolveTierConfig } from './tiers.ts';
import type { AppId } from './types.ts';

export interface CreateRegistryOptions extends Omit<CommandRegistryOptions, 'context'> {
  app: AppId;
  /** Declare the whole catalog (default true). */
  declareCatalog?: boolean;
}

export const CATALOG_SIZE = COMMAND_CATALOG.length;

/**
 * A registry for one app window, pre-loaded with the declared catalog, Office synonyms, the build's
 * tier gate and a per-app recent-commands list.
 */
export function createCommandRegistry(options: CreateRegistryOptions): CommandRegistry {
  const { app, declareCatalog = true, ...rest } = options;
  const registry = new CommandRegistry({
    tiers: resolveTierConfig(),
    recent: new RecentCommands({ key: `officina.recentCommands.${app}` }),
    ...rest,
    context: { app },
  });
  if (declareCatalog) registry.declare(COMMAND_CATALOG);
  registry.addSynonyms(createOfficeSynonyms());
  return registry;
}
