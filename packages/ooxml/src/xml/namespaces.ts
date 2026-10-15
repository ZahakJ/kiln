/**
 * The stable namespace prefix table. Office writes these prefixes, so using the same ones keeps our
 * output diffable against Office files and lets readers query elements by qualified name (`w:pPr`).
 *
 * A prefix of `''` means Office writes that namespace as the default namespace (SpreadsheetML,
 * content types, relationships, extended and custom properties).
 */

export const NS = {
  // WordprocessingML
  w: 'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
  w10: 'urn:schemas-microsoft-com:office:word',
  w14: 'http://schemas.microsoft.com/office/word/2010/wordml',
  w15: 'http://schemas.microsoft.com/office/word/2012/wordml',
  w16: 'http://schemas.microsoft.com/office/word/2018/wordml',
  w16cex: 'http://schemas.microsoft.com/office/word/2018/wordml/cex',
  w16cid: 'http://schemas.microsoft.com/office/word/2016/wordml/cid',
  w16se: 'http://schemas.microsoft.com/office/word/2015/wordml/symex',
  w16sdtdh: 'http://schemas.microsoft.com/office/word/2020/wordml/sdtdatahash',
  wp: 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing',
  wp14: 'http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing',
  wps: 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
  wpg: 'http://schemas.microsoft.com/office/word/2010/wordprocessingGroup',
  wpc: 'http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas',
  wpi: 'http://schemas.microsoft.com/office/word/2010/wordprocessingInk',
  wne: 'http://schemas.microsoft.com/office/word/2006/wordml',
  // DrawingML
  a: 'http://schemas.openxmlformats.org/drawingml/2006/main',
  a14: 'http://schemas.microsoft.com/office/drawing/2010/main',
  a16: 'http://schemas.microsoft.com/office/drawing/2014/main',
  pic: 'http://schemas.openxmlformats.org/drawingml/2006/picture',
  c: 'http://schemas.openxmlformats.org/drawingml/2006/chart',
  c14: 'http://schemas.microsoft.com/office/drawing/2007/8/2/chart',
  c16: 'http://schemas.microsoft.com/office/drawing/2014/chart',
  c16r2: 'http://schemas.microsoft.com/office/drawing/2015/06/chart',
  cx: 'http://schemas.microsoft.com/office/drawing/2014/chartex',
  dgm: 'http://schemas.openxmlformats.org/drawingml/2006/diagram',
  xdr: 'http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing',
  cdr: 'http://schemas.openxmlformats.org/drawingml/2006/chartDrawing',
  lc: 'http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas',
  // PresentationML
  p: 'http://schemas.openxmlformats.org/presentationml/2006/main',
  p14: 'http://schemas.microsoft.com/office/powerpoint/2010/main',
  p15: 'http://schemas.microsoft.com/office/powerpoint/2012/main',
  p188: 'http://schemas.microsoft.com/office/powerpoint/2018/8/main',
  // SpreadsheetML (Office writes the main namespace as the default namespace)
  x: 'http://schemas.openxmlformats.org/spreadsheetml/2006/main',
  x14: 'http://schemas.microsoft.com/office/spreadsheetml/2009/9/main',
  x14ac: 'http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac',
  x15: 'http://schemas.microsoft.com/office/spreadsheetml/2010/11/main',
  x15ac: 'http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac',
  x16r2: 'http://schemas.microsoft.com/office/spreadsheetml/2015/02/main',
  xr: 'http://schemas.microsoft.com/office/spreadsheetml/2014/revision',
  xr2: 'http://schemas.microsoft.com/office/spreadsheetml/2015/revision2',
  xr3: 'http://schemas.microsoft.com/office/spreadsheetml/2016/revision3',
  xr6: 'http://schemas.microsoft.com/office/spreadsheetml/2016/revision6',
  xr10: 'http://schemas.microsoft.com/office/spreadsheetml/2016/revision10',
  xm: 'http://schemas.microsoft.com/office/excel/2006/main',
  // Shared
  r: 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
  m: 'http://schemas.openxmlformats.org/officeDocument/2006/math',
  mc: 'http://schemas.openxmlformats.org/markup-compatibility/2006',
  vt: 'http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes',
  sl: 'http://schemas.openxmlformats.org/schemaLibrary/2006/main',
  // Document properties
  cp: 'http://schemas.openxmlformats.org/package/2006/metadata/core-properties',
  dc: 'http://purl.org/dc/elements/1.1/',
  dcterms: 'http://purl.org/dc/terms/',
  dcmitype: 'http://purl.org/dc/dcmitype/',
  xsi: 'http://www.w3.org/2001/XMLSchema-instance',
  ep: 'http://schemas.openxmlformats.org/officeDocument/2006/extended-properties',
  op: 'http://schemas.openxmlformats.org/officeDocument/2006/custom-properties',
  // Packaging
  ct: 'http://schemas.openxmlformats.org/package/2006/content-types',
  rel: 'http://schemas.openxmlformats.org/package/2006/relationships',
  // VML (legacy)
  v: 'urn:schemas-microsoft-com:vml',
  o: 'urn:schemas-microsoft-com:office:office',
} as const;

export type NsPrefix = keyof typeof NS;

/**
 * Namespaces that Office serialises as the *default* namespace. Their elements are unprefixed in
 * our trees (`sheetData`, `Relationship`, `Override`, `Application`, `property`).
 */
export const DEFAULT_NAMESPACE_URIS: ReadonlySet<string> = new Set([
  NS.x,
  NS.ct,
  NS.rel,
  NS.ep,
  NS.op,
  'http://purl.oclc.org/ooxml/spreadsheetml/main',
]);

/** ISO/IEC 29500 Strict namespaces, mapped to the transitional prefix readers query by. */
const STRICT_URIS: Record<string, string> = {
  'http://purl.oclc.org/ooxml/wordprocessingml/main': 'w',
  'http://purl.oclc.org/ooxml/drawingml/main': 'a',
  'http://purl.oclc.org/ooxml/drawingml/picture': 'pic',
  'http://purl.oclc.org/ooxml/drawingml/chart': 'c',
  'http://purl.oclc.org/ooxml/drawingml/diagram': 'dgm',
  'http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing': 'xdr',
  'http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing': 'wp',
  'http://purl.oclc.org/ooxml/presentationml/main': 'p',
  'http://purl.oclc.org/ooxml/officeDocument/relationships': 'r',
  'http://purl.oclc.org/ooxml/officeDocument/math': 'm',
  'http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes': 'vt',
  'http://purl.oclc.org/ooxml/officeDocument/extendedProperties': '',
  'http://purl.oclc.org/ooxml/officeDocument/customProperties': '',
};

const PREFIX_ORDER = Object.keys(NS) as NsPrefix[];

const URI_TO_PREFIX: Map<string, string> = (() => {
  const m = new Map<string, string>();
  for (const p of PREFIX_ORDER) {
    const uri = NS[p];
    if (m.has(uri)) continue;
    m.set(uri, DEFAULT_NAMESPACE_URIS.has(uri) ? '' : p);
  }
  // VML drawings in workbooks bind `x` to the legacy Excel namespace.
  m.set('urn:schemas-microsoft-com:office:excel', 'x');
  for (const [uri, p] of Object.entries(STRICT_URIS)) m.set(uri, p);
  return m;
})();

/** The canonical prefix Office uses for a namespace URI (`''` = default namespace), if known. */
export function canonicalPrefix(uri: string): string | undefined {
  return URI_TO_PREFIX.get(uri);
}

/**
 * The namespace URI for a prefix in the stable table, used when a builder emits an element with a
 * prefix that no ancestor declares. `x` resolves to SpreadsheetML main.
 */
export function namespaceForPrefix(prefix: string): string | undefined {
  return (NS as Record<string, string>)[prefix];
}

/** Rank of a prefix in the stable table, so automatically added declarations come out in a fixed order. */
export function prefixRank(prefix: string): number {
  const i = PREFIX_ORDER.indexOf(prefix as NsPrefix);
  return i < 0 ? PREFIX_ORDER.length : i;
}

/** Splits `w:pPr` into `['w', 'pPr']` and `sheetData` into `['', 'sheetData']`. */
export function splitQName(name: string): [prefix: string, local: string] {
  const i = name.indexOf(':');
  return i < 0 ? ['', name] : [name.slice(0, i), name.slice(i + 1)];
}

export function localName(name: string): string {
  const i = name.indexOf(':');
  return i < 0 ? name : name.slice(i + 1);
}

/** Attributes whose *value* is a whitespace-separated list of namespace prefixes (Markup Compatibility). */
export const PREFIX_LIST_ATTRIBUTES: ReadonlySet<string> = new Set([
  'mc:Ignorable',
  'mc:MustUnderstand',
  'mc:ProcessContent',
  'mc:PreserveElements',
  'mc:PreserveAttributes',
  'Requires',
]);
