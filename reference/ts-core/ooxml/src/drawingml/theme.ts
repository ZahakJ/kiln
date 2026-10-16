/**
 * DrawingML themes, `a:theme` (ECMA-376 Part 1 §20.1.6): colour scheme, font scheme and format scheme
 * (style matrix). Unmodelled children (`a:objectDefaults`, `a:extraClrSchemeLst`, `a:custClrLst`,
 * `a:extLst`, scene/3-D properties inside effect styles) are preserved for round-tripping.
 */
import { ST_ColorSchemeIndex } from '../st.ts';
import { NS } from '../xml/namespaces.ts';
import { cloneNode, el, type XmlDocument, type XmlElement } from '../xml/node.ts';
import { parseXml } from '../xml/parse.ts';
import { attr, child, children } from '../xml/query.ts';
import {
  type ColorContext,
  type DrawingColor,
  parseColorChild,
  type Rgba,
  resolveColor,
  schemeColor,
  serializeColor,
  srgb,
} from './color.ts';
import { type EffectList, parseEffectList, serializeEffectList } from './effects.ts';
import { type Fill, parseFill, serializeFill } from './fill.ts';
import { type Line, parseLine, serializeLine } from './line.ts';

export interface ThemeFont {
  typeface: string;
  panose?: string;
  pitchFamily?: string;
  charset?: string;
}

export interface FontCollection {
  latin: ThemeFont;
  ea: ThemeFont;
  cs: ThemeFont;
  /** Per-script fonts (`<a:font script="Jpan" typeface="游ゴシック Light"/>`). */
  scripts: { script: string; typeface: string }[];
  extLst?: XmlElement;
}

export interface EffectStyle {
  effects?: EffectList;
  /** `a:effectDag`, `a:scene3d`, `a:sp3d`, preserved. */
  extras: XmlElement[];
}

export interface Theme {
  name: string;
  colorScheme: { name: string; colors: Record<ST_ColorSchemeIndex, DrawingColor> };
  fontScheme: { name: string; major: FontCollection; minor: FontCollection };
  formatScheme: {
    name: string;
    fillStyles: Fill[];
    lineStyles: Line[];
    effectStyles: EffectStyle[];
    bgFillStyles: Fill[];
  };
  /** Children of `a:theme` after `a:themeElements`, preserved verbatim. */
  extras: XmlElement[];
  /** Children of `a:themeElements` after `a:fmtScheme` (e.g. `a:extLst`). */
  elementExtras: XmlElement[];
}

export class ThemeError extends Error {
  override name = 'ThemeError';
}

function parseThemeFont(node: XmlElement | undefined): ThemeFont {
  const f: ThemeFont = { typeface: attr(node, 'typeface') ?? '' };
  const panose = attr(node, 'panose');
  const pf = attr(node, 'pitchFamily');
  const cs = attr(node, 'charset');
  if (panose) f.panose = panose;
  if (pf) f.pitchFamily = pf;
  if (cs) f.charset = cs;
  return f;
}

function parseFontCollection(node: XmlElement | undefined): FontCollection {
  const fc: FontCollection = {
    latin: parseThemeFont(child(node, 'a:latin')),
    ea: parseThemeFont(child(node, 'a:ea')),
    cs: parseThemeFont(child(node, 'a:cs')),
    scripts: children(node, 'a:font').map((f) => ({
      script: attr(f, 'script') ?? '',
      typeface: attr(f, 'typeface') ?? '',
    })),
  };
  const ext = child(node, 'a:extLst');
  if (ext) fc.extLst = cloneNode(ext);
  return fc;
}

/** Parses a theme part (bytes, XML text or an already-parsed `a:theme` element). */
export function parseTheme(input: Uint8Array | string | XmlElement): Theme {
  const root = typeof input === 'object' && 'type' in input ? input : parseXml(input).root;
  if (root.name !== 'a:theme') throw new ThemeError(`expected a:theme, found ${root.name}`);
  const te = child(root, 'a:themeElements');
  const cs = child(te, 'a:clrScheme');
  const colors = {} as Record<ST_ColorSchemeIndex, DrawingColor>;
  for (const slot of ST_ColorSchemeIndex) {
    colors[slot] =
      parseColorChild(child(cs, `a:${slot}`)) ?? srgb(slot === 'lt1' || slot === 'lt2' ? 'FFFFFF' : '000000');
  }
  const fs = child(te, 'a:fontScheme');
  const fmt = child(te, 'a:fmtScheme');
  const fills = (list: XmlElement | undefined) =>
    children(list)
      .map((c) => parseFill(c))
      .filter((f): f is Fill => f !== undefined);
  const known = new Set(['a:clrScheme', 'a:fontScheme', 'a:fmtScheme']);
  return {
    name: attr(root, 'name') ?? '',
    colorScheme: { name: attr(cs, 'name') ?? '', colors },
    fontScheme: {
      name: attr(fs, 'name') ?? '',
      major: parseFontCollection(child(fs, 'a:majorFont')),
      minor: parseFontCollection(child(fs, 'a:minorFont')),
    },
    formatScheme: {
      name: attr(fmt, 'name') ?? '',
      fillStyles: fills(child(fmt, 'a:fillStyleLst')),
      lineStyles: children(child(fmt, 'a:lnStyleLst'), 'a:ln').map((l) => parseLine(l) ?? {}),
      effectStyles: children(child(fmt, 'a:effectStyleLst'), 'a:effectStyle').map((es) => {
        const s: EffectStyle = {
          extras: children(es)
            .filter((c) => c.name !== 'a:effectLst')
            .map((c) => cloneNode(c)),
        };
        const fx = parseEffectList(child(es, 'a:effectLst'));
        if (fx) s.effects = fx;
        return s;
      }),
      bgFillStyles: fills(child(fmt, 'a:bgFillStyleLst')),
    },
    extras: children(root)
      .filter((c) => c.name !== 'a:themeElements')
      .map((c) => cloneNode(c)),
    elementExtras: children(te)
      .filter((c) => !known.has(c.name))
      .map((c) => cloneNode(c)),
  };
}

function serializeFontCollection(name: string, fc: FontCollection): XmlElement {
  const font = (n: string, f: ThemeFont) =>
    el(n, { typeface: f.typeface, panose: f.panose, pitchFamily: f.pitchFamily, charset: f.charset });
  return el(
    name,
    {},
    font('a:latin', fc.latin),
    font('a:ea', fc.ea),
    font('a:cs', fc.cs),
    fc.scripts.map((s) => el('a:font', { script: s.script, typeface: s.typeface })),
    fc.extLst && cloneNode(fc.extLst),
  );
}

/** Serialises a theme as an `a:theme` document. */
export function serializeTheme(theme: Theme): XmlDocument {
  const f = theme.formatScheme;
  const root = el(
    'a:theme',
    { 'xmlns:a': NS.a, name: theme.name },
    el(
      'a:themeElements',
      {},
      el(
        'a:clrScheme',
        { name: theme.colorScheme.name },
        ST_ColorSchemeIndex.map((slot) =>
          el(`a:${slot}`, {}, serializeColor(theme.colorScheme.colors[slot])),
        ),
      ),
      el(
        'a:fontScheme',
        { name: theme.fontScheme.name },
        serializeFontCollection('a:majorFont', theme.fontScheme.major),
        serializeFontCollection('a:minorFont', theme.fontScheme.minor),
      ),
      el(
        'a:fmtScheme',
        { name: f.name },
        el(
          'a:fillStyleLst',
          {},
          f.fillStyles.map((x) => serializeFill(x)),
        ),
        el(
          'a:lnStyleLst',
          {},
          f.lineStyles.map((l) => serializeLine(l)),
        ),
        el(
          'a:effectStyleLst',
          {},
          f.effectStyles.map((s) =>
            el(
              'a:effectStyle',
              {},
              serializeEffectList(s.effects ?? {}),
              s.extras.map((x) => cloneNode(x)),
            ),
          ),
        ),
        el(
          'a:bgFillStyleLst',
          {},
          f.bgFillStyles.map((x) => serializeFill(x)),
        ),
      ),
      theme.elementExtras.map((x) => cloneNode(x)),
    ),
    theme.extras.length
      ? theme.extras.map((x) => cloneNode(x))
      : [el('a:objectDefaults'), el('a:extraClrSchemeLst')],
  );
  return { declaration: { version: '1.0', encoding: 'UTF-8', standalone: 'yes' }, prolog: [], root };
}

/** Theme colour slots resolved to RGBA, for `ColorContext.themeColors`. */
export function themeColorTable(theme: Theme): Record<ST_ColorSchemeIndex, Rgba> {
  const out = {} as Record<ST_ColorSchemeIndex, Rgba>;
  for (const slot of ST_ColorSchemeIndex) out[slot] = resolveColor(theme.colorScheme.colors[slot]);
  return out;
}

/** A colour context for a theme, optionally with a colour map (`p:clrMap`) and placeholder colour. */
export function themeColorContext(
  theme: Theme,
  colorMap?: Partial<Record<string, string>>,
  phClr?: Rgba,
): ColorContext {
  const ctx: ColorContext = { themeColors: themeColorTable(theme) };
  if (colorMap) ctx.colorMap = colorMap;
  if (phClr) ctx.phClr = phClr;
  return ctx;
}

/**
 * Resolves a theme font reference: `+mj-lt`, `+mn-ea`, `+mj-cs` (DrawingML) or `majorHAnsi`,
 * `minorEastAsia`, `minorBidi` (WordprocessingML `w:asciiTheme` values). Other names pass through.
 */
export function resolveThemeFont(name: string, theme: Theme): string {
  const m = /^\+(mj|mn)-(lt|ea|cs)$/.exec(name);
  if (m) {
    const coll = m[1] === 'mj' ? theme.fontScheme.major : theme.fontScheme.minor;
    return (m[2] === 'lt' ? coll.latin : m[2] === 'ea' ? coll.ea : coll.cs).typeface;
  }
  const w = /^(major|minor)(Ascii|HAnsi|EastAsia|Bidi)$/.exec(name);
  if (w) {
    const coll = w[1] === 'major' ? theme.fontScheme.major : theme.fontScheme.minor;
    return (w[2] === 'EastAsia' ? coll.ea : w[2] === 'Bidi' ? coll.cs : coll.latin).typeface;
  }
  return name;
}

const phClr = (...t: { name: string; val?: number }[]): DrawingColor => ({
  kind: 'scheme',
  scheme: 'phClr',
  transforms: t,
});

/**
 * The Office theme of Office 2013–2022 ("Office Theme": Calibri Light / Calibri, the 4472C4 accent row),
 * which Word 2016–2021 documents and our new documents use (§3.3).
 */
export function officeTheme(): Theme {
  const sys = (sysName: string, last: string): DrawingColor => ({
    kind: 'sys',
    sys: sysName,
    lastClr: last,
    transforms: [],
  });
  const fonts = (latin: string): FontCollection => ({
    latin: {
      typeface: latin,
      panose: latin === 'Calibri Light' ? '020F0302020204030204' : '020F0502020204030204',
    },
    ea: { typeface: '' },
    cs: { typeface: '' },
    scripts: [
      { script: 'Jpan', typeface: latin === 'Calibri Light' ? '游ゴシック Light' : '游明朝' },
      { script: 'Hang', typeface: '맑은 고딕' },
      { script: 'Hans', typeface: '等线 Light' },
      { script: 'Hant', typeface: '新細明體' },
      { script: 'Arab', typeface: 'Times New Roman' },
      { script: 'Hebr', typeface: 'Times New Roman' },
      { script: 'Thai', typeface: 'Angsana New' },
      { script: 'Ethi', typeface: 'Nyala' },
      { script: 'Beng', typeface: 'Vrinda' },
      { script: 'Gujr', typeface: 'Shruti' },
      { script: 'Khmr', typeface: 'MoolBoran' },
      { script: 'Knda', typeface: 'Tunga' },
      { script: 'Guru', typeface: 'Raavi' },
      { script: 'Cans', typeface: 'Euphemia' },
      { script: 'Cher', typeface: 'Plantagenet Cherokee' },
      { script: 'Yiii', typeface: 'Microsoft Yi Baiti' },
      { script: 'Tibt', typeface: 'Microsoft Himalaya' },
      { script: 'Thaa', typeface: 'MV Boli' },
      { script: 'Deva', typeface: 'Mangal' },
      { script: 'Telu', typeface: 'Gautami' },
      { script: 'Taml', typeface: 'Latha' },
      { script: 'Syrc', typeface: 'Estrangelo Edessa' },
      { script: 'Orya', typeface: 'Kalinga' },
      { script: 'Mlym', typeface: 'Kartika' },
      { script: 'Laoo', typeface: 'DokChampa' },
      { script: 'Sinh', typeface: 'Iskoola Pota' },
      { script: 'Mong', typeface: 'Mongolian Baiti' },
      { script: 'Viet', typeface: 'Times New Roman' },
      { script: 'Uigh', typeface: 'Microsoft Uighur' },
      { script: 'Geor', typeface: 'Sylfaen' },
    ],
  });
  const grad = (stops: [number, DrawingColor][]): Fill => ({
    type: 'gradient',
    stops: stops.map(([pos, color]) => ({ pos, color })),
    linear: { angle: 90, scaled: false },
    rotWithShape: true,
  });
  const line = (w: number): Line => ({
    width: w,
    cap: 'flat',
    compound: 'sng',
    align: 'ctr',
    fill: { type: 'solid', color: phClr() },
    dash: 'solid',
    join: { type: 'miter', limit: 8 },
  });
  return {
    name: 'Office Theme',
    colorScheme: {
      name: 'Office',
      colors: {
        dk1: sys('windowText', '000000'),
        lt1: sys('window', 'FFFFFF'),
        dk2: srgb('44546A'),
        lt2: srgb('E7E6E6'),
        accent1: srgb('4472C4'),
        accent2: srgb('ED7D31'),
        accent3: srgb('A5A5A5'),
        accent4: srgb('FFC000'),
        accent5: srgb('5B9BD5'),
        accent6: srgb('70AD47'),
        hlink: srgb('0563C1'),
        folHlink: srgb('954F72'),
      },
    },
    fontScheme: { name: 'Office', major: fonts('Calibri Light'), minor: fonts('Calibri') },
    formatScheme: {
      name: 'Office',
      fillStyles: [
        { type: 'solid', color: phClr() },
        grad([
          [
            0,
            phClr(
              { name: 'lumMod', val: 110000 },
              { name: 'satMod', val: 105000 },
              { name: 'tint', val: 67000 },
            ),
          ],
          [
            0.5,
            phClr(
              { name: 'lumMod', val: 105000 },
              { name: 'satMod', val: 103000 },
              { name: 'tint', val: 73000 },
            ),
          ],
          [
            1,
            phClr(
              { name: 'lumMod', val: 105000 },
              { name: 'satMod', val: 109000 },
              { name: 'tint', val: 81000 },
            ),
          ],
        ]),
        grad([
          [
            0,
            phClr(
              { name: 'satMod', val: 103000 },
              { name: 'lumMod', val: 102000 },
              { name: 'tint', val: 94000 },
            ),
          ],
          [
            0.5,
            phClr(
              { name: 'satMod', val: 110000 },
              { name: 'lumMod', val: 100000 },
              { name: 'shade', val: 100000 },
            ),
          ],
          [
            1,
            phClr(
              { name: 'lumMod', val: 99000 },
              { name: 'satMod', val: 120000 },
              { name: 'shade', val: 78000 },
            ),
          ],
        ]),
      ],
      lineStyles: [line(6350), line(12700), line(19050)],
      effectStyles: [
        { effects: {}, extras: [] },
        { effects: {}, extras: [] },
        {
          effects: {
            outerShadow: {
              blurRad: 57150,
              dist: 19050,
              dir: 90,
              sx: 1,
              sy: 1,
              kx: 0,
              ky: 0,
              algn: 'ctr',
              rotWithShape: false,
              color: srgb('000000', { name: 'alpha', val: 63000 }),
            },
          },
          extras: [],
        },
      ],
      bgFillStyles: [
        { type: 'solid', color: phClr() },
        { type: 'solid', color: phClr({ name: 'tint', val: 95000 }, { name: 'satMod', val: 170000 }) },
        grad([
          [
            0,
            phClr(
              { name: 'tint', val: 93000 },
              { name: 'satMod', val: 150000 },
              { name: 'shade', val: 98000 },
              { name: 'lumMod', val: 102000 },
            ),
          ],
          [
            0.5,
            phClr(
              { name: 'tint', val: 98000 },
              { name: 'satMod', val: 130000 },
              { name: 'shade', val: 90000 },
              { name: 'lumMod', val: 103000 },
            ),
          ],
          [1, phClr({ name: 'shade', val: 63000 }, { name: 'satMod', val: 120000 })],
        ]),
      ],
    },
    extras: [],
    elementExtras: [],
  };
}

/** Theme colour used for a WordprocessingML/SpreadsheetML theme index (`w:themeColor`, `<color theme="4">`). */
export function themeColorForIndex(index: number): DrawingColor {
  // SpreadsheetML theme indexes follow clrScheme order but swap the dark/light pairs (0 = lt1, 1 = dk1).
  const order = [
    'lt1',
    'dk1',
    'lt2',
    'dk2',
    'accent1',
    'accent2',
    'accent3',
    'accent4',
    'accent5',
    'accent6',
    'hlink',
    'folHlink',
  ];
  return schemeColor((order[index] ?? 'dk1') as ST_ColorSchemeIndex);
}
