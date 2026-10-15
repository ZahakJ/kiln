import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

/** Root of @officina/testing. */
export const PACKAGE_DIR = join(here, '..');
/** Repository root. */
export const REPO_ROOT = join(PACKAGE_DIR, '../..');
/** `packages/testing/fixtures`: `<format>/features` (generated), `<format>/manual`, `external` (downloaded). */
export const FIXTURES_DIR = join(PACKAGE_DIR, 'fixtures');
export const TOOLS_DIR = join(REPO_ROOT, 'tools');
export const PYTHON_DIR = join(PACKAGE_DIR, 'python');
