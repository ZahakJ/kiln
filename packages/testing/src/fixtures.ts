import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { basename, join, relative } from 'node:path';
import { FIXTURES_DIR } from './paths.ts';

export type FixtureFormat = 'docx' | 'xlsx' | 'pptx';
export type FixtureKind = 'features' | 'manual' | 'external';
export const FIXTURE_FORMATS: readonly FixtureFormat[] = ['docx', 'xlsx', 'pptx'];

export class FixtureNotFoundError extends Error {
  override name = 'FixtureNotFoundError';
}

/**
 * Absolute path of a fixture. `name` may be a path relative to the fixtures root
 * (`docx/features/styles.docx`) or a bare file name (`styles.docx`), which is looked up in
 * `features/`, then `manual/`, then `external/` of the format given by its extension.
 */
export function fixturePath(name: string): string {
  const direct = join(FIXTURES_DIR, name);
  if (name.includes('/')) {
    if (existsSync(direct)) return direct;
    throw new FixtureNotFoundError(`no fixture ${name} under ${FIXTURES_DIR}`);
  }
  const ext = name.slice(name.lastIndexOf('.') + 1).toLowerCase();
  const candidates = (['features', 'manual'] as const).map((k) => join(FIXTURES_DIR, ext, k, name));
  candidates.push(join(FIXTURES_DIR, 'external', ext, name));
  for (const c of candidates) if (existsSync(c)) return c;
  throw new FixtureNotFoundError(
    `no fixture named ${name} (looked in ${candidates.map((c) => relative(FIXTURES_DIR, c)).join(', ')})`,
  );
}

/** Reads a fixture's bytes (see `fixturePath` for how names resolve). */
export function loadFixture(name: string): Uint8Array {
  return new Uint8Array(readFileSync(fixturePath(name)));
}

/** Absolute paths of the fixtures of a format and kind, sorted by name. Missing directories give `[]`. */
export function listFixtures(format: FixtureFormat, kind: FixtureKind = 'features'): string[] {
  const dir = kind === 'external' ? join(FIXTURES_DIR, 'external', format) : join(FIXTURES_DIR, format, kind);
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => f.toLowerCase().endsWith(`.${format}`))
    .sort()
    .map((f) => join(dir, f));
}

/** `docx/features/styles.docx` style label for test names. */
export function fixtureLabel(path: string): string {
  return relative(FIXTURES_DIR, path) || basename(path);
}
