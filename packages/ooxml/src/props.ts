/**
 * Document properties: core (`docProps/core.xml`, Dublin Core), extended (`docProps/app.xml`) and custom
 * (`docProps/custom.xml`) — ECMA-376 Part 2 §11 and Part 1 §22.2–22.3.
 */
import { ContentType } from './opc/content-types.ts';
import type { OpcPackage } from './opc/package.ts';
import { RelType } from './opc/rel-types.ts';
import { NS } from './xml/namespaces.ts';
import { cloneNode, el, type XmlElement } from './xml/node.ts';
import { attr, child, children, text } from './xml/query.ts';

export interface CoreProperties {
  title?: string;
  subject?: string;
  creator?: string;
  keywords?: string;
  description?: string;
  lastModifiedBy?: string;
  revision?: string;
  /** W3CDTF timestamps (`2026-10-15T09:30:00Z`). */
  created?: string;
  modified?: string;
  lastPrinted?: string;
  category?: string;
  contentStatus?: string;
  language?: string;
  identifier?: string;
  version?: string;
}

const CORE_FIELDS: [keyof CoreProperties, string][] = [
  ['title', 'dc:title'],
  ['subject', 'dc:subject'],
  ['creator', 'dc:creator'],
  ['keywords', 'cp:keywords'],
  ['description', 'dc:description'],
  ['lastModifiedBy', 'cp:lastModifiedBy'],
  ['revision', 'cp:revision'],
  ['lastPrinted', 'cp:lastPrinted'],
  ['created', 'dcterms:created'],
  ['modified', 'dcterms:modified'],
  ['category', 'cp:category'],
  ['contentStatus', 'cp:contentStatus'],
  ['language', 'dc:language'],
  ['identifier', 'dc:identifier'],
  ['version', 'cp:version'],
];

export interface AppProperties {
  template?: string;
  totalTime?: number;
  pages?: number;
  words?: number;
  characters?: number;
  application?: string;
  docSecurity?: number;
  lines?: number;
  paragraphs?: number;
  slides?: number;
  notes?: number;
  hiddenSlides?: number;
  mmClips?: number;
  scaleCrop?: boolean;
  manager?: string;
  company?: string;
  linksUpToDate?: boolean;
  charactersWithSpaces?: number;
  sharedDoc?: boolean;
  hyperlinkBase?: string;
  hyperlinksChanged?: boolean;
  appVersion?: string;
  presentationFormat?: string;
  /** `HeadingPairs`, `TitlesOfParts`, `HLinks`, `DigSig`, preserved verbatim. */
  extras?: XmlElement[];
}

/** Schema order of app.xml elements, with their types. */
const APP_FIELDS: [keyof AppProperties, string, 'str' | 'int' | 'bool'][] = [
  ['template', 'Template', 'str'],
  ['totalTime', 'TotalTime', 'int'],
  ['pages', 'Pages', 'int'],
  ['words', 'Words', 'int'],
  ['characters', 'Characters', 'int'],
  ['application', 'Application', 'str'],
  ['docSecurity', 'DocSecurity', 'int'],
  ['presentationFormat', 'PresentationFormat', 'str'],
  ['lines', 'Lines', 'int'],
  ['paragraphs', 'Paragraphs', 'int'],
  ['slides', 'Slides', 'int'],
  ['notes', 'Notes', 'int'],
  ['hiddenSlides', 'HiddenSlides', 'int'],
  ['mmClips', 'MMClips', 'int'],
  ['scaleCrop', 'ScaleCrop', 'bool'],
  ['manager', 'Manager', 'str'],
  ['company', 'Company', 'str'],
  ['linksUpToDate', 'LinksUpToDate', 'bool'],
  ['charactersWithSpaces', 'CharactersWithSpaces', 'int'],
  ['sharedDoc', 'SharedDoc', 'bool'],
  ['hyperlinkBase', 'HyperlinkBase', 'str'],
  ['hyperlinksChanged', 'HyperlinksChanged', 'bool'],
  ['appVersion', 'AppVersion', 'str'],
];
/** Where Office places the structured elements we preserve verbatim (before the named element). */
const APP_EXTRAS_BEFORE: Record<string, string> = {
  HeadingPairs: 'Manager',
  TitlesOfParts: 'Manager',
  HLinks: 'HyperlinksChanged',
  DigSig: 'AppVersion',
};

export type CustomPropertyValue =
  | { type: 'lpwstr'; value: string }
  | { type: 'i4'; value: number }
  | { type: 'r8'; value: number }
  | { type: 'bool'; value: boolean }
  | { type: 'filetime'; value: string }
  | { type: 'other'; element: XmlElement };

export interface CustomProperty {
  name: string;
  value: CustomPropertyValue;
  /** `linkTarget` (a bookmark the property is linked to), if any. */
  linkTarget?: string;
}

export interface DocumentProperties {
  core: CoreProperties;
  app: AppProperties;
  custom: CustomProperty[];
}

const CUSTOM_FMTID = '{D5CDD505-2E9C-101B-9397-08002B2CF9AE}';

// ------------------------------------------------------------------ read

export function parseCoreProperties(root: XmlElement): CoreProperties {
  const out: CoreProperties = {};
  for (const [key, name] of CORE_FIELDS) {
    const c = child(root, name);
    if (c) out[key] = text(c);
  }
  return out;
}

export function parseAppProperties(root: XmlElement): AppProperties {
  const out: AppProperties = {};
  const rec = out as Record<string, unknown>;
  const known = new Set(APP_FIELDS.map(([, n]) => n));
  for (const [key, name, type] of APP_FIELDS) {
    const c = child(root, name);
    if (!c) continue;
    const t = text(c).trim();
    if (type === 'str') rec[key] = text(c);
    else if (type === 'int') {
      const n = Number.parseInt(t, 10);
      if (Number.isFinite(n)) rec[key] = n;
    } else rec[key] = t === 'true' || t === '1';
  }
  const extras = children(root).filter((c) => !known.has(c.name));
  if (extras.length) out.extras = extras.map((c) => cloneNode(c));
  return out;
}

export function parseCustomProperties(root: XmlElement): CustomProperty[] {
  return children(root, 'property').map((p) => {
    const v = children(p)[0];
    let value: CustomPropertyValue;
    const t = text(v);
    switch (v?.name) {
      case 'vt:lpwstr':
      case 'vt:bstr':
        value = { type: 'lpwstr', value: t };
        break;
      case 'vt:i4':
      case 'vt:int':
        value = { type: 'i4', value: Number.parseInt(t, 10) };
        break;
      case 'vt:r8':
      case 'vt:r4':
        value = { type: 'r8', value: Number(t) };
        break;
      case 'vt:bool':
        value = { type: 'bool', value: t.trim() === 'true' || t.trim() === '1' };
        break;
      case 'vt:filetime':
        value = { type: 'filetime', value: t };
        break;
      default:
        value = { type: 'other', element: v ? cloneNode(v) : el('vt:empty') };
    }
    const prop: CustomProperty = { name: attr(p, 'name') ?? '', value };
    const link = attr(p, 'linkTarget');
    if (link) prop.linkTarget = link;
    return prop;
  });
}

/** Reads core, extended and custom properties from a package (missing parts give empty values). */
export function readDocumentProperties(pkg: OpcPackage): DocumentProperties {
  const read = (type: string) => {
    const name = pkg.relatedPart('/', type);
    return name ? pkg.getPartXml(name)?.root : undefined;
  };
  const core = read(RelType.coreProperties);
  const app = read(RelType.extendedProperties);
  const custom = read(RelType.customProperties);
  return {
    core: core ? parseCoreProperties(core) : {},
    app: app ? parseAppProperties(app) : {},
    custom: custom ? parseCustomProperties(custom) : [],
  };
}

// ----------------------------------------------------------------- write

export function serializeCoreProperties(core: CoreProperties): XmlElement {
  return el(
    'cp:coreProperties',
    {
      'xmlns:cp': NS.cp,
      'xmlns:dc': NS.dc,
      'xmlns:dcterms': NS.dcterms,
      'xmlns:dcmitype': NS.dcmitype,
      'xmlns:xsi': NS.xsi,
    },
    CORE_FIELDS.map(([key, name]) => {
      const v = core[key];
      if (v === undefined) return null;
      return el(name, name.startsWith('dcterms:') ? { 'xsi:type': 'dcterms:W3CDTF' } : {}, v);
    }),
  );
}

export function serializeAppProperties(app: AppProperties): XmlElement {
  const rec = app as Record<string, unknown>;
  const extras = app.extras ?? [];
  const kids: XmlElement[] = [];
  const emitExtras = (before: string) => {
    for (const x of extras) if (APP_EXTRAS_BEFORE[x.name] === before) kids.push(cloneNode(x));
  };
  for (const [key, name, type] of APP_FIELDS) {
    emitExtras(name);
    const v = rec[key];
    if (v === undefined) continue;
    kids.push(el(name, {}, type === 'bool' ? (v ? 'true' : 'false') : String(v)));
  }
  for (const x of extras) if (!APP_EXTRAS_BEFORE[x.name]) kids.push(cloneNode(x));
  return el('Properties', { xmlns: NS.ep, 'xmlns:vt': NS.vt }, kids);
}

export function serializeCustomProperties(props: readonly CustomProperty[]): XmlElement {
  return el(
    'Properties',
    { xmlns: NS.op, 'xmlns:vt': NS.vt },
    props.map((p, i) => {
      const v = p.value;
      let valueEl: XmlElement;
      switch (v.type) {
        case 'lpwstr':
          valueEl = el('vt:lpwstr', {}, v.value);
          break;
        case 'i4':
          valueEl = el('vt:i4', {}, String(Math.trunc(v.value)));
          break;
        case 'r8':
          valueEl = el('vt:r8', {}, String(v.value));
          break;
        case 'bool':
          valueEl = el('vt:bool', {}, v.value ? 'true' : 'false');
          break;
        case 'filetime':
          valueEl = el('vt:filetime', {}, v.value);
          break;
        default:
          valueEl = cloneNode(v.element);
      }
      return el(
        'property',
        { fmtid: CUSTOM_FMTID, pid: i + 2, name: p.name, linkTarget: p.linkTarget },
        valueEl,
      );
    }),
  );
}

function upsertPropsPart(
  pkg: OpcPackage,
  relType: string,
  defaultName: string,
  contentType: string,
  root: XmlElement | undefined,
): void {
  const existing = pkg.relatedPart('/', relType);
  if (!root) {
    if (existing) pkg.removePart(existing);
    return;
  }
  if (existing) {
    pkg.setPart(existing, root, contentType);
    return;
  }
  pkg.addPart(defaultName, contentType, root);
  pkg.addRelationship('/', relType, defaultName);
}

/**
 * Writes properties into a package, creating `docProps/*.xml` and their package relationships as needed.
 * An empty custom-property list removes `custom.xml`.
 */
export function writeDocumentProperties(pkg: OpcPackage, props: Partial<DocumentProperties>): void {
  if (props.core) {
    upsertPropsPart(
      pkg,
      RelType.coreProperties,
      '/docProps/core.xml',
      ContentType.coreProperties,
      serializeCoreProperties(props.core),
    );
  }
  if (props.app) {
    upsertPropsPart(
      pkg,
      RelType.extendedProperties,
      '/docProps/app.xml',
      ContentType.extendedProperties,
      serializeAppProperties(props.app),
    );
  }
  if (props.custom) {
    upsertPropsPart(
      pkg,
      RelType.customProperties,
      '/docProps/custom.xml',
      ContentType.customProperties,
      props.custom.length ? serializeCustomProperties(props.custom) : undefined,
    );
  }
}
