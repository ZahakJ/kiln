import { COMMAND_CATALOG } from './catalog/index.ts';
import { coreApps, coreCommand } from './core/catalog.ts';
import { RecentCommands } from './recent.ts';
import { CommandRegistry, type CommandRegistryOptions } from './registry.ts';
import { createOfficeSynonyms } from './synonyms.ts';
import { resolveTierConfig } from './tiers.ts';
import type { AppId, CommandDeclaration } from './types.ts';

export interface CreateRegistryOptions extends Omit<CommandRegistryOptions, 'context'> {
  app: AppId;
  /** Declare the whole catalog (default true). */
  declareCatalog?: boolean;
}

export const CATALOG_SIZE = COMMAND_CATALOG.length;

/**
 * A declaration with the core's metadata applied: for a command in `catalog.json`, the core decides
 * which apps offer it, its tier and (when the chrome names none) its Key Tip.
 */
export function withCoreMeta(decl: CommandDeclaration): CommandDeclaration {
  const info = coreCommand(decl.id);
  if (!info) return decl;
  return {
    ...decl,
    apps: coreApps(info),
    tier: info.tier,
    ...(decl.keyTip || !info.keytip ? {} : { keyTip: info.keytip }),
  };
}

/** The declared catalog with the core's metadata applied. */
export const REGISTRY_CATALOG: readonly CommandDeclaration[] = Object.freeze(
  COMMAND_CATALOG.map(withCoreMeta),
);

/**
 * A registry for one app window, pre-loaded with the declared catalog, Office synonyms, the build's
 * tier gate and a per-app recent-commands list.
 */
export function createCommandRegistry(options: CreateRegistryOptions): CommandRegistry {
  const { app, declareCatalog = true, ...rest } = options;
  const registry = new CommandRegistry({
    tiers: resolveTierConfig(),
    recent: new RecentCommands({ key: `kiln.recentCommands.${app}` }),
    ...rest,
    context: { app },
  });
  if (declareCatalog) registry.declare(REGISTRY_CATALOG);
  registry.addSynonyms(createOfficeSynonyms());
  return registry;
}
