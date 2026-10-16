/**
 * String-based XML emission for the DOCX writer. WordprocessingML parts are large and mostly made of
 * small elements, and the model keeps unmodelled markup as raw XML fragments; building strings directly
 * (instead of element trees) keeps writing linear and lets fragments be embedded verbatim.
 *
 * Every attribute value and text passes through `escapeAttr`/`escapeText` from `@kiln-ref/ooxml`, which
 * also strip characters XML 1.0 forbids, so no model content can produce a malformed part.
 */
import { escapeAttr, escapeText, NS, stripInvalidXmlChars } from '@kiln-ref/ooxml';

export type AttrValue = string | number | boolean | undefined | null;
export type Attrs = readonly (readonly [string, AttrValue])[];

/** Attributes in the given order; `undefined`/`null` are skipped, booleans become `1`/`0`. */
export function attrs(list: Attrs): string {
  let s = '';
  for (const [k, v] of list) {
    if (v === undefined || v === null) continue;
    const value = typeof v === 'boolean' ? (v ? '1' : '0') : String(v);
    s += ` ${k}="${escapeAttr(value)}"`;
  }
  return s;
}

/** `<name attrs>children</name>`, or `<name attrs/>` when there are no children. */
export function tag(name: string, list: Attrs = [], children = ''): string {
  return children ? `<${name}${attrs(list)}>${children}</${name}>` : `<${name}${attrs(list)}/>`;
}

/** `<name w:val="value"/>` (the most common WordprocessingML shape); `''` when `value` is undefined. */
export function val(name: string, value: AttrValue): string {
  if (value === undefined || value === null) return '';
  return tag(name, [['w:val', value]]);
}

/** A toggle property: `<w:b/>` when true, `<w:b w:val="0"/>` when false, nothing when undefined. */
export function toggle(name: string, value: boolean | undefined | null): string {
  if (value === undefined || value === null) return '';
  return value ? `<${name}/>` : `<${name} w:val="0"/>`;
}

export { escapeText, stripInvalidXmlChars };

/** Whether `w:t` text needs `xml:space="preserve"` (leading/trailing or repeated whitespace, tabs). */
export function needsPreserve(text: string): boolean {
  return /^\s|\s$|\s\s|[\t\n\r]/.test(text);
}

/** The qualified name of a fragment's root element (`<w:numPr …>` → `w:numPr`). */
export function fragmentName(xml: string): string {
  const m = /^\s*<([\w.:-]+)/.exec(xml);
  return m ? (m[1] as string) : '';
}

/** Two-digit upper-case hex of a byte (theme tint/shade values). */
export function hexByte(n: number): string {
  return Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .toUpperCase()
    .padStart(2, '0');
}

/**
 * Orders `[elementName, xml]` entries by a schema sequence. Names not in the sequence keep their
 * relative order and go where `unknownBefore` says (before that element, or at the end).
 */
export function orderBySequence(
  entries: readonly (readonly [string, string])[],
  sequence: readonly string[],
  unknownBefore?: string,
): string {
  const rank = new Map(sequence.map((n, i) => [n, i]));
  const unknownRank = unknownBefore !== undefined ? (rank.get(unknownBefore) ?? sequence.length) - 0.5 : 1e9;
  return entries
    .map(([name, xml], i) => ({ r: rank.get(name) ?? unknownRank, i, xml }))
    .sort((a, b) => a.r - b.r || a.i - b.i)
    .map((e) => e.xml)
    .join('');
}

/** Namespaces Word declares as ignorable (Word 2010+ extensions). */
const IGNORABLE = ['w14', 'w15', 'w16se', 'w16cid', 'w16', 'w16cex', 'w16sdtdh', 'w16du', 'wp14'] as const;

/** Prefixes with known URIs beyond the stable `@kiln-ref/ooxml` table. */
const EXTRA_NS: Record<string, string> = {
  w16du: 'http://schemas.microsoft.com/office/word/2023/wordml/word16du',
  aink: 'http://schemas.microsoft.com/office/drawing/2016/ink',
  am3d: 'http://schemas.microsoft.com/office/drawing/2017/model3d',
  cx1: 'http://schemas.microsoft.com/office/drawing/2015/9/8/chartex',
  cx2: 'http://schemas.microsoft.com/office/drawing/2015/10/21/chartex',
  cx3: 'http://schemas.microsoft.com/office/drawing/2016/5/9/chartex',
  cx4: 'http://schemas.microsoft.com/office/drawing/2016/5/10/chartex',
  cx5: 'http://schemas.microsoft.com/office/drawing/2016/5/11/chartex',
  cx6: 'http://schemas.microsoft.com/office/drawing/2016/5/12/chartex',
  cx7: 'http://schemas.microsoft.com/office/drawing/2016/5/13/chartex',
  cx8: 'http://schemas.microsoft.com/office/drawing/2016/5/14/chartex',
};

/** Root prefixes in Word's declaration order (the order Word 2019+ writes on `w:document`). */
const ROOT_ORDER = [
  'wpc',
  'cx',
  'cx1',
  'cx2',
  'cx3',
  'cx4',
  'cx5',
  'cx6',
  'cx7',
  'cx8',
  'mc',
  'aink',
  'am3d',
  'o',
  'r',
  'm',
  'v',
  'wp14',
  'wp',
  'w10',
  'w',
  'w14',
  'w15',
  'w16cex',
  'w16cid',
  'w16',
  'w16du',
  'w16sdtdh',
  'w16se',
  'wpg',
  'wpi',
  'wne',
  'wps',
];

function uriOf(prefix: string): string | undefined {
  return (NS as Record<string, string>)[prefix] ?? EXTRA_NS[prefix];
}

/**
 * Root namespace declarations for a part body: every stable prefix the body uses (element and
 * attribute names, `mc:Ignorable`/`Requires` lists) plus `always`, in Word's order, followed by
 * `mc:Ignorable` listing the Word extension namespaces that are declared. Foreign prefixes (unknown
 * URIs) are declared by their own fragments (see `fragmentXml`) and are left alone.
 */
export function rootNamespaces(body: string, always: readonly string[] = ['w', 'r']): string {
  const used = new Set<string>(always);
  for (const m of body.matchAll(/<\/?([A-Za-z][\w.-]*):[\w.-]+/g)) used.add(m[1] as string);
  for (const m of body.matchAll(/\s([A-Za-z][\w.-]*):[\w.-]+=/g)) {
    const p = m[1] as string;
    if (p !== 'xmlns' && p !== 'xml') used.add(p);
  }
  for (const m of body.matchAll(
    /\s(?:mc:Ignorable|Requires|mc:PreserveAttributes|mc:ProcessContent)="([^"]*)"/g,
  )) {
    for (const p of (m[1] as string).split(/\s+/)) if (p) used.add(p.split(':')[0] as string);
  }
  const declared: string[] = [];
  const ordered = [
    ...ROOT_ORDER.filter((p) => used.has(p)),
    ...[...used].filter((p) => !ROOT_ORDER.includes(p)).sort(),
  ];
  let s = '';
  for (const p of ordered) {
    const uri = uriOf(p);
    // Foreign prefixes have no known URI: their fragments declare them (see `fragmentXml`).
    if (!uri) continue;
    declared.push(p);
    s += ` xmlns:${p}="${uri}"`;
  }
  const ignorable = IGNORABLE.filter((p) => declared.includes(p));
  if (ignorable.length) {
    if (!declared.includes('mc')) s = ` xmlns:mc="${NS.mc}"${s}`;
    s += ` mc:Ignorable="${ignorable.join(' ')}"`;
  }
  return s;
}

/** The XML declaration Office writes. */
export const XML_DECLARATION = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n';

/** A complete part: declaration, root with namespace declarations, body. */
export function part(root: string, body: string, extraAttrs = '', always?: readonly string[]): string {
  return `${XML_DECLARATION}<${root}${rootNamespaces(body, always)}${extraAttrs}>${body}</${root}>`;
}
