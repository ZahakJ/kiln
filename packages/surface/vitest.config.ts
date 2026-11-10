import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const packagesDir = fileURLToPath(new URL('..', import.meta.url));

/** `@kiln/<pkg>` -> `packages/<pkg>/src/index.ts`, as in the web app. */
const kilnAliases = readdirSync(packagesDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => ({
    find: new RegExp(`^@kiln/${entry.name}$`),
    replacement: `${packagesDir}${entry.name}/src/index.ts`,
  }));

export default defineConfig({
  resolve: { alias: kilnAliases },
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
    maxWorkers: 2,
  },
});
