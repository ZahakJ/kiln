/**
 * DrawingML for WordprocessingML (`w:drawing`): pictures (`pic:pic`) and shapes/text boxes (`wps:wsp`)
 * placed inline (`wp:inline`) or floating (`wp:anchor` with position and wrapping).
 *
 * Shapes read from a file keep their original XML; the writer re-emits it and patches only the
 * properties the model changed (size, position and wrapping, preset geometry, fill, outline, rotation),
 * so effects, text box content and Office extensions survive editing. A changed shape that was wrapped
 * in `mc:AlternateContent` loses its (now stale) VML fallback.
 */
import type { AnchorPosition, WrapType } from '@officina/doc-model';
import {
  buildXml,
  child,
  childPath,
  children,
  type Fill,
  findFill,
  firstElement,
  type Line,
  parseLine,
  serializeFill,
  serializeLine,
  type XmlElement,
  type XmlNode,
  text as xmlText,
} from '@officina/ooxml';
import { parseDecimal, parseEmu, parseOnOff } from '../common/units.ts';
import { parseFragment } from '../common/xml.ts';
import { attrs, tag } from './xml.ts';

export const GRAPHIC_URI = {
  picture: 'http://schemas.openxmlformats.org/drawingml/2006/picture',
  shape: 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
} as const;

const DECORATIVE_EXT = '{C183D7F6-B498-43B3-948B-1728B52AA6E4}';
const DECORATIVE_NS = 'http://schemas.microsoft.com/office/drawing/2017/decorative';

export interface Placement {
  placement: 'inline' | 'anchor';
  wrap: WrapType;
  position: AnchorPosition | null;
}

export interface ContainerInput extends Placement {
  /** Extent in EMU. */
  width: number;
  height: number;
  docPr: string;
  graphic: string;
  /** Raw `wp:effectExtent` to keep (shapes with shadows); a zero extent otherwise. */
  effectExtent?: string;
  /** Raw wrap polygon / other anchor children to keep when patching. */
  keepWrap?: string;
}

const emu = (n: number) => Math.round(n);

function axisXml(name: string, rel: string, axis: AnchorPosition['h'] | AnchorPosition['v']): string {
  const inner =
    'align' in axis
      ? `<wp:align>${axis.align}</wp:align>`
      : `<wp:posOffset>${emu(axis.offset)}</wp:posOffset>`;
  return `<${name}${attrs([['relativeFrom', rel]])}>${inner}</${name}>`;
}

/** A tight/through wrap needs a polygon; this one is the object's bounding box (Word's default). */
const DEFAULT_POLYGON =
  '<wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon>';

function wrapXml(wrap: WrapType, keep?: string): string {
  if (keep) return keep;
  switch (wrap) {
    case 'square':
      return '<wp:wrapSquare wrapText="bothSides"/>';
    case 'tight':
      return `<wp:wrapTight wrapText="bothSides">${DEFAULT_POLYGON}</wp:wrapTight>`;
    case 'through':
      return `<wp:wrapThrough wrapText="bothSides">${DEFAULT_POLYGON}</wp:wrapThrough>`;
    case 'topAndBottom':
      return '<wp:wrapTopAndBottom/>';
    default:
      return '<wp:wrapNone/>';
  }
}

const DEFAULT_POSITION: AnchorPosition = {
  relativeFromH: 'column',
  relativeFromV: 'paragraph',
  h: { offset: 0 },
  v: { offset: 0 },
};

/** `wp:inline` or `wp:anchor` around a graphic. */
export function containerXml(c: ContainerInput, zIndexFallback: number): string {
  const extent = `<wp:extent cx="${emu(c.width)}" cy="${emu(c.height)}"/>`;
  const effect = c.effectExtent ?? '<wp:effectExtent l="0" t="0" r="0" b="0"/>';
  const frame = '<wp:cNvGraphicFramePr/>';
  if (c.placement !== 'anchor' || c.wrap === 'inline') {
    return `<wp:inline distT="0" distB="0" distL="0" distR="0">${extent}${effect}${c.docPr}${frame}${c.graphic}</wp:inline>`;
  }
  const p = c.position ?? DEFAULT_POSITION;
  const behind = c.wrap === 'behind';
  const anchorAttrs = attrs([
    ['distT', p.distT],
    ['distB', p.distB],
    ['distL', p.distL],
    ['distR', p.distR],
    ['simplePos', '0'],
    ['relativeHeight', p.zIndex ?? zIndexFallback],
    ['behindDoc', behind ? '1' : '0'],
    ['locked', '0'],
    ['layoutInCell', p.layoutInCell === false ? '0' : '1'],
    ['allowOverlap', p.allowOverlap === false ? '0' : '1'],
  ]);
  return (
    `<wp:anchor${anchorAttrs}><wp:simplePos x="0" y="0"/>` +
    axisXml('wp:positionH', p.relativeFromH, p.h) +
    axisXml('wp:positionV', p.relativeFromV, p.v) +
    extent +
    effect +
    wrapXml(c.wrap, c.keepWrap) +
    c.docPr +
    frame +
    c.graphic +
    '</wp:anchor>'
  );
}

export interface DocPrInput {
  id: number;
  name: string;
  alt?: string;
  title?: string;
  decorative?: boolean;
}

export function docPrXml(d: DocPrInput): string {
  const ext = d.decorative
    ? `<a:extLst><a:ext uri="${DECORATIVE_EXT}"><adec:decorative xmlns:adec="${DECORATIVE_NS}" val="1"/></a:ext></a:extLst>`
    : '';
  return tag(
    'wp:docPr',
    [
      ['id', d.id],
      ['name', d.name],
      ['descr', d.alt || undefined],
      ['title', d.title || undefined],
    ],
    ext,
  );
}

export interface PictureInput {
  relId: string;
  docPrId: number;
  name: string;
  width: number;
  height: number;
  crop: { l: number; t: number; r: number; b: number } | null;
  rotation: number;
  flipH: boolean;
  flipV: boolean;
}

const pct = (f: number) => Math.round(f * 100000);

/** `a:graphic` holding a picture. */
export function pictureGraphicXml(p: PictureInput): string {
  const crop = p.crop
    ? tag('a:srcRect', [
        ['l', p.crop.l ? pct(p.crop.l) : undefined],
        ['t', p.crop.t ? pct(p.crop.t) : undefined],
        ['r', p.crop.r ? pct(p.crop.r) : undefined],
        ['b', p.crop.b ? pct(p.crop.b) : undefined],
      ])
    : '';
  const xfrm = tag(
    'a:xfrm',
    [
      ['rot', p.rotation ? Math.round(p.rotation * 60000) : undefined],
      ['flipH', p.flipH ? '1' : undefined],
      ['flipV', p.flipV ? '1' : undefined],
    ],
    `<a:off x="0" y="0"/><a:ext cx="${emu(p.width)}" cy="${emu(p.height)}"/>`,
  );
  return (
    `<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="${GRAPHIC_URI.picture}">` +
    `<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">` +
    `<pic:nvPicPr>${tag('pic:cNvPr', [
      ['id', p.docPrId],
      ['name', p.name],
    ])}<pic:cNvPicPr><a:picLocks noChangeAspect="1" noChangeArrowheads="1"/></pic:cNvPicPr></pic:nvPicPr>` +
    `<pic:blipFill>${tag('a:blip', [['r:embed', p.relId]])}${crop}<a:stretch><a:fillRect/></a:stretch></pic:blipFill>` +
    `<pic:spPr bwMode="auto">${xfrm}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>` +
    '</pic:pic></a:graphicData></a:graphic>'
  );
}

const frag = (e: XmlElement) => buildXml(e, { declaration: false, declareNamespaces: false });

/** `a:graphic` holding a new shape (`wps:wsp`) built from the model. */
export function shapeGraphicXml(s: {
  docPrId: number;
  preset: string;
  width: number;
  height: number;
  rotation: number;
  fill: Fill | null;
  line: Line | null;
}): string {
  const xfrm = tag(
    'a:xfrm',
    [['rot', s.rotation ? Math.round(s.rotation * 60000) : undefined]],
    `<a:off x="0" y="0"/><a:ext cx="${emu(s.width)}" cy="${emu(s.height)}"/>`,
  );
  const fill = s.fill ? frag(serializeFill(s.fill)) : '';
  const line = s.line ? frag(serializeLine(s.line)) : '';
  return (
    `<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="${GRAPHIC_URI.shape}">` +
    `<wps:wsp><wps:cNvSpPr/><wps:spPr>${xfrm}${tag('a:prstGeom', [['prst', s.preset]], '<a:avLst/>')}${fill}${line}</wps:spPr>` +
    '<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="t" anchorCtr="0"><a:noAutofit/></wps:bodyPr>' +
    '</wps:wsp></a:graphicData></a:graphic>'
  );
}

// ------------------------------------------------------------------ patching kept shapes

const WRAP_ELEMENTS: Record<string, WrapType> = {
  'wp:wrapSquare': 'square',
  'wp:wrapTight': 'tight',
  'wp:wrapThrough': 'through',
  'wp:wrapTopAndBottom': 'topAndBottom',
};

function axisOf(
  node: XmlElement | undefined,
  aligns: readonly string[],
): { offset: number } | { align: string } {
  const align = child(node, 'wp:align');
  if (align) {
    const a = xmlText(align).trim();
    if (aligns.includes(a)) return { align: a };
  }
  const off = child(node, 'wp:posOffset');
  return { offset: off ? (parseEmu(xmlText(off)) ?? 0) : 0 };
}

/** The placement the reader derives from a `wp:inline`/`wp:anchor` (mirrors its parser). */
export function derivePlacement(container: XmlElement): Placement {
  if (container.name !== 'wp:anchor') return { placement: 'inline', wrap: 'inline', position: null };
  const behind = parseOnOff(container.attrs.behindDoc, false);
  let wrap: WrapType = behind ? 'behind' : 'inFront';
  for (const c of children(container)) {
    const w = WRAP_ELEMENTS[c.name];
    if (w) {
      wrap = w;
      break;
    }
    if (c.name === 'wp:wrapNone') break;
  }
  const posH = child(container, 'wp:positionH');
  const posV = child(container, 'wp:positionV');
  let relH = (posH?.attrs.relativeFrom ?? 'column') as AnchorPosition['relativeFromH'];
  let relV = (posV?.attrs.relativeFrom ?? 'paragraph') as AnchorPosition['relativeFromV'];
  let h: AnchorPosition['h'];
  let v: AnchorPosition['v'];
  if (parseOnOff(container.attrs.simplePos, false)) {
    const sp = child(container, 'wp:simplePos');
    h = { offset: parseEmu(sp?.attrs.x) ?? 0 };
    v = { offset: parseEmu(sp?.attrs.y) ?? 0 };
    relH = 'page';
    relV = 'page';
  } else {
    h = axisOf(posH, ['left', 'center', 'right', 'inside', 'outside']) as AnchorPosition['h'];
    v = axisOf(posV, ['top', 'center', 'bottom', 'inside', 'outside']) as AnchorPosition['v'];
  }
  const position: AnchorPosition = { relativeFromH: relH, relativeFromV: relV, h, v };
  for (const k of ['distT', 'distB', 'distL', 'distR'] as const) {
    const n = parseEmu(container.attrs[k]);
    if (n !== undefined) position[k] = n;
  }
  if (container.attrs.allowOverlap !== undefined)
    position.allowOverlap = parseOnOff(container.attrs.allowOverlap);
  if (container.attrs.layoutInCell !== undefined)
    position.layoutInCell = parseOnOff(container.attrs.layoutInCell);
  const z = parseDecimal(container.attrs.relativeHeight);
  if (z !== undefined) position.zIndex = z;
  return { placement: 'anchor', wrap, position };
}

const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

export interface ShapeAttrs {
  preset: string;
  width: number;
  height: number;
  placement: 'inline' | 'anchor';
  wrap: WrapType;
  position: AnchorPosition | null;
  fill: Fill | null;
  line: Line | null;
  rotation: number;
}

function findDrawing(root: XmlElement): { drawing: XmlElement; viaAlternate: boolean } | undefined {
  if (root.name === 'w:drawing') return { drawing: root, viaAlternate: false };
  if (root.name === 'mc:AlternateContent') {
    for (const choice of children(root, 'mc:Choice')) {
      const d = child(choice, 'w:drawing');
      if (d) return { drawing: d, viaAlternate: true };
    }
  }
  return undefined;
}

function replaceChild(
  parent: XmlElement,
  name: string,
  replacement: XmlNode | undefined,
  before?: string[],
): void {
  const i = parent.children.findIndex((c) => c.type === 'element' && c.name === name);
  if (i >= 0) {
    if (replacement) parent.children[i] = replacement;
    else parent.children.splice(i, 1);
    return;
  }
  if (!replacement) return;
  const j = before ? parent.children.findIndex((c) => c.type === 'element' && before.includes(c.name)) : -1;
  if (j >= 0) parent.children.splice(j, 0, replacement);
  else parent.children.push(replacement);
}

const FILL_NAMES = ['a:noFill', 'a:solidFill', 'a:gradFill', 'a:pattFill', 'a:blipFill', 'a:grpFill'];

/**
 * Re-emits a kept shape, applying the model's changes. Returns the run content (`w:drawing`, or the
 * original `mc:AlternateContent` when nothing changed).
 */
export function patchShapeXml(xml: string, s: ShapeAttrs, zIndexFallback: number): string {
  let root: XmlElement;
  try {
    root = parseFragment(xml);
  } catch {
    return xml;
  }
  const found = findDrawing(root);
  if (!found) return xml;
  const { drawing } = found;
  const container = firstElement(drawing);
  if (!container || (container.name !== 'wp:inline' && container.name !== 'wp:anchor')) return xml;
  const wsp = childPath(container, 'a:graphic', 'a:graphicData', 'wps:wsp');
  const spPr = child(wsp, 'wps:spPr');
  const extent = child(container, 'wp:extent');
  const xfrm = child(spPr, 'a:xfrm');
  const derived = {
    preset: child(spPr, 'a:prstGeom')?.attrs.prst ?? 'rect',
    fill: findFill(spPr) ?? null,
    line: parseLine(child(spPr, 'a:ln')) ?? null,
    rotation: (parseDecimal(xfrm?.attrs.rot) ?? 0) / 60000,
    width: parseEmu(extent?.attrs.cx) ?? 0,
    height: parseEmu(extent?.attrs.cy) ?? 0,
    ...derivePlacement(container),
  };
  let changed = false;
  if (spPr) {
    if (derived.preset !== s.preset) {
      const geom = child(spPr, 'a:prstGeom');
      if (geom) geom.attrs.prst = s.preset;
      else {
        replaceChild(
          spPr,
          'a:custGeom',
          parseFragment(`<a:prstGeom prst="${s.preset}"><a:avLst/></a:prstGeom>`),
        );
      }
      changed = true;
    }
    if (!same(derived.fill, s.fill)) {
      for (const n of FILL_NAMES) replaceChild(spPr, n, undefined);
      if (s.fill)
        replaceChild(spPr, '#none', serializeFill(s.fill), [
          'a:ln',
          'a:effectLst',
          'a:effectDag',
          'a:scene3d',
          'a:sp3d',
          'a:extLst',
        ]);
      changed = true;
    }
    if (!same(derived.line, s.line)) {
      replaceChild(spPr, 'a:ln', s.line ? serializeLine(s.line) : undefined, [
        'a:effectLst',
        'a:effectDag',
        'a:scene3d',
        'a:sp3d',
        'a:extLst',
      ]);
      changed = true;
    }
    if (derived.rotation !== s.rotation && xfrm) {
      if (s.rotation) xfrm.attrs.rot = String(Math.round(s.rotation * 60000));
      else delete xfrm.attrs.rot;
      changed = true;
    }
    if ((derived.width !== s.width || derived.height !== s.height) && xfrm) {
      const ext = child(xfrm, 'a:ext');
      if (ext) {
        ext.attrs.cx = String(emu(s.width));
        ext.attrs.cy = String(emu(s.height));
      }
    }
  }
  const placementChanged =
    derived.placement !== s.placement ||
    derived.wrap !== s.wrap ||
    !same(derived.position, s.position) ||
    derived.width !== s.width ||
    derived.height !== s.height;
  if (!changed && !placementChanged) return xml;
  let containerXmlOut = frag(container);
  if (placementChanged) {
    const keep = (name: string) => {
      const c = child(container, name);
      return c ? frag(c) : undefined;
    };
    const wrapEl = children(container).find((c) => WRAP_ELEMENTS[c.name] || c.name === 'wp:wrapNone');
    const keepWrap =
      wrapEl && derived.wrap === s.wrap && WRAP_ELEMENTS[wrapEl.name] ? frag(wrapEl) : undefined;
    const graphic = child(container, 'a:graphic');
    containerXmlOut = containerXml(
      {
        placement: s.placement,
        wrap: s.wrap,
        position: s.position,
        width: s.width,
        height: s.height,
        docPr: keep('wp:docPr') ?? '<wp:docPr id="1" name="Shape"/>',
        graphic: graphic ? frag(graphic) : '',
        ...(keep('wp:effectExtent') ? { effectExtent: keep('wp:effectExtent') as string } : {}),
        ...(keepWrap ? { keepWrap } : {}),
      },
      zIndexFallback,
    );
  }
  return `<w:drawing>${containerXmlOut}</w:drawing>`;
}
