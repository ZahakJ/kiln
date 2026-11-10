/**
 * Builds the check-core site for the `pwa-check` project (`tools/check-site.sh`), when it is
 * missing or older than the release site and the sources.
 */
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from '@playwright/test';

test('the check-core site is built', () => {
  test.setTimeout(3_600_000);
  const root = fileURLToPath(new URL('../..', import.meta.url));
  execFileSync('bash', ['tools/check-site.sh'], { cwd: root, stdio: 'inherit' });
});
