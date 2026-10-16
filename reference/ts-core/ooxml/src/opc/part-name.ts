/** OPC part names (ECMA-376 Part 2 §6.2.2): absolute, `/`-separated, case-insensitive. */

export class PartNameError extends Error {
  override name = 'PartNameError';
}

/** Turns a zip entry name (`word/document.xml`) into a part name (`/word/document.xml`). */
export function zipNameToPartName(zipName: string): string {
  let n = zipName.replace(/\\/g, '/');
  try {
    n = decodeURIComponent(n);
  } catch {
    // Not percent-encoded; keep as is.
  }
  return n.startsWith('/') ? n : `/${n}`;
}

/** Turns a part name back into a zip entry name. */
export function partNameToZipName(partName: string): string {
  return partName.replace(/^\/+/, '');
}

/** Case-folded key used for part-name lookups (OPC part names compare case-insensitively). */
export function partKey(partName: string): string {
  return partName.toLowerCase();
}

/** Validates a part name; throws `PartNameError` on violations of the OPC grammar we rely on. */
export function assertPartName(name: string): void {
  if (!name.startsWith('/')) throw new PartNameError(`part name must start with "/": ${name}`);
  if (name.endsWith('/')) throw new PartNameError(`part name must not end with "/": ${name}`);
  const segments = name.slice(1).split('/');
  for (const s of segments) {
    if (s === '') throw new PartNameError(`empty segment in part name: ${name}`);
    if (s.endsWith('.')) throw new PartNameError(`segment ends with ".": ${name}`);
    if (s === '.' || s === '..') throw new PartNameError(`relative segment in part name: ${name}`);
  }
}

/** Extension without the dot, lower-cased (`/word/media/image1.PNG` → `png`). */
export function extensionOf(partName: string): string {
  const slash = partName.lastIndexOf('/');
  const dot = partName.lastIndexOf('.');
  return dot > slash ? partName.slice(dot + 1).toLowerCase() : '';
}

export function directoryOf(partName: string): string {
  return partName.slice(0, partName.lastIndexOf('/') + 1) || '/';
}

export function basenameOf(partName: string): string {
  return partName.slice(partName.lastIndexOf('/') + 1);
}

/** The relationships part for a source part: `/word/document.xml` → `/word/_rels/document.xml.rels`. */
export function relsPartName(sourcePart: string): string {
  if (sourcePart === '/' || sourcePart === '') return '/_rels/.rels';
  return `${directoryOf(sourcePart)}_rels/${basenameOf(sourcePart)}.rels`;
}

/** Inverse of `relsPartName`; `undefined` if `relsPart` is not a relationships part. */
export function sourcePartOfRels(relsPart: string): string | undefined {
  const m = /^(.*\/)_rels\/([^/]*)\.rels$/i.exec(relsPart);
  if (!m) return undefined;
  const [, dir, base] = m as unknown as [string, string, string];
  if (base === '') return dir === '/' ? '/' : undefined;
  return `${dir}${base}`;
}

/**
 * Resolves a relationship target against its source part. `target` may be relative (`media/image1.png`,
 * `../theme/theme1.xml`) or absolute (`/xl/workbook.xml`). Query and fragment are removed.
 */
export function resolveTarget(sourcePart: string, target: string): string {
  let t = target.split('#')[0]?.split('?')[0] ?? '';
  try {
    t = decodeURIComponent(t);
  } catch {
    // keep as is
  }
  t = t.replace(/\\/g, '/');
  const base = t.startsWith('/') ? [] : directoryOf(sourcePart === '' ? '/' : sourcePart).split('/');
  const segs = [...base, ...t.split('/')];
  const out: string[] = [];
  for (const s of segs) {
    if (s === '' || s === '.') continue;
    if (s === '..') out.pop();
    else out.push(s);
  }
  return `/${out.join('/')}`;
}

/**
 * The shortest relative reference from `sourcePart` to `targetPart`, as Office writes it
 * (`/word/document.xml` → `/word/media/image1.png` gives `media/image1.png`; package relationships use
 * the path without the leading slash).
 */
export function relativeTarget(sourcePart: string, targetPart: string): string {
  const from = directoryOf(sourcePart === '' ? '/' : sourcePart)
    .split('/')
    .filter(Boolean);
  const to = targetPart.split('/').filter(Boolean);
  let i = 0;
  while (i < from.length && i < to.length - 1 && from[i] === to[i]) i++;
  const up = from.length - i;
  return [...Array<string>(up).fill('..'), ...to.slice(i)].join('/');
}
