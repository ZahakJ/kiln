/**
 * Formats generated sources with the repository's Biome configuration, so generated files are exactly
 * what `biome check` expects. Used by the generator and by the drift test, which therefore compare the
 * same bytes.
 */
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const biome = createRequire(import.meta.url).resolve('@biomejs/biome/bin/biome');
const pkgDir = fileURLToPath(new URL('..', import.meta.url));

/** `source` formatted as if it were the file `fileName` (relative to packages/tokens). */
export function formatGenerated(source: string, fileName: string): string {
  return execFileSync(process.execPath, [biome, 'format', `--stdin-file-path=${fileName}`], {
    cwd: pkgDir,
    input: source,
    encoding: 'utf8',
    stdio: ['pipe', 'pipe', 'pipe'],
  });
}
