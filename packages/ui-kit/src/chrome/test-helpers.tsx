/** Test helpers for chrome components (not exported from the package). */
import './test-matchers.ts';
import {
  type AppId,
  COMMAND_CATALOG,
  CommandRegistry,
  createDefaultKeymap,
  type Keymap,
  MemoryStorage,
  RecentCommands,
} from '@officina/commands';
import { render } from '@testing-library/react';
import type { ReactElement } from 'react';
import { TooltipProvider } from '../primitives/Tooltip.tsx';
import { CommandsProvider } from './CommandContext.tsx';

export interface Harness {
  registry: CommandRegistry;
  keymap: Keymap;
  recent: RecentCommands;
}

export function createHarness(app: AppId = 'write'): Harness {
  const recent = new RecentCommands({ storage: new MemoryStorage() });
  const registry = new CommandRegistry({ context: { app }, recent });
  registry.declare(COMMAND_CATALOG);
  const keymap = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
  return { registry, keymap, recent };
}

export function renderWithCommands(
  ui: ReactElement,
  harness: Harness = createHarness(),
  app: AppId = 'write',
) {
  const result = render(
    <CommandsProvider registry={harness.registry} keymap={harness.keymap} app={app} platform="other">
      <TooltipProvider delayDuration={0}>{ui}</TooltipProvider>
    </CommandsProvider>,
  );
  return { ...result, ...harness };
}
