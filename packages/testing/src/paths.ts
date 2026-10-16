import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));

/** Root of @kiln/testing. */
export const PACKAGE_DIR = join(here, '..');
/** Repository root. */
export const REPO_ROOT = join(PACKAGE_DIR, '../..');
/** `fixtures/` at the repository root (moved there in WP-50): `<format>/features` (generated), `<format>/manual`, `external` (downloaded). */
export const FIXTURES_DIR = join(REPO_ROOT, 'fixtures');
export const TOOLS_DIR = join(REPO_ROOT, 'tools');
export const PYTHON_DIR = join(PACKAGE_DIR, 'python');
