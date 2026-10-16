/** `xl/sharedStrings.xml`: plain and rich-text items (phonetic runs are ignored). */
import { buildXml, parseXml, type XmlElement } from '@officina/ooxml';
import type { RichTextRun } from '../types.ts';
import { plainStringItem, prefixOf } from '../util/scan.ts';
import { decodeXEscapes, encodeXEscapes, escText } from '../util/text.ts';
import { readFont, writeFont } from './styles.ts';

export interface SharedStrings {
  strings: string[];
  /** Items with formatting runs, by index. */
  rich: Map<number, RichTextRun[]>;
}

/** Text of an `<is>`/`<si>` element parsed as a tree (runs and phonetic properties handled). */
export function richRunsOf(e: XmlElement): RichTextRun[] {
  const runs: RichTextRun[] = [];
  for (const c of e.children) {
    if (c.type !== 'element') continue;
    if (c.name === 't') runs.push({ text: decodeXEscapes(textOf(c)) });
    else if (c.name === 'r') {
      const t = c.children.find((k): k is XmlElement => k.type === 'element' && k.name === 't');
      const rPr = c.children.find((k): k is XmlElement => k.type === 'element' && k.name === 'rPr');
      const run: RichTextRun = { text: t ? decodeXEscapes(textOf(t)) : '' };
      if (rPr) {
        const f = readFont(rPr);
        if (Object.keys(f).length) run.font = f;
      }
      runs.push(run);
    }
  }
  return runs;
}

function textOf(e: XmlElement): string {
  let s = '';
  for (const c of e.children) if (c.type === 'text') s += c.text;
  return s;
}

/** Parses a raw `<si>…</si>` / `<is>…</is>` block (which may use a namespace prefix). */
export function parseRichBlock(block: string, prefix: string): RichTextRun[] {
  let b = block;
  if (prefix) b = b.split(`<${prefix}`).join('<').split(`</${prefix}`).join('</');
  const root = parseXml(b, { normalizePrefixes: false }).root;
  return richRunsOf(root);
}

export function readSharedStrings(xml: string | undefined): SharedStrings {
  const out: SharedStrings = { strings: [], rich: new Map() };
  if (!xml) return out;
  const p = prefixOf(xml, 'sst');
  const open = `<${p}si`;
  const close = `</${p}si>`;
  let i = xml.indexOf('<', xml.indexOf(`<${p}sst`) + 1);
  for (;;) {
    const s = xml.indexOf(open, i);
    if (s < 0) break;
    const gt = xml.indexOf('>', s);
    if (xml.charCodeAt(gt - 1) === 47) {
      out.strings.push('');
      i = gt + 1;
      continue;
    }
    const e = xml.indexOf(close, gt);
    if (e < 0) break;
    const plain = plainStringItem(xml, gt + 1, e, p);
    if (plain !== undefined) out.strings.push(decodeXEscapes(plain));
    else {
      const runs = parseRichBlock(xml.slice(s, e + close.length), p);
      out.rich.set(out.strings.length, runs);
      out.strings.push(runs.map((r) => r.text).join(''));
    }
    i = e + close.length;
  }
  return out;
}

/** `<t>` text with `xml:space="preserve"` when needed. */
export function tElement(text: string): string {
  const v = escText(encodeXEscapes(text));
  return /^\s|\s$/.test(text) ? `<t xml:space="preserve">${v}</t>` : `<t>${v}</t>`;
}

/** Inner XML of a rich-text item (`<r><rPr/><t/></r>…`). */
export function richInnerXml(runs: readonly RichTextRun[]): string {
  let s = '';
  for (const r of runs) {
    s += '<r>';
    if (r.font && Object.keys(r.font).length) s += buildXml(writeFont(r.font, 'rPr', 'rFont'), { declaration: false });
    s += `${tElement(r.text)}</r>`;
  }
  return s;
}

/** Builds the shared-string table while the sheets are written. */
export class SharedStringTable {
  private readonly index = new Map<string, number>();
  private readonly items: string[] = [];
  /** Number of references (the `count` attribute). */
  refs = 0;

  add(text: string): number {
    this.refs++;
    let i = this.index.get(text);
    if (i === undefined) {
      i = this.items.length;
      this.index.set(text, i);
      this.items.push(tElement(text));
    }
    return i;
  }

  addRich(runs: readonly RichTextRun[]): number {
    this.refs++;
    const key = `\u0000rich${JSON.stringify(runs)}`;
    let i = this.index.get(key);
    if (i === undefined) {
      i = this.items.length;
      this.index.set(key, i);
      this.items.push(richInnerXml(runs));
    }
    return i;
  }

  get size(): number {
    return this.items.length;
  }

  toXml(): string {
    const parts = [
      '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n',
      `<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="${this.refs}" uniqueCount="${this.items.length}">`,
    ];
    for (const it of this.items) parts.push(`<si>${it}</si>`);
    parts.push('</sst>');
    return parts.join('');
  }
}
