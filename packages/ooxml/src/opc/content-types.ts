import { buildXmlBytes } from '../xml/build.ts';
import { NS } from '../xml/namespaces.ts';
import { el } from '../xml/node.ts';
import { parseXml } from '../xml/parse.ts';
import { attr, children } from '../xml/query.ts';
import { extensionOf, partKey } from './part-name.ts';

/** Common content types. */
export const ContentType = {
  relationships: 'application/vnd.openxmlformats-package.relationships+xml',
  coreProperties: 'application/vnd.openxmlformats-package.core-properties+xml',
  extendedProperties: 'application/vnd.openxmlformats-officedocument.extended-properties+xml',
  customProperties: 'application/vnd.openxmlformats-officedocument.custom-properties+xml',
  theme: 'application/vnd.openxmlformats-officedocument.theme+xml',
  xml: 'application/xml',
  png: 'image/png',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  bmp: 'image/bmp',
  tiff: 'image/tiff',
  svg: 'image/svg+xml',
  emf: 'image/x-emf',
  wmf: 'image/x-wmf',
  // WordprocessingML
  docxDocument: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml',
  docxStyles: 'application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml',
  docxNumbering: 'application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml',
  docxSettings: 'application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml',
  // SpreadsheetML
  xlsxWorkbook: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml',
  xlsxWorksheet: 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml',
  xlsxSharedStrings: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml',
  xlsxStyles: 'application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml',
  // PresentationML
  pptxPresentation: 'application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml',
  pptxSlide: 'application/vnd.openxmlformats-officedocument.presentationml.slide+xml',
} as const;

/** Default content types for extensions Office registers as `<Default>` entries. */
export const DEFAULT_EXTENSION_TYPES: Readonly<Record<string, string>> = {
  rels: ContentType.relationships,
  xml: ContentType.xml,
  png: ContentType.png,
  jpeg: ContentType.jpeg,
  jpg: ContentType.jpeg,
  gif: ContentType.gif,
  bmp: ContentType.bmp,
  tif: ContentType.tiff,
  tiff: ContentType.tiff,
  svg: ContentType.svg,
  emf: ContentType.emf,
  wmf: ContentType.wmf,
  bin: 'application/vnd.openxmlformats-officedocument.oleObject',
  xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  mp4: 'video/mp4',
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  wav: 'audio/wav',
  odttf: 'application/vnd.openxmlformats-officedocument.obfuscatedFont',
  fntdata: 'application/x-fontdata',
};

/** True for XML part content types, which Office always registers as `<Override>`. */
export function isXmlContentType(ct: string): boolean {
  return ct.endsWith('+xml') || ct === 'application/xml' || ct === 'text/xml';
}

/** The `[Content_Types].xml` stream: extension defaults and per-part overrides, in document order. */
export class ContentTypes {
  /** extension (lower case) → content type */
  readonly defaults = new Map<string, string>();
  /** part key (lower case) → [part name as written, content type] */
  readonly overrides = new Map<string, [string, string]>();
  dirty = false;

  static parse(bytes: Uint8Array): ContentTypes {
    const ct = new ContentTypes();
    const root = parseXml(bytes).root;
    for (const d of children(root, 'Default')) {
      const ext = attr(d, 'Extension');
      const type = attr(d, 'ContentType');
      if (ext !== undefined && type !== undefined) ct.defaults.set(ext.toLowerCase(), type);
    }
    for (const o of children(root, 'Override')) {
      const name = attr(o, 'PartName');
      const type = attr(o, 'ContentType');
      if (name !== undefined && type !== undefined) ct.overrides.set(partKey(name), [name, type]);
    }
    return ct;
  }

  /** The content type of a part: its override, else the default for its extension. */
  get(partName: string): string | undefined {
    const o = this.overrides.get(partKey(partName));
    if (o) return o[1];
    return this.defaults.get(extensionOf(partName));
  }

  /**
   * Registers a part's content type the way Office does: a `<Default>` for non-XML media extensions
   * (adding one if the extension has none), otherwise an `<Override>`.
   */
  set(partName: string, contentType: string): void {
    const ext = extensionOf(partName);
    const def = this.defaults.get(ext);
    const key = partKey(partName);
    if (def === contentType) {
      if (this.overrides.delete(key)) this.dirty = true;
      return;
    }
    if (def === undefined && ext !== '' && !isXmlContentType(contentType)) {
      this.defaults.set(ext, contentType);
      this.overrides.delete(key);
      this.dirty = true;
      return;
    }
    const prev = this.overrides.get(key);
    if (!prev || prev[1] !== contentType) {
      this.overrides.set(key, [partName, contentType]);
      this.dirty = true;
    }
  }

  remove(partName: string): void {
    if (this.overrides.delete(partKey(partName))) this.dirty = true;
  }

  /** Drops `<Default>` entries for extensions no remaining part uses (except rels and xml). */
  pruneDefaults(partNames: Iterable<string>): void {
    const used = new Set(['rels', 'xml']);
    for (const p of partNames) used.add(extensionOf(p));
    for (const ext of [...this.defaults.keys()]) {
      if (!used.has(ext)) {
        this.defaults.delete(ext);
        this.dirty = true;
      }
    }
  }

  toBytes(): Uint8Array {
    return buildXmlBytes(
      el(
        'Types',
        { xmlns: NS.ct },
        [...this.defaults].map(([ext, type]) => el('Default', { Extension: ext, ContentType: type })),
        [...this.overrides.values()].map(([name, type]) =>
          el('Override', { PartName: name, ContentType: type }),
        ),
      ),
    );
  }
}
