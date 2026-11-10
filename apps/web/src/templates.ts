/**
 * The templates of the web build: `assets/templates/index.json` and the bundles and
 * thumbnails beside it (`assets/templates/<app>/<name>.k*` and `.png`, made by
 * `tools/templates/build.sh`). Vite copies every file into the build and gives its URL; File ▸ New
 * fetches a bundle and the core opens it as a new, untitled document (`OpenHint::template`), so the
 * template itself is never written.
 */
import { type HomeTemplate, kindApp } from '@kiln/home';
import type { DocKind, KilnClient } from '@kiln/protocol';

interface IndexEntry {
  id: string;
  kind: DocKind;
  name: string;
  file: string;
  thumbnail?: string | null;
  description?: string | null;
  featured?: boolean;
}

const ROOT = '../../../assets/templates/';

const index = import.meta.glob<{ version: number; templates: IndexEntry[] }>(
  '../../../assets/templates/index.json',
  { eager: true, import: 'default' },
);
const urls = import.meta.glob<string>('../../../assets/templates/*/*.{kwrite,kgrid,kdeck,png}', {
  eager: true,
  query: '?url',
  import: 'default',
});

const urlOf = (file: string | null | undefined): string | undefined =>
  file ? urls[`${ROOT}${file}`] : undefined;

/** Every template of the build, in the index's order (bundles that are missing are left out). */
export const TEMPLATES: readonly HomeTemplate[] = (Object.values(index)[0]?.templates ?? [])
  .filter((t) => urlOf(t.file) !== undefined)
  .map((t) => ({
    id: t.id,
    app: kindApp(t.kind),
    name: t.name,
    file: t.file,
    thumbnail: urlOf(t.thumbnail),
    description: t.description ?? undefined,
    featured: t.featured ?? false,
  }));

/** A template by its file (`write/letter.kwrite`), as the route names it. */
export function templateByFile(file: string): HomeTemplate | undefined {
  return TEMPLATES.find((t) => t.file === file);
}

/**
 * Creates a new document from a template: fetches the bundle and has the core open it as a new,
 * untitled document. Resolves with the new document's summary.
 */
export async function newFromTemplate(client: KilnClient, template: HomeTemplate) {
  const file = template.file ?? '';
  const url = urlOf(file);
  if (!url) throw new Error(`the template ${file} is not in this build`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`the template ${file} could not be read (${response.status})`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const name = file.slice(file.lastIndexOf('/') + 1);
  return client.open(bytes, { name, path: null, format: null, read_only: false, template: true });
}
