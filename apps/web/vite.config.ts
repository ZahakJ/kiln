import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));
const packagesDir = fileURLToPath(new URL('../../packages', import.meta.url));

/** `@officina/<pkg>` -> `packages/<pkg>/src` (mirrors the `paths` in tsconfig.base.json). */
const officinaAliases = readdirSync(packagesDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => ({
    find: new RegExp(`^@officina/${entry.name}$`),
    replacement: `${packagesDir}/${entry.name}/src/index.ts`,
  }));

/** The suite version shown in the UI is the root package version. */
const suiteVersion: string = JSON.parse(readFileSync(`${repoRoot}/package.json`, 'utf8')).version;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: officinaAliases },
  define: { __OFFICINA_VERSION__: JSON.stringify(suiteVersion) },
  server: { port: 5173, strictPort: true },
  build: { target: 'es2023', sourcemap: true },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    maxWorkers: 2,
  },
});
