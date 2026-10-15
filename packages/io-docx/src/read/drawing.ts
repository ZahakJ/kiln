/**
 * Drawings: DrawingML (`w:drawing` with `wp:inline`/`wp:anchor`), legacy VML (`w:pict`) and embedded
 * objects (`w:object`).
 *
 * - `pic:pic` → `image` (media extracted to the result's `media` map, keyed by part name);
 * - `wps:wsp` (shapes and text boxes) → `shape` with preset geometry, fill and line parsed through
 *   `@officina/ooxml` DrawingML, and the original XML kept for everything else (text box content,
 *   effects, custom geometry);
 * - charts → an `opaqueInline` placeholder plus a `DocxChartRef` for the charts package;
 * - SmartArt, groups, canvases, ink, OLE objects and VML other than plain pictures → `opaqueInline`
 *   placeholders that keep the XML (reported as `info` warnings).
 */
import type { AnchorPosition, WrapType } from '@officina/doc-model';
import {
  child,
  childPath,
  children,
  descendants,
  findFill,
  firstElement,
  parseLine,
  type XmlElement,
  text as xmlText,
} from '@officina/ooxml';
import type { Mark, Node as PMNode } from '@tiptap/pm/model';
import { parseDecimal, parseEmu, parseOnOff } from '../common/units.ts';
import { attr } from '../common/xml.ts';
import type { StoryReader } from './story.ts';

const URI = {
  picture: 'http://schemas.openxmlformats.org/drawingml/2006/picture',
  shape: 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
  group: 'http://schemas.microsoft.com/office/word/2010/wordprocessingGroup',
  canvas: 'http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas',
  ink: 'http://schemas.microsoft.com/office/word/2010/wordprocessingInk',
  chart: 'http://schemas.openxmlformats.org/drawingml/2006/chart',
  chartex: 'http://schemas.microsoft.com/office/drawing/2014/chartex',
  diagram: 'http://schemas.openxmlformats.org/drawingml/2006/diagram',
  lockedCanvas: 'http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas',
} as const;

const localName = (n: string) => n.slice(n.indexOf(':') + 1);

/** Records every relationship id referenced from a raw XML fragment, so the writer can map it. */
export function recordRelationshipRefs(story: StoryReader, el: XmlElement): void {
  const walk = (n: XmlElement) => {
    for (const [k, v] of Object.entries(n.attrs)) {
      if (k.startsWith('r:') && v) story.ctx.relationship(story.partName, v);
    }
    for (const c of n.children) if (c.type === 'element') walk(c);
  };
  walk(el);
}

function placeholder(
  story: StoryReader,
  xmlEl: XmlElement,
  label: string,
  marks: readonly Mark[],
): PMNode | undefined {
  recordRelationshipRefs(story, xmlEl);
  return story.schema.nodes.opaqueInline?.create({ xml: story.raw(xmlEl), label }, null, marks);
}

interface Placement {
  placement: 'inline' | 'anchor';
  wrap: WrapType;
  position: AnchorPosition | null;
}

const WRAPS: Record<string, WrapType> = {
  'wp:wrapSquare': 'square',
  'wp:wrapTight': 'tight',
  'wp:wrapThrough': 'through',
  'wp:wrapTopAndBottom': 'topAndBottom',
};

function parseAxis(
  story: StoryReader,
  node: XmlElement | undefined,
  aligns: readonly string[],
): { offset: number } | { align: string } {
  const align = child(node, 'wp:align');
  if (align) {
    const a = xmlText(align).trim();
    if (aligns.includes(a)) return { align: a };
  }
  const off = child(node, 'wp:posOffset');
  if (off) return { offset: parseEmu(xmlText(off)) ?? 0 };
  const pct = children(node).find((c) => localName(c.name).startsWith('pctPos'));
  if (pct) {
    story.warn(
      'drawing.unsupported',
      'warning',
      'A position given as a percentage of the page was approximated as an offset of zero.',
    );
  }
  return { offset: 0 };
}

function parseAnchor(story: StoryReader, anchor: XmlElement): Placement {
  const behind = parseOnOff(attr(anchor, 'behindDoc'), false);
  let wrap: WrapType = behind ? 'behind' : 'inFront';
  for (const c of children(anchor)) {
    const w = WRAPS[c.name];
    if (w) {
      wrap = w;
      const side = attr(c, 'wrapText');
      if (side && side !== 'bothSides') {
        story.warn(
          'drawing.unsupported',
          'warning',
          `Text wrapping on one side only (${side}) is shown as wrapping on both sides.`,
        );
      }
      break;
    }
    if (c.name === 'wp:wrapNone') {
      wrap = behind ? 'behind' : 'inFront';
      break;
    }
  }
  const posH = child(anchor, 'wp:positionH');
  const posV = child(anchor, 'wp:positionV');
  let h: AnchorPosition['h'];
  let v: AnchorPosition['v'];
  let relH = (attr(posH, 'relativeFrom') ?? 'column') as AnchorPosition['relativeFromH'];
  let relV = (attr(posV, 'relativeFrom') ?? 'paragraph') as AnchorPosition['relativeFromV'];
  if (parseOnOff(attr(anchor, 'simplePos'), false)) {
    const sp = child(anchor, 'wp:simplePos');
    h = { offset: parseEmu(attr(sp, 'x')) ?? 0 };
    v = { offset: parseEmu(attr(sp, 'y')) ?? 0 };
    relH = 'page';
    relV = 'page';
  } else {
    h = parseAxis(story, posH, ['left', 'center', 'right', 'inside', 'outside']) as AnchorPosition['h'];
    v = parseAxis(story, posV, ['top', 'center', 'bottom', 'inside', 'outside']) as AnchorPosition['v'];
  }
  const position: AnchorPosition = { relativeFromH: relH, relativeFromV: relV, h, v };
  for (const k of ['distT', 'distB', 'distL', 'distR'] as const) {
    const n = parseEmu(attr(anchor, k));
    if (n !== undefined) position[k] = n;
  }
  if (attr(anchor, 'allowOverlap') !== undefined)
    position.allowOverlap = parseOnOff(attr(anchor, 'allowOverlap'));
  if (attr(anchor, 'layoutInCell') !== undefined)
    position.layoutInCell = parseOnOff(attr(anchor, 'layoutInCell'));
  const z = parseDecimal(attr(anchor, 'relativeHeight'));
  if (z !== undefined) position.zIndex = z;
  return { placement: 'anchor', wrap, position };
}

function transform(xfrm: XmlElement | undefined): { rotation: number; flipH: boolean; flipV: boolean } {
  return {
    rotation: (parseDecimal(attr(xfrm, 'rot')) ?? 0) / 60000,
    flipH: parseOnOff(attr(xfrm, 'flipH'), false),
    flipV: parseOnOff(attr(xfrm, 'flipV'), false),
  };
}

/** Loads an image part into the media map; returns its part name (the image's `mediaId`). */
function loadMedia(story: StoryReader, relId: string): string | undefined {
  const rel = story.ctx.relationship(story.partName, relId);
  if (!rel || rel.targetMode === 'External' || !rel.part) return undefined;
  const data = story.ctx.pkg.getPart(rel.part);
  if (!data) return undefined;
  story.ctx.media.set(rel.part, {
    contentType: story.ctx.pkg.getContentType(rel.part) ?? 'application/octet-stream',
    data,
  });
  story.ctx.consume(rel.part);
  return rel.part;
}

function readPicture(
  story: StoryReader,
  pic: XmlElement,
  base: Record<string, unknown>,
  marks: readonly Mark[],
): PMNode | undefined {
  const blipFill = child(pic, 'pic:blipFill');
  const blip = child(blipFill, 'a:blip');
  const embed = attr(blip, 'r:embed');
  const link = attr(blip, 'r:link');
  let mediaId: string | undefined;
  if (embed) {
    mediaId = loadMedia(story, embed);
    if (!mediaId)
      story.warn('image.missing', 'warning', 'A picture refers to an image that is missing from the file.');
  } else if (link) {
    story.ctx.relationship(story.partName, link);
    story.warn(
      'image.linked',
      'warning',
      'A picture linked to an external file is not loaded; save the picture into the document to keep it.',
    );
  }
  if (descendants(blip, 'a:ext').some((e) => children(e).some((c) => localName(c.name) === 'svgBlip'))) {
    story.warn('drawing.unsupported', 'warning', 'An SVG picture is shown and saved using its PNG fallback.');
  }
  const src = child(blipFill, 'a:srcRect');
  let crop: { l: number; t: number; r: number; b: number } | null = null;
  if (src) {
    const f = (k: string) => (parseDecimal(attr(src, k)) ?? 0) / 100000;
    crop = { l: f('l'), t: f('t'), r: f('r'), b: f('b') };
    if (!crop.l && !crop.t && !crop.r && !crop.b) crop = null;
  }
  const xf = transform(childPath(pic, 'pic:spPr', 'a:xfrm'));
  return story.schema.nodes.image?.create({ ...base, mediaId: mediaId ?? null, crop, ...xf }, null, marks);
}

function readShape(
  story: StoryReader,
  wsp: XmlElement,
  drawing: XmlElement,
  base: Record<string, unknown>,
  marks: readonly Mark[],
): PMNode | undefined {
  recordRelationshipRefs(story, drawing);
  const spPr = child(wsp, 'wps:spPr');
  const preset = attr(child(spPr, 'a:prstGeom'), 'prst') ?? 'rect';
  const fill = findFill(spPr) ?? null;
  const line = parseLine(child(spPr, 'a:ln')) ?? null;
  const { rotation } = transform(child(spPr, 'a:xfrm'));
  return story.schema.nodes.shape?.create(
    {
      preset,
      width: base.width,
      height: base.height,
      placement: base.placement,
      wrap: base.wrap,
      position: base.position,
      fill,
      line,
      rotation,
      xml: story.raw(drawing),
    },
    null,
    marks,
  );
}

/** `w:drawing` → image, shape or placeholder. */
export function readDrawing(
  story: StoryReader,
  drawing: XmlElement,
  marks: readonly Mark[],
): PMNode | undefined {
  const container = firstElement(drawing);
  if (!container || (container.name !== 'wp:inline' && container.name !== 'wp:anchor')) {
    story.warn('drawing.unsupported', 'warning', 'An empty or unknown drawing was skipped.');
    return undefined;
  }
  const extent = child(container, 'wp:extent');
  const docPr = child(container, 'wp:docPr');
  const placement: Placement =
    container.name === 'wp:anchor'
      ? parseAnchor(story, container)
      : { placement: 'inline', wrap: 'inline', position: null };
  const decorative = descendants(docPr, 'a:ext').some((e) =>
    children(e).some((c) => localName(c.name) === 'decorative' && parseOnOff(attr(c, 'val'), false)),
  );
  const base: Record<string, unknown> = {
    width: parseEmu(attr(extent, 'cx')) ?? 0,
    height: parseEmu(attr(extent, 'cy')) ?? 0,
    alt: attr(docPr, 'descr') ?? '',
    title: attr(docPr, 'title') ?? '',
    decorative,
    ...placement,
  };
  const graphicData = childPath(container, 'a:graphic', 'a:graphicData');
  const uri = attr(graphicData, 'uri') ?? '';
  switch (uri) {
    case URI.picture: {
      const pic = child(graphicData, 'pic:pic');
      if (pic) return readPicture(story, pic, base, marks);
      break;
    }
    case URI.shape: {
      const wsp = child(graphicData, 'wps:wsp');
      if (wsp) return readShape(story, wsp, drawing, base, marks);
      break;
    }
    case URI.chart:
    case URI.chartex: {
      const ref = firstElement(graphicData);
      const relId = attr(ref, 'r:id');
      if (relId) {
        const rel = story.ctx.relationship(story.partName, relId);
        story.ctx.charts.push({
          sourcePart: story.partName,
          relId,
          ...(rel?.part ? { chartPart: rel.part } : {}),
          width: base.width as number,
          height: base.height as number,
          placement: placement.placement,
          ...(attr(docPr, 'name') ? { name: attr(docPr, 'name') as string } : {}),
          ...(attr(docPr, 'descr') ? { description: attr(docPr, 'descr') as string } : {}),
        });
      }
      return placeholder(story, drawing, 'Chart', marks);
    }
    case URI.diagram:
      story.warn('drawing.smartArt', 'info', 'SmartArt is kept in the file but shown as a placeholder.');
      return placeholder(story, drawing, 'SmartArt', marks);
    case URI.group:
      story.warn(
        'drawing.group',
        'info',
        'A group of shapes is kept in the file but shown as a placeholder.',
      );
      return placeholder(story, drawing, 'Group', marks);
    case URI.canvas:
    case URI.lockedCanvas:
      story.warn(
        'drawing.canvas',
        'info',
        'A drawing canvas is kept in the file but shown as a placeholder.',
      );
      return placeholder(story, drawing, 'Drawing canvas', marks);
    case URI.ink:
      story.warn('drawing.ink', 'info', 'Ink is kept in the file but cannot be edited.');
      return placeholder(story, drawing, 'Ink', marks);
    default:
      break;
  }
  story.warn(
    'drawing.unsupported',
    'info',
    `A drawing of type ${uri || 'unknown'} is kept but shown as a placeholder.`,
  );
  return placeholder(story, drawing, 'Object', marks);
}

/** VML length (`12pt`, `1.5in`, `96px`, `2cm`) → EMU. */
function vmlLength(v: string | undefined): number | undefined {
  if (!v) return undefined;
  const m = /^\s*(-?[\d.]+)\s*(pt|in|cm|mm|px|pc|emu)?\s*$/.exec(v);
  if (!m) return undefined;
  const n = Number(m[1]);
  const unit = m[2] ?? 'px';
  const perUnit: Record<string, number> = {
    pt: 12700,
    in: 914400,
    cm: 360000,
    mm: 36000,
    px: 9525,
    pc: 152400,
    emu: 1,
  };
  return Math.round(n * (perUnit[unit] ?? 9525));
}

function parseStyle(style: string | undefined): Map<string, string> {
  const out = new Map<string, string>();
  for (const decl of (style ?? '').split(';')) {
    const i = decl.indexOf(':');
    if (i > 0) out.set(decl.slice(0, i).trim().toLowerCase(), decl.slice(i + 1).trim());
  }
  return out;
}

/** `w:pict` (VML). Plain pictures are read as images; everything else is preserved as a placeholder. */
export function readPict(story: StoryReader, pict: XmlElement, marks: readonly Mark[]): PMNode | undefined {
  const shapes = children(pict).filter((c) => c.name.startsWith('v:') && c.name !== 'v:shapetype');
  const shape = shapes.length === 1 ? shapes[0] : undefined;
  const imagedata = shape ? child(shape, 'v:imagedata') : undefined;
  const hasText = shape ? descendants(shape, 'v:textbox').length > 0 : false;
  const relId = attr(imagedata, 'r:id') ?? attr(imagedata, 'r:pict');
  if (shape && imagedata && relId && !hasText) {
    const style = parseStyle(attr(shape, 'style'));
    if (style.get('position') === 'absolute') {
      story.warn('drawing.vml', 'warning', 'A legacy (VML) floating picture is shown in line with the text.');
    }
    const mediaId = loadMedia(story, relId);
    if (!mediaId)
      story.warn('image.missing', 'warning', 'A picture refers to an image that is missing from the file.');
    return story.schema.nodes.image?.create(
      {
        mediaId: mediaId ?? null,
        width: vmlLength(style.get('width')) ?? 0,
        height: vmlLength(style.get('height')) ?? 0,
        alt: attr(shape, 'alt') ?? attr(imagedata, 'o:title') ?? '',
        title: attr(imagedata, 'o:title') ?? '',
        placement: 'inline',
        wrap: 'inline',
      },
      null,
      marks,
    );
  }
  const textbox = descendants(pict, 'w:txbxContent')[0];
  const label = textbox ? xmlText(textbox).trim().slice(0, 80) || 'Text box' : 'Drawing';
  story.warn('drawing.vml', 'info', 'A legacy (VML) drawing is kept in the file but shown as a placeholder.');
  return placeholder(story, pict, label, marks);
}

/** `w:object` (embedded or linked OLE object). */
export function readObject(story: StoryReader, obj: XmlElement, marks: readonly Mark[]): PMNode | undefined {
  const ole = descendants(obj, 'o:OLEObject')[0];
  const prog = attr(ole, 'ProgID');
  story.warn(
    'object.embedded',
    'info',
    `An embedded object${prog ? ` (${prog})` : ''} is kept in the file but shown as a placeholder.`,
  );
  return placeholder(story, obj, prog ? `Embedded ${prog}` : 'Embedded object', marks);
}
