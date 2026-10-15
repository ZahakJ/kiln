/** `w:sectPr` → `DocxSectionProps` (ECMA-376 Part 1 §17.6). */
import { defaultSectionProps, type HeaderFooterType, type SectionBreakType } from '@officina/doc-model';
import { child, children, type XmlElement } from '@officina/ooxml';
import type { DocxSectionProps, PageBorders } from '../common/types.ts';
import { parseDecimal, parseOnOff, parseTwips } from '../common/units.ts';
import { attr } from '../common/xml.ts';
import { parseBorder, type RawXml } from './props.ts';
import { parseNoteProps } from './settings.ts';

const val = (n: XmlElement | undefined) => attr(n, 'w:val');
const BREAKS = new Set<string>(['nextPage', 'continuous', 'evenPage', 'oddPage', 'nextColumn']);

export interface ParsedSection {
  props: DocxSectionProps;
  /** Header/footer references: kind → relationship id. */
  headers: [HeaderFooterType, string][];
  footers: [HeaderFooterType, string][];
}

function hfType(v: string | undefined): HeaderFooterType {
  return v === 'first' || v === 'even' ? v : 'default';
}

function parsePageBorders(node: XmlElement): PageBorders {
  const pb: PageBorders = {};
  const offset = attr(node, 'w:offsetFrom');
  if (offset === 'page' || offset === 'text') pb.offsetFrom = offset;
  const display = attr(node, 'w:display');
  if (display === 'allPages' || display === 'firstPage' || display === 'notFirstPage') pb.display = display;
  const z = attr(node, 'w:zOrder');
  if (z === 'front' || z === 'back') pb.zOrder = z;
  for (const edge of ['top', 'left', 'bottom', 'right'] as const) {
    const b = parseBorder(child(node, `w:${edge}`));
    if (b) pb[edge] = b;
  }
  return pb;
}

/**
 * Parses a section's properties. Values the file omits take Word's defaults (Letter, 1" margins, one
 * column), as Word does for a bare `<w:sectPr/>`.
 */
export function parseSectionProps(node: XmlElement | undefined, raw: RawXml): ParsedSection {
  const props: DocxSectionProps = defaultSectionProps('letter');
  delete props.docGrid;
  const out: ParsedSection = { props, headers: [], footers: [] };
  if (!node) return out;
  const xml: string[] = [];
  for (const c of children(node)) {
    switch (c.name) {
      case 'w:headerReference': {
        const id = attr(c, 'r:id');
        if (id) out.headers.push([hfType(attr(c, 'w:type')), id]);
        break;
      }
      case 'w:footerReference': {
        const id = attr(c, 'r:id');
        if (id) out.footers.push([hfType(attr(c, 'w:type')), id]);
        break;
      }
      case 'w:type': {
        const t = val(c);
        if (t && BREAKS.has(t)) props.type = t as SectionBreakType;
        break;
      }
      case 'w:pgSz': {
        const w = parseTwips(attr(c, 'w:w'));
        const h = parseTwips(attr(c, 'w:h'));
        if (w !== undefined) props.pageSize.width = w;
        if (h !== undefined) props.pageSize.height = h;
        const orient = attr(c, 'w:orient');
        props.pageSize.orientation =
          orient === 'landscape' || (orient === undefined && w !== undefined && h !== undefined && w > h)
            ? 'landscape'
            : 'portrait';
        const code = parseDecimal(attr(c, 'w:code'));
        if (code !== undefined) props.pageSize.code = code;
        break;
      }
      case 'w:pgMar': {
        const m = props.margins;
        for (const k of ['top', 'right', 'bottom', 'left', 'header', 'footer', 'gutter'] as const) {
          const v = parseTwips(attr(c, `w:${k}`));
          if (v !== undefined) m[k] = v;
        }
        break;
      }
      case 'w:cols': {
        const count = parseDecimal(attr(c, 'w:num'));
        const space = parseTwips(attr(c, 'w:space'));
        const cols = children(c, 'w:col').map((col) => ({
          width: parseTwips(attr(col, 'w:w')) ?? 0,
          space: parseTwips(attr(col, 'w:space')) ?? 0,
        }));
        const eq = attr(c, 'w:equalWidth');
        props.columns = {
          count: Math.max(1, count ?? (cols.length || 1)),
          space: space ?? 720,
          equalWidth: eq === undefined ? cols.length === 0 : parseOnOff(eq),
          separator: parseOnOff(attr(c, 'w:sep'), false),
          ...(cols.length ? { cols } : {}),
        };
        break;
      }
      case 'w:titlePg':
        props.titlePage = parseOnOff(val(c));
        break;
      case 'w:pgNumType': {
        const start = parseDecimal(attr(c, 'w:start'));
        const fmt = attr(c, 'w:fmt');
        props.pageNumbering = {
          ...(start !== undefined ? { start } : {}),
          ...(fmt ? { format: fmt } : {}),
        };
        // Chapter numbering attributes (w:chapStyle, w:chapSep) are kept verbatim.
        if (attr(c, 'w:chapStyle') !== undefined || attr(c, 'w:chapSep') !== undefined) xml.push(raw(c));
        break;
      }
      case 'w:vAlign': {
        const v = val(c);
        if (v === 'top' || v === 'center' || v === 'both' || v === 'bottom') props.vAlign = v;
        break;
      }
      case 'w:bidi':
        props.bidi = parseOnOff(val(c));
        break;
      case 'w:docGrid': {
        const grid: NonNullable<DocxSectionProps['docGrid']> = {};
        const type = attr(c, 'w:type');
        const pitch = parseDecimal(attr(c, 'w:linePitch'));
        const cs = parseDecimal(attr(c, 'w:charSpace'));
        if (type) grid.type = type;
        if (pitch !== undefined) grid.linePitch = pitch;
        if (cs !== undefined) grid.charSpace = cs;
        props.docGrid = grid;
        break;
      }
      case 'w:lnNumType': {
        const restart = attr(c, 'w:restart');
        const start = parseDecimal(attr(c, 'w:start'));
        const distance = parseTwips(attr(c, 'w:distance'));
        props.lineNumbering = {
          countBy: parseDecimal(attr(c, 'w:countBy')) ?? 1,
          restart: restart === 'newSection' || restart === 'continuous' ? restart : 'newPage',
          ...(start !== undefined ? { start } : {}),
          ...(distance !== undefined ? { distance } : {}),
        };
        break;
      }
      case 'w:pgBorders':
        props.pageBorders = parsePageBorders(c);
        break;
      case 'w:footnotePr': {
        const p = parseNoteProps(c);
        if (p) props.footnotePr = p;
        break;
      }
      case 'w:endnotePr': {
        const p = parseNoteProps(c);
        if (p) props.endnotePr = p;
        break;
      }
      case 'w:textDirection': {
        const v = val(c);
        if (v) props.textDirection = v;
        break;
      }
      case 'w:rtlGutter':
        props.rtlGutter = parseOnOff(val(c));
        break;
      case 'w:formProt':
        props.formProt = parseOnOff(val(c));
        break;
      case 'w:noEndnote':
        props.noEndnote = parseOnOff(val(c));
        break;
      case 'w:sectPrChange':
        props.sectPrChange = raw(c);
        break;
      default:
        // w:paperSrc, w:printerSettings, …
        xml.push(raw(c));
    }
  }
  if (out.headers.length) props.headerRefs = Object.fromEntries(out.headers);
  if (out.footers.length) props.footerRefs = Object.fromEntries(out.footers);
  if (xml.length) props.xml = xml;
  return out;
}
