import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';
import { defineConfig } from 'vitest/config';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));
const packagesDir = fileURLToPath(new URL('../../packages', import.meta.url));

/** `@kiln/<pkg>` -> `packages/<pkg>/src` (mirrors the `paths` in tsconfig.base.json). */
const kilnAliases = readdirSync(packagesDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => ({
    find: new RegExp(`^@kiln/${entry.name}$`),
    replacement: `${packagesDir}/${entry.name}/src/index.ts`,
  }));

/** The suite version shown in the UI is the root package version. */
const suiteVersion: string = JSON.parse(readFileSync(`${repoRoot}/package.json`, 'utf8')).version;

/**
 * The GitHub Pages site serves the web app under this path (`tools/pages-build.sh`). Only the
 * `pages` mode uses it, with the service worker; the dev server, the e2e suite and the Tauri
 * shells keep `/` and run without one.
 */
export const PAGES_BASE = '/kiln/app/';

/** The largest file the service worker may cache: the biggest core is about 12 MB. */
const MAX_CACHED_BYTES = 32 * 1024 * 1024;

export default defineConfig(({ mode }) => {
  const pages = mode === 'pages';
  return {
    base: pages ? PAGES_BASE : '/',
    plugins: [
      react(),
      tailwindcss(),
      // the installable web app. The worker (`src/sw.ts`) gets the build's file list;
      // the manifest is `public/manifest.webmanifest` and `src/pwa/` registers the worker.
      ...(pages
        ? [
            VitePWA({
              strategies: 'injectManifest',
              srcDir: 'src',
              filename: 'sw.ts',
              injectRegister: false,
              manifest: false,
              includeManifestIcons: false,
              injectManifest: {
                globPatterns: ['**/*'],
                globIgnores: ['**/*.map', 'sw.js', 'screenshots/**'],
                maximumFileSizeToCacheInBytes: MAX_CACHED_BYTES,
                // Every asset name carries its content hash; the rest get a revision.
                dontCacheBustURLsMatching: /^assets\//,
              },
            }),
          ]
        : []),
    ],
    resolve: { alias: kilnAliases },
    define: {
      __KILN_VERSION__: JSON.stringify(suiteVersion),
      /** Whether this build registers the service worker (the Pages build only). */
      __KILN_PWA__: JSON.stringify(pages),
    },
    server: { port: 5173, strictPort: true },
    // The Pages build writes Vite's manifest: tools/pages-check.mjs measures the scripts each
    // window loads from it (budget B12).
    build: { target: 'es2023', sourcemap: true, manifest: pages },
    // The core's Worker loads the wasm bindings with a dynamic import, which needs ES module workers.
    worker: { format: 'es' },
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.{ts,tsx}'],
      maxWorkers: 2,
    },
  };
});
