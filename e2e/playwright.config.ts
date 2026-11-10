import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

const repoRoot = fileURLToPath(new URL('..', import.meta.url));
const PORT = 5173;
/**
 * The welcome tour shows once per browser profile. Specs start from a profile that has
 * seen it, so the docked card stays out of their way; home.spec.ts tests the tour from a fresh one.
 */
const TOUR_SEEN = fileURLToPath(new URL('./tour-seen.json', import.meta.url));
/**
 * The phone and tablet specs: only the Pixel 7 and iPad projects run them, with touch, and
 * those projects run nothing else, so the desktop suite does not run twice.
 */
const MOBILE_SPECS = /mobile(-[a-z]+)?\.spec\.ts$/;
/**
 * The installable web app runs against the GitHub Pages artefact (`tools/pages-build.sh`),
 * served as Pages serves it under /kiln/ by `tools/pages-serve.mjs`, or against a deployed site:
 * `KILN_E2E_BASE=https://zahakj.github.io/kiln/app/`.
 */
const PWA_SPECS = /pwa(-[a-z]+)?\.spec\.ts$/;
/** `tools/chaos-planted.sh` serves a planted build on a port of its own. */
const PAGES_PORT = Number(process.env.KILN_PAGES_PORT ?? 4180);
const PAGES_ROOT = process.env.KILN_PAGES_ROOT ?? 'target/pages/site';
const PWA_BASE = process.env.KILN_E2E_BASE ?? `http://127.0.0.1:${PAGES_PORT}/kiln/app/`;
/**
 * Grid's recovery spec on check cores (`tools/check-site.sh`: every journal write is replayed onto
 * a shadow copy of the workbook and compared, and a difference stops the core), served on a port
 * of its own. The site is built first when it is missing or older than the release one.
 */
const CHECK_PORT = 4182;
const CHECK_BASE = `http://127.0.0.1:${CHECK_PORT}/kiln/app/`;

export default defineConfig({
  testDir: './specs',
  outputDir: './screenshots/.results',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  // A CI runner (4 shared vCPUs) runs these several times slower than a development machine, so
  // a test and an expectation get three times the time there. Tests that set their own limit keep
  // it, and the timed budgets (`@budget`) measure, they do not wait.
  timeout: process.env.CI ? 90_000 : 30_000,
  expect: { timeout: process.env.CI ? 15_000 : 5_000 },
  workers: 2,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: `http://localhost:${PORT}`,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  // The tests that measure a time budget (tagged `@budget`: frame rates, keystroke latency, open
  // times) run last, one at a time, once every other test is done, so nothing else competes with
  // them for the machine; `pnpm e2e --workers=2` still runs everything.
  projects: [
    {
      name: 'chromium',
      grepInvert: /@budget/,
      testIgnore: [MOBILE_SPECS, PWA_SPECS],
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 }, storageState: TOUR_SEEN },
    },
    {
      // A fresh profile each test (the welcome tour is dismissed by the spec): service workers,
      // caches and the Origin Private File System start empty.
      name: 'pwa',
      testMatch: PWA_SPECS,
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 }, baseURL: PWA_BASE },
    },
    {
      name: 'pwa-check-site',
      testMatch: /check-site\.setup\.ts$/,
    },
    {
      // Check cores are slower (each write is replayed and the workbook compared): the stress
      // rounds stay with the release project, and the time limits are doubled.
      name: 'pwa-check',
      dependencies: ['pwa-check-site'],
      testMatch: /pwa-recovery\.spec\.ts$/,
      grepInvert: /committed cells come back closed after Reload/,
      timeout: 240_000,
      expect: { timeout: 20_000 },
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 800 }, baseURL: CHECK_BASE },
    },
    {
      name: 'pixel-7',
      testMatch: MOBILE_SPECS,
      use: { ...devices['Pixel 7'], hasTouch: true, storageState: TOUR_SEEN },
    },
    {
      // The iPad's viewport, touch and user agent. Playwright's WebKit needs system libraries this
      // build machine lacks (ICU 74, flite), so Chromium runs it unless KILN_IPAD_WEBKIT=1.
      name: 'ipad',
      testMatch: MOBILE_SPECS,
      use: {
        ...devices['iPad (gen 7)'],
        ...(process.env.KILN_IPAD_WEBKIT === '1' ? {} : { browserName: 'chromium' as const }),
        hasTouch: true,
        storageState: TOUR_SEEN,
      },
    },
    {
      name: 'budgets',
      grep: /@budget/,
      testIgnore: [MOBILE_SPECS, PWA_SPECS],
      // Run after the suite by run.mjs, alone, so other tests do not slow what they time.
      outputDir: './screenshots/.results-budgets',
      workers: 1,
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 }, storageState: TOUR_SEEN },
    },
  ],
  webServer: [
    {
      command: 'pnpm --filter @kiln/web dev',
      cwd: repoRoot,
      url: `http://localhost:${PORT}`,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      stdout: 'ignore',
      stderr: 'pipe',
    },
    // The local GitHub Pages emulation (no COOP/COEP), unless a deployed site is under test.
    ...(process.env.KILN_E2E_BASE
      ? []
      : [
          {
            command: `node tools/pages-serve.mjs --port ${CHECK_PORT} --root target/pages-check/site --test`,
            cwd: repoRoot,
            url: `http://127.0.0.1:${CHECK_PORT}/__kiln/health`,
            reuseExistingServer: !process.env.CI,
            timeout: 30_000,
            stdout: 'ignore' as const,
            stderr: 'pipe' as const,
          },
          {
            command: `node tools/pages-serve.mjs --port ${PAGES_PORT} --root ${PAGES_ROOT} --test`,
            cwd: repoRoot,
            url: `http://127.0.0.1:${PAGES_PORT}/__kiln/health`,
            reuseExistingServer: !process.env.CI,
            timeout: 30_000,
            stdout: 'ignore' as const,
            stderr: 'pipe' as const,
          },
        ]),
  ],
});
