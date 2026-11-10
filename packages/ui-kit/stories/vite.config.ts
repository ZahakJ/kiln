import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const packagesDir = fileURLToPath(new URL('../..', import.meta.url));

/** `@kiln/<pkg>` → `packages/<pkg>/src/index.ts`, mirroring tsconfig paths and apps/web. */
const kilnAliases = readdirSync(packagesDir, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .map((e) => ({
    find: new RegExp(`^@kiln/${e.name}$`),
    replacement: `${packagesDir}/${e.name}/src/index.ts`,
  }));

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [react()],
  resolve: { alias: kilnAliases, dedupe: ['react', 'react-dom'] },
  server: {
    port: 5180,
    strictPort: true,
    fs: { allow: [fileURLToPath(new URL('../../..', import.meta.url))] },
  },
});
