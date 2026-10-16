/**
 * A small, allocation-light XML scanner for the two large SpreadsheetML structures — `sheetData` and
 * the shared-string table — which the generic DOM parser handles too slowly for 100k-row sheets.
 * It understands exactly what those structures contain (elements, attributes, text, entities) and
 * leaves everything else to `parseXml`.
 */
import { decodeEntities } from './text.ts';

/** Attributes of the tag starting at `xml[start] === '<'`; returns the index after `>` in `end`. */
export function scanAttrs(xml: string, start: number, out: Record<string, string>): number {
  // skip "<name"
  let i = start + 1;
  const n = xml.length;
  while (i < n) {
    const c = xml.charCodeAt(i);
    if (c === 32 || c === 9 || c === 10 || c === 13 || c === 62 || c === 47) break;
    i++;
  }
  for (;;) {
    while (i < n) {
      const c = xml.charCodeAt(i);
      if (c !== 32 && c !== 9 && c !== 10 && c !== 13) break;
      i++;
    }
    const c = xml.charCodeAt(i);
    if (c === 62) return i + 1; // >
    if (c === 47) return i + 2; // />
    if (i >= n) return n;
    const nameStart = i;
    while (i < n) {
      const d = xml.charCodeAt(i);
      if (d === 61 || d === 32 || d === 9 || d === 10 || d === 13) break;
      i++;
    }
    const name = xml.slice(nameStart, i);
    while (i < n && xml.charCodeAt(i) !== 61) i++;
    i++; // =
    while (i < n) {
      const d = xml.charCodeAt(i);
      if (d === 34 || d === 39) break;
      i++;
    }
    const quote = xml.charCodeAt(i);
    const vStart = i + 1;
    const vEnd = xml.indexOf(quote === 34 ? '"' : "'", vStart);
    if (vEnd < 0) return n;
    const raw = xml.slice(vStart, vEnd);
    out[name] = raw.indexOf('&') >= 0 ? decodeEntities(raw) : raw;
    i = vEnd + 1;
  }
}

/** True when the tag at `start` is self-closing (`<x/>`). */
export function isSelfClosing(xml: string, tagEnd: number): boolean {
  return xml.charCodeAt(tagEnd - 2) === 47;
}

/** The local name of the tag starting at `start` (`<x:row ` → `row`), and where the name ends. */
export function tagName(xml: string, start: number): string {
  let i = start + 1;
  const n = xml.length;
  let colon = -1;
  while (i < n) {
    const c = xml.charCodeAt(i);
    if (c === 32 || c === 9 || c === 10 || c === 13 || c === 62 || c === 47) break;
    if (c === 58) colon = i;
    i++;
  }
  return xml.slice(colon >= 0 ? colon + 1 : start + 1, i);
}

/**
 * Concatenated text of the `<t>` elements in `xml[start, end)`, skipping phonetic runs (`<rPh>`).
 * Returns undefined when the block contains rich-text runs (`<r>`), which callers parse fully.
 */
export function plainStringItem(xml: string, start: number, end: number, prefix: string): string | undefined {
  const rTag = `<${prefix}r>`;
  const rTag2 = `<${prefix}r `;
  const block = xml.slice(start, end);
  if (block.includes(rTag) || block.includes(rTag2)) return undefined;
  const tOpen = `<${prefix}t`;
  const tClose = `</${prefix}t>`;
  const phOpen = `<${prefix}rPh`;
  let out = '';
  let i = 0;
  for (;;) {
    const ph = block.indexOf(phOpen, i);
    let t = block.indexOf(tOpen, i);
    if (t < 0) break;
    if (ph >= 0 && ph < t) {
      const phEnd = block.indexOf(`</${prefix}rPh>`, ph);
      i = phEnd < 0 ? block.length : phEnd + 1;
      continue;
    }
    // `<t>` or `<t xml:space="preserve">`; not `<tx…>` (no such element inside si).
    const gt = block.indexOf('>', t);
    if (gt < 0) break;
    if (block.charCodeAt(gt - 1) === 47) {
      i = gt + 1;
      continue;
    }
    const close = block.indexOf(tClose, gt);
    if (close < 0) break;
    const raw = block.slice(gt + 1, close);
    out += raw.indexOf('&') >= 0 ? decodeEntities(raw) : raw;
    i = close + tClose.length;
    t = -1;
  }
  return out;
}

/** The namespace prefix (with colon, or empty) used for `localName` in a document. */
export function prefixOf(xml: string, localName: string): string {
  const re = new RegExp(`<([A-Za-z_][\\w.-]*:)?${localName}[\\s>/]`);
  const m = re.exec(xml);
  return m?.[1] ?? '';
}
