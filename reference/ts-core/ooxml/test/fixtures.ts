import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

/** The generated corpus lives in @kiln/testing; read it by path to keep ooxml free of that dependency. */
export const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '../../testing/fixtures');

export function fixture(rel: string): Uint8Array {
  return new Uint8Array(readFileSync(join(FIXTURES_DIR, rel)));
}

export function featureFixtures(format: 'docx' | 'xlsx' | 'pptx'): string[] {
  return readdirSync(join(FIXTURES_DIR, format, 'features'))
    .filter((f) => f.endsWith(`.${format}`))
    .sort()
    .map((f) => `${format}/features/${f}`);
}

/** Real-world files downloaded by `tools/fetch-corpus.sh` (absent on fresh clones and CI). */
export function externalFixtures(): string[] {
  const dir = join(FIXTURES_DIR, 'external');
  if (!existsSync(dir)) return [];
  return ['docx', 'xlsx', 'pptx'].flatMap((f) =>
    existsSync(join(dir, f))
      ? readdirSync(join(dir, f))
          .filter((n) => n.endsWith(`.${f}`))
          .sort()
          .map((n) => `external/${f}/${n}`)
      : [],
  );
}
