import { describe, expect, it } from 'vitest';
import {
  applyExcelTint,
  type DrawingColor,
  hexToRgba,
  parseColor,
  parseColorChild,
  presetColorHex,
  resolveColor,
  resolveColorHex,
  rgbaToCss,
  rgbToHsl,
  schemeColor,
  serializeColor,
  srgb,
} from '../src/drawingml/color.ts';
import { outerShadowToCss, parseEffectList, serializeEffectList } from '../src/drawingml/effects.ts';
import { type Fill, fillToCss, findFill, parseFill, serializeFill } from '../src/drawingml/fill.ts';
import { dashArrayPx, parseLine, serializeLine, svgCapJoin } from '../src/drawingml/line.ts';
import {
  mergeParagraphProperties,
  mergeRunProperties,
  parseBodyProperties,
  parseListStyle,
  parseParagraphProperties,
  parseRunProperties,
  serializeBodyProperties,
  serializeListStyle,
  serializeParagraphProperties,
  serializeRunProperties,
} from '../src/drawingml/text.ts';
import {
  officeTheme,
  parseTheme,
  resolveThemeFont,
  serializeTheme,
  themeColorContext,
  themeColorForIndex,
} from '../src/drawingml/theme.ts';
import { OpcPackage } from '../src/opc/package.ts';
import { RelType } from '../src/opc/rel-types.ts';
import { buildXml } from '../src/xml/build.ts';
import { NS } from '../src/xml/namespaces.ts';
import type { XmlElement } from '../src/xml/node.ts';
import { parseXml } from '../src/xml/parse.ts';
import { fixture } from './fixtures.ts';

const A = `xmlns:a="${NS.a}"`;
const x = (s: string): XmlElement => parseXml(s.replace(/<a:(\w+)/, `<a:$1 ${A}`)).root;
/** Serialise → parse → compare, the round-trip property every DrawingML mapper must have. */
const reparse = (node: XmlElement) => parseXml(buildXml(node, { declaration: false })).root;

const theme = officeTheme();
const ctx = themeColorContext(theme);
const scheme = (val: string, ...t: [string, number][]): DrawingColor => ({
  kind: 'scheme',
  scheme: val,
  transforms: t.map(([name, v]) => ({ name, val: v })),
});

describe('theme colour resolution (Office theme)', () => {
  it.each([
    ['accent1', [], '4472C4'],
    [
      'accent1 lighter 80%',
      [
        ['lumMod', 20000],
        ['lumOff', 80000],
      ],
      'DAE3F3',
    ],
    [
      'accent1 lighter 60%',
      [
        ['lumMod', 40000],
        ['lumOff', 60000],
      ],
      'B4C7E7',
    ],
    [
      'accent1 lighter 40%',
      [
        ['lumMod', 60000],
        ['lumOff', 40000],
      ],
      '8FAADC',
    ],
    ['accent1 darker 25%', [['lumMod', 75000]], '2F5597'],
    ['accent1 darker 50%', [['lumMod', 50000]], '203864'],
    [
      'accent2 lighter 80%',
      [
        ['lumMod', 20000],
        ['lumOff', 80000],
      ],
      'FBE5D6',
    ],
    [
      'accent2 lighter 40%',
      [
        ['lumMod', 60000],
        ['lumOff', 40000],
      ],
      'F4B183',
    ],
    ['accent2 darker 25%', [['lumMod', 75000]], 'C55A11'],
    [
      'accent6 lighter 60%',
      [
        ['lumMod', 40000],
        ['lumOff', 60000],
      ],
      'C5E0B4',
    ],
    ['accent6 darker 25%', [['lumMod', 75000]], '548235'],
  ] as const)('%s', (label, transforms, hex) => {
    const slot = label.split(' ')[0] as string;
    expect(resolveColorHex(scheme(slot, ...(transforms as unknown as [string, number][])), ctx)).toBe(hex);
  });

  it.each([
    ['bg1', [['lumMod', 95000]], 'F2F2F2'],
    ['bg1', [['lumMod', 85000]], 'D9D9D9'],
    ['bg1', [['lumMod', 75000]], 'BFBFBF'],
    ['bg1', [['lumMod', 65000]], 'A6A6A6'],
    [
      'tx1',
      [
        ['lumMod', 95000],
        ['lumOff', 5000],
      ],
      '0D0D0D',
    ],
    [
      'tx1',
      [
        ['lumMod', 85000],
        ['lumOff', 15000],
      ],
      '262626',
    ],
    [
      'tx1',
      [
        ['lumMod', 75000],
        ['lumOff', 25000],
      ],
      '404040',
    ],
    [
      'tx1',
      [
        ['lumMod', 65000],
        ['lumOff', 35000],
      ],
      '595959',
    ],
    ['bg2', [['lumMod', 90000]], 'D0CECE'],
    [
      'tx2',
      [
        ['lumMod', 60000],
        ['lumOff', 40000],
      ],
      '8497B0',
    ],
  ] as const)('%s %j → %s (colour map bg/tx → lt/dk)', (slot, transforms, hex) => {
    expect(resolveColorHex(scheme(slot, ...(transforms as unknown as [string, number][])), ctx)).toBe(hex);
  });

  it('matches Office within one step where Office quantises HSL (accent2 darker 50%)', () => {
    // Office shows 843C0C; exact HSL arithmetic gives 843C0B. One 8-bit step is below visible difference.
    const got = hexToRgba(resolveColorHex(scheme('accent2', ['lumMod', 50000]), ctx));
    const want = hexToRgba('843C0C');
    for (const k of ['r', 'g', 'b'] as const) expect(Math.abs(got[k] - want[k])).toBeLessThanOrEqual(1);
  });

  it('honours a custom colour map and phClr', () => {
    const inverted = themeColorContext(theme, { bg1: 'dk1', tx1: 'lt1' });
    expect(resolveColorHex(schemeColor('bg1'), inverted)).toBe('000000');
    expect(resolveColorHex(schemeColor('tx1'), inverted)).toBe('FFFFFF');
    const withPh = themeColorContext(theme, undefined, hexToRgba('ED7D31'));
    expect(resolveColorHex(scheme('phClr', ['shade', 50000]), withPh)).toBe(
      resolveColorHex(srgb('ED7D31', { name: 'shade', val: 50000 })),
    );
    expect(resolveColorHex(schemeColor('accent9' as 'accent1'), ctx)).toBe('000000');
  });

  it('applies tint and shade in linear RGB, alpha and other transforms', () => {
    expect(resolveColorHex(srgb('000000', { name: 'tint', val: 50000 }))).toBe('BCBCBC');
    expect(resolveColorHex(srgb('FFFFFF', { name: 'shade', val: 50000 }))).toBe('BCBCBC');
    expect(resolveColorHex(srgb('4472C4', { name: 'tint', val: 100000 }))).toBe('4472C4');
    const a = resolveColor(srgb('FF0000', { name: 'alpha', val: 40000 }));
    expect(a.a).toBeCloseTo(0.4);
    expect(rgbaToCss(a)).toBe('rgba(255, 0, 0, 0.4)');
    expect(
      resolveColor(srgb('FF0000', { name: 'alpha', val: 50000 }, { name: 'alphaMod', val: 50000 })).a,
    ).toBeCloseTo(0.25);
    expect(resolveColorHex(srgb('FF0000', { name: 'comp' }))).toBe('00FFFF');
    expect(resolveColorHex(srgb('FF0000', { name: 'inv' }))).toBe('00FFFF');
    expect(resolveColorHex(srgb('FF0000', { name: 'gray' }))).toBe('383838');
    expect(resolveColorHex(srgb('FF0000', { name: 'hueOff', val: 7200000 }))).toBe('00FF00');
    expect(resolveColorHex(srgb('808080', { name: 'satMod', val: 200000 }))).toBe('808080');
    expect(resolveColorHex(srgb('336699', { name: 'red', val: 100000 }))).toBe('FF6699');
    expect(resolveColorHex(srgb('123456', { name: 'unknownTransform', val: 1 }))).toBe('123456');
  });

  it('resolves system, preset, HSL and scRGB colours', () => {
    expect(
      resolveColorHex(parseColor(x('<a:sysClr val="windowText" lastClr="111111"/>')) as DrawingColor),
    ).toBe('111111');
    expect(resolveColorHex(parseColor(x('<a:sysClr val="window"/>')) as DrawingColor)).toBe('FFFFFF');
    expect(resolveColorHex(parseColor(x('<a:prstClr val="dkBlue"/>')) as DrawingColor)).toBe('00008B');
    expect(presetColorHex('medPurple')).toBe('9370DB');
    expect(presetColorHex('ltGray')).toBe('D3D3D3');
    expect(presetColorHex('nope')).toBeUndefined();
    expect(
      resolveColorHex(parseColor(x('<a:hslClr hue="14400000" sat="100000" lum="50000"/>')) as DrawingColor),
    ).toBe('0000FF');
    expect(resolveColorHex(parseColor(x('<a:scrgbClr r="100000" g="0" b="21586"/>')) as DrawingColor)).toBe(
      'FF0080',
    );
  });

  it('round-trips colour XML with transforms in order', () => {
    const src = x(
      '<a:schemeClr val="accent1"><a:lumMod val="60000"/><a:lumOff val="40000"/><a:alpha val="50000"/></a:schemeClr>',
    );
    const c = parseColor(src) as DrawingColor;
    expect(c.transforms.map((t) => t.name)).toEqual(['lumMod', 'lumOff', 'alpha']);
    expect(parseColor(reparse(serializeColor(c)))).toEqual(c);
    expect(parseColorChild(x('<a:solidFill><a:srgbClr val="#a1b2c3"/></a:solidFill>'))).toEqual(
      srgb('A1B2C3'),
    );
  });

  it('applies SpreadsheetML tints like Excel', () => {
    expect(applyExcelTint('4472C4', 0.7999816888943144)).toBe('DAE3F3');
    expect(applyExcelTint('4472C4', -0.249977111117893)).toBe('2F5597');
    expect(applyExcelTint('FFFFFF', -0.1499984740745262)).toBe('D9D9D9');
    expect(themeColorForIndex(0)).toEqual(schemeColor('lt1'));
    expect(themeColorForIndex(4)).toEqual(schemeColor('accent1'));
  });

  it('converts RGB to HSL', () => {
    const hsl = rgbToHsl(68, 114, 196);
    expect(hsl.h).toBeCloseTo(218.44, 1);
    expect(hsl.s).toBeCloseTo(0.5203, 3);
    expect(hsl.l).toBeCloseTo(0.5176, 3);
  });
});

describe('themes', () => {
  const docxTheme = (() => {
    const pkg = OpcPackage.open(fixture('docx/features/styles.docx'));
    const part = pkg.relatedPart('/word/document.xml', RelType.theme) as string;
    return pkg.getPart(part) as Uint8Array;
  })();

  it('parses colour, font and format schemes from a real theme part', () => {
    const t = parseTheme(docxTheme);
    expect(t.name).toBe('Office Theme');
    expect(t.colorScheme.colors.dk1).toMatchObject({ kind: 'sys', sys: 'windowText' });
    expect(t.colorScheme.colors.lt1).toMatchObject({ kind: 'sys', sys: 'window' });
    expect(Object.keys(t.colorScheme.colors)).toHaveLength(12);
    expect(t.fontScheme.major.latin.typeface).toBeTruthy();
    expect(t.fontScheme.minor.scripts.length).toBeGreaterThan(10);
    expect(t.formatScheme.fillStyles).toHaveLength(3);
    expect(t.formatScheme.lineStyles).toHaveLength(3);
    expect(t.formatScheme.effectStyles).toHaveLength(3);
    expect(t.formatScheme.bgFillStyles).toHaveLength(3);
    expect(t.formatScheme.fillStyles[1]?.type).toBe('gradient');
  });

  it('round-trips real theme parts from all three formats through the model', () => {
    const parts = [
      docxTheme,
      OpcPackage.open(fixture('xlsx/features/styles.xlsx')).getPart('/xl/theme/theme1.xml') as Uint8Array,
      OpcPackage.open(fixture('pptx/features/theme-colors.pptx')).getPart(
        '/ppt/theme/theme1.xml',
      ) as Uint8Array,
    ];
    for (const bytes of parts) {
      const t = parseTheme(bytes);
      const again = parseTheme(buildXml(serializeTheme(t)));
      expect(again).toEqual(t);
    }
  });

  it('serialises the built-in Office theme and reads it back identically', () => {
    const xml = buildXml(serializeTheme(theme));
    expect(xml).toContain('<a:srgbClr val="4472C4"/>');
    expect(xml).toContain('typeface="Calibri Light"');
    expect(parseTheme(xml)).toEqual({ ...theme, extras: parseTheme(xml).extras });
  });

  it('resolves theme font references', () => {
    expect(resolveThemeFont('+mj-lt', theme)).toBe('Calibri Light');
    expect(resolveThemeFont('+mn-lt', theme)).toBe('Calibri');
    expect(resolveThemeFont('minorHAnsi', theme)).toBe('Calibri');
    expect(resolveThemeFont('majorAscii', theme)).toBe('Calibri Light');
    expect(resolveThemeFont('Georgia', theme)).toBe('Georgia');
  });

  it('rejects other XML', () => {
    expect(() => parseTheme('<a:foo xmlns:a="x"/>')).toThrow(/a:theme/);
  });
});

describe('fills', () => {
  it.each([
    ['none', '<a:noFill/>'],
    ['solid', '<a:solidFill><a:schemeClr val="accent2"><a:lumMod val="75000"/></a:schemeClr></a:solidFill>'],
    [
      'linear gradient',
      '<a:gradFill flip="none" rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="1"/><a:tileRect/></a:gradFill>',
    ],
    [
      'path gradient',
      '<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path></a:gradFill>',
    ],
    [
      'pattern',
      '<a:pattFill prst="dkDnDiag"><a:fgClr><a:srgbClr val="4472C4"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>',
    ],
    [
      'picture (stretch, crop)',
      `<a:blipFill dpi="0" rotWithShape="1"><a:blip xmlns:r="${NS.r}" r:embed="rId2"><a:alphaModFix amt="50000"/><a:lum bright="20000"/></a:blip><a:srcRect l="10000" t="15000" r="20000" b="5000"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>`,
    ],
    [
      'picture (tile)',
      `<a:blipFill><a:blip xmlns:r="${NS.r}" r:embed="rId3"/><a:tile tx="0" ty="0" sx="50000" sy="50000" flip="xy" algn="ctr"/></a:blipFill>`,
    ],
    ['group', '<a:grpFill/>'],
  ])('%s round-trips', (_label, xml) => {
    const fill = parseFill(x(xml)) as Fill;
    expect(fill).toBeDefined();
    expect(parseFill(reparse(serializeFill(fill)))).toEqual(fill);
  });

  it('models picture fill details', () => {
    const f = parseFill(
      x(
        `<a:blipFill><a:blip xmlns:r="${NS.r}" r:embed="rId2"><a:alphaModFix amt="50000"/></a:blip><a:srcRect l="10000" r="20000"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>`,
      ),
    );
    expect(f).toMatchObject({
      type: 'blip',
      embed: 'rId2',
      mode: 'stretch',
      alpha: 0.5,
      srcRect: { l: 0.1, t: 0, r: 0.2, b: 0 },
    });
    const pic = serializeFill(f as Fill, { blipFillName: 'p:blipFill' });
    expect(pic.name).toBe('p:blipFill');
  });

  it('renders CSS backgrounds', () => {
    expect(fillToCss({ type: 'none' })).toBe('transparent');
    expect(fillToCss({ type: 'solid', color: schemeColor('accent1') }, ctx)).toBe('#4472c4');
    const lin = parseFill(
      x(
        '<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="5400000"/></a:gradFill>',
      ),
    ) as Fill;
    expect(fillToCss(lin)).toBe('linear-gradient(180deg, #ff0000 0%, #0000ff 100%)');
    const rad = parseFill(
      x(
        '<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs></a:gsLst><a:path path="circle"><a:fillToRect l="50000" t="50000" r="50000" b="50000"/></a:path></a:gradFill>',
      ),
    ) as Fill;
    expect(fillToCss(rad)).toBe('radial-gradient(circle at 50% 50%, #ffffff 0%, #000000 100%)');
    expect(
      findFill(x('<a:spPr><a:xfrm/><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:spPr>')),
    ).toEqual({
      type: 'solid',
      color: srgb('00FF00'),
    });
  });
});

describe('lines', () => {
  const xml =
    '<a:ln w="28575" cap="rnd" cmpd="dbl" algn="ctr"><a:solidFill><a:srgbClr val="C00000"/></a:solidFill><a:prstDash val="dashDot"/><a:miter lim="800000"/><a:headEnd type="oval" w="sm" len="sm"/><a:tailEnd type="triangle" w="lg" len="lg"/></a:ln>';

  it('parses width, dash, caps, joins and ends', () => {
    const line = parseLine(x(xml));
    expect(line).toEqual({
      width: 28575,
      cap: 'rnd',
      compound: 'dbl',
      align: 'ctr',
      fill: { type: 'solid', color: srgb('C00000') },
      dash: 'dashDot',
      join: { type: 'miter', limit: 8 },
      head: { type: 'oval', w: 'sm', len: 'sm' },
      tail: { type: 'triangle', w: 'lg', len: 'lg' },
    });
    expect(parseLine(reparse(serializeLine(line as NonNullable<typeof line>)))).toEqual(line);
  });

  it('handles custom dashes and maps to SVG', () => {
    const line = parseLine(
      x('<a:ln w="12700"><a:custDash><a:ds d="300000" sp="100000"/></a:custDash><a:round/></a:ln>'),
    );
    expect(line?.dash).toEqual({ custom: [[300, 100]] });
    expect(dashArrayPx(line as NonNullable<typeof line>)).toBe('4 1.33');
    expect(svgCapJoin(line as NonNullable<typeof line>)).toEqual({ linecap: 'butt', linejoin: 'round' });
    expect(parseLine(reparse(serializeLine(line as NonNullable<typeof line>)))).toEqual(line);
    expect(dashArrayPx({ width: 12700, dash: 'sysDot' })).toBe('1.33 1.33');
    expect(dashArrayPx({ width: 12700, dash: 'solid' })).toBeUndefined();
  });
});

describe('effects', () => {
  const xml =
    '<a:effectLst><a:glow rad="101600"><a:schemeClr val="accent1"><a:alpha val="40000"/></a:schemeClr></a:glow><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:prstClr val="black"><a:alpha val="40000"/></a:prstClr></a:outerShdw><a:reflection blurRad="6350" stA="52000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/><a:softEdge rad="31750"/></a:effectLst>';

  it('parses and round-trips shadow, glow, reflection and soft edges', () => {
    const fx = parseEffectList(x(xml));
    expect(fx?.outerShadow).toMatchObject({
      blurRad: 50800,
      dist: 38100,
      dir: 45,
      algn: 'tl',
      rotWithShape: false,
    });
    expect(fx?.glow?.rad).toBe(101600);
    expect(fx?.reflection).toMatchObject({ stA: 0.52, endA: 0.003, endPos: 0.35, sy: -1, algn: 'bl' });
    expect(fx?.softEdge).toBe(31750);
    expect(parseEffectList(reparse(serializeEffectList(fx as NonNullable<typeof fx>)))).toEqual(fx);
    expect(buildXml(serializeEffectList(fx as NonNullable<typeof fx>), { declaration: false })).toBe(
      xml.replace('<a:effectLst>', `<a:effectLst ${A}>`),
    );
  });

  it('preserves unmodelled effects', () => {
    const fx = parseEffectList(
      x(
        '<a:effectLst><a:prstShdw prst="shdw1" dist="1"><a:srgbClr val="000000"/></a:prstShdw></a:effectLst>',
      ),
    );
    expect(fx?.other?.[0]?.name).toBe('a:prstShdw');
    expect(buildXml(serializeEffectList(fx as NonNullable<typeof fx>), { declaration: false })).toContain(
      '<a:prstShdw prst="shdw1" dist="1">',
    );
  });

  it('renders the outer shadow as CSS', () => {
    const fx = parseEffectList(x(xml));
    const css = outerShadowToCss(fx?.outerShadow as NonNullable<NonNullable<typeof fx>['outerShadow']>);
    expect(css.x).toBeCloseTo(2.83, 2);
    expect(css.y).toBeCloseTo(2.83, 2);
    expect(css.blur).toBeCloseTo(5.33, 2);
    expect(css.color).toBe('rgba(0, 0, 0, 0.4)');
    expect(css.filter).toBe('drop-shadow(2.83px 2.83px 2.67px rgba(0, 0, 0, 0.4))');
  });
});

describe('text properties', () => {
  it('round-trips a:bodyPr with autofit and preserved warp', () => {
    const src = x(
      '<a:bodyPr rot="0" vert="vert270" wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" numCol="2" spcCol="182880" rtlCol="0" anchor="ctr" anchorCtr="0"><a:prstTxWarp prst="textArchUp"><a:avLst/></a:prstTxWarp><a:normAutofit fontScale="92500" lnSpcReduction="10000"/></a:bodyPr>',
    );
    const bp = parseBodyProperties(src);
    expect(bp).toMatchObject({ vert: 'vert270', numCol: 2, anchor: 'ctr', anchorCtr: false, rtlCol: false });
    expect(bp.autofit).toEqual({ type: 'normal', fontScale: 0.925, lnSpcReduction: 0.1 });
    expect(bp.extras.map((e) => e.name)).toEqual(['a:prstTxWarp']);
    expect(buildXml(serializeBodyProperties(bp), { declaration: false })).toBe(
      buildXml(src, { declaration: false }),
    );
  });

  it('round-trips a:rPr', () => {
    const src = x(
      `<a:rPr lang="en-GB" altLang="de-DE" sz="2400" b="1" i="0" u="dbl" strike="sngStrike" kern="1200" cap="small" spc="-10" baseline="30000" dirty="0"><a:ln w="9525"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:effectLst><a:outerShdw blurRad="38100"><a:srgbClr val="000000"/></a:outerShdw></a:effectLst><a:highlight><a:srgbClr val="FFFF00"/></a:highlight><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:hlinkClick xmlns:r="${NS.r}" r:id="rId4"/><a:rtl val="0"/></a:rPr>`,
    );
    const rp = parseRunProperties(src);
    expect(rp).toMatchObject({
      lang: 'en-GB',
      sz: 2400,
      b: true,
      i: false,
      u: 'dbl',
      strike: 'sngStrike',
      cap: 'small',
      baseline: 30000,
      rtl: false,
    });
    expect(rp.latin?.typeface).toBe('+mj-lt');
    expect(rp.hyperlinks).toHaveLength(1);
    expect(parseRunProperties(reparse(serializeRunProperties(rp)))).toEqual(rp);
  });

  it('round-trips a:pPr with bullets, spacing and tabs', () => {
    const src = x(
      '<a:pPr marL="342900" lvl="1" indent="-342900" algn="just" rtl="0"><a:lnSpc><a:spcPct val="150000"/></a:lnSpc><a:spcBef><a:spcPts val="1200"/></a:spcBef><a:buClr><a:srgbClr val="FF0000"/></a:buClr><a:buSzPct val="80000"/><a:buFont typeface="Wingdings" pitchFamily="2" charset="2"/><a:buChar char="§"/><a:tabLst><a:tab pos="914400" algn="dec"/></a:tabLst><a:defRPr sz="1800"/></a:pPr>',
    );
    const pp = parseParagraphProperties(src);
    expect(pp).toMatchObject({ marL: 342900, lvl: 1, indent: -342900, algn: 'just' });
    expect(pp.lnSpc).toEqual({ pct: 1.5 });
    expect(pp.spcBef).toEqual({ pts: 1200 });
    expect(pp.bullet).toEqual({ type: 'char', char: '§' });
    expect(pp.buSz).toEqual({ pct: 0.8 });
    expect(pp.tabs).toEqual([{ pos: 914400, algn: 'dec' }]);
    expect(pp.defRPr?.sz).toBe(1800);
    expect(parseParagraphProperties(reparse(serializeParagraphProperties(pp)))).toEqual(pp);
    const auto = parseParagraphProperties(
      x('<a:pPr><a:buFontTx/><a:buAutoNum type="romanUcPeriod" startAt="3"/></a:pPr>'),
    );
    expect(auto.bullet).toEqual({ type: 'autoNum', scheme: 'romanUcPeriod', startAt: 3 });
    expect(auto.buFont).toBe('text');
  });

  it('round-trips a:lstStyle and merges inherited properties', () => {
    const src = x(
      '<a:lstStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr><a:lvl1pPr marL="0" algn="l"><a:defRPr sz="2800" b="1"/></a:lvl1pPr><a:lvl3pPr marL="914400"><a:buNone/></a:lvl3pPr></a:lstStyle>',
    );
    const ls = parseListStyle(src);
    expect(ls.levels[0]?.algn).toBe('l');
    expect(ls.levels[1]).toBeUndefined();
    expect(ls.levels[2]?.bullet).toEqual({ type: 'none' });
    expect(buildXml(serializeListStyle(ls), { declaration: false })).toBe(
      buildXml(src, { declaration: false }),
    );
    const merged = mergeParagraphProperties(
      ls.levels[0],
      parseParagraphProperties(x('<a:pPr algn="ctr"><a:defRPr i="1"/></a:pPr>')),
    );
    expect(merged.algn).toBe('ctr');
    expect(merged.marL).toBe(0);
    expect(merged.defRPr).toMatchObject({ sz: 2800, b: true, i: true });
    expect(mergeRunProperties(undefined, undefined)).toEqual({ hyperlinks: [], extras: [] });
  });

  it('reads text properties from a real slide', () => {
    const pkg = OpcPackage.open(fixture('pptx/features/text-formatting.pptx'));
    const slide = pkg.getPartXml('/ppt/slides/slide1.xml')?.root as XmlElement;
    const bodies = slide ? findAll(slide, 'a:bodyPr') : [];
    expect(
      bodies.map((b) => parseBodyProperties(b)).some((b) => b.numCol === 2 && b.autofit?.type === 'normal'),
    ).toBe(true);
    expect(bodies.map((b) => parseBodyProperties(b)).some((b) => b.vert === 'vert270')).toBe(true);
  });
});

function findAll(node: XmlElement, name: string): XmlElement[] {
  const out: XmlElement[] = [];
  const walk = (n: XmlElement) => {
    for (const c of n.children) {
      if (c.type !== 'element') continue;
      if (c.name === name) out.push(c);
      walk(c);
    }
  };
  walk(node);
  return out;
}
