import { describe, expect, it } from 'vitest';
import {
  evalFormula,
  geometryToSvgPath,
  parseGeometry,
  resolveGeometry,
  serializeCustomGeometry,
} from '../src/drawingml/geometry.ts';
import {
  isPresetShapeType,
  PRESET_CATEGORIES,
  PRESET_SHAPE_TYPES,
  presetCategory,
  presetGeometry,
  presetToSvgPath,
} from '../src/drawingml/presets/index.ts';
import { buildXml } from '../src/xml/build.ts';
import { parseXml } from '../src/xml/parse.ts';

/** Lines and connectors are open by design; every other preset outlines a closed area. */
const OPEN_PRESETS = new Set(Object.keys(PRESET_CATEGORIES.linesConnectors));

describe('preset geometry table', () => {
  it('has all 187 ECMA-376 presets', () => {
    expect(PRESET_SHAPE_TYPES).toHaveLength(187);
    expect(new Set(PRESET_SHAPE_TYPES).size).toBe(187);
    expect(isPresetShapeType('rect')).toBe(true);
    expect(isPresetShapeType('toString')).toBe(false);
    expect(presetCategory('flowChartDecision')).toBe('flowchart');
    expect(presetCategory('rightArrow')).toBe('arrows');
  });

  it('throws on an unknown preset', () => {
    expect(() => presetToSvgPath('notAShape', 10, 10)).toThrow(RangeError);
  });
});

describe('all 187 presets produce a non-empty path without NaN', () => {
  it.each(PRESET_SHAPE_TYPES)('%s', (name) => {
    for (const [w, h] of [
      [200, 100],
      [100, 300],
      [914400, 914400],
    ] as const) {
      const g = presetGeometry(name, w, h);
      expect(g.paths.length).toBeGreaterThan(0);
      for (const p of g.paths) {
        expect(p.d.length).toBeGreaterThan(0);
        expect(p.d).not.toMatch(/NaN|Infinity|undefined/);
        expect(p.d.startsWith('M')).toBe(true);
      }
      if (OPEN_PRESETS.has(name)) {
        expect(g.paths.some((p) => !p.closed)).toBe(true);
      } else {
        // At least one closed path describes the filled area.
        expect(g.paths.some((p) => p.closed && p.d.endsWith('Z'))).toBe(true);
      }
      for (const v of [g.textRect.l, g.textRect.t, g.textRect.r, g.textRect.b])
        expect(Number.isFinite(v)).toBe(true);
      for (const c of g.connectionSites) expect(Number.isFinite(c.x) && Number.isFinite(c.y)).toBe(true);
    }
  });
});

/**
 * Hand-computed expectations at w = 200, h = 100 (so ss = 100), from the formulas in
 * presetShapeDefinitions.xml with default adjust values unless stated. Arcs use the "visual" angle
 * convention of DrawingML (the angle of the ray from the ellipse centre).
 */
const HAND: [name: string, d: string, adj?: Record<string, number>][] = [
  ['rect', 'M0 0 L200 0 L200 100 L0 100 Z'],
  ['triangle', 'M0 100 L100 0 L200 100 Z'],
  ['triangle', 'M0 100 L50 0 L200 100 Z', { adj: 25000 }],
  ['rtTriangle', 'M0 100 L0 0 L200 100 Z'],
  ['diamond', 'M0 50 L100 0 L200 50 L100 100 Z'],
  ['parallelogram', 'M0 100 L25 0 L200 0 L175 100 Z'],
  ['trapezoid', 'M0 100 L25 0 L175 0 L200 100 Z'],
  ['nonIsoscelesTrapezoid', 'M0 100 L10 0 L160 0 L200 100 Z', { adj1: 10000, adj2: 40000 }],
  ['hexagon', 'M0 50 L25 0 L175 0 L200 50 L175 100 L25 100 Z'],
  ['octagon', 'M0 29.289 L29.289 0 L170.711 0 L200 29.289 L200 70.711 L170.711 100 L29.289 100 L0 70.711 Z'],
  ['plus', 'M0 25 L25 25 L25 0 L175 0 L175 25 L200 25 L200 75 L175 75 L175 100 L25 100 L25 75 L0 75 Z'],
  ['frame', 'M0 0 L200 0 L200 100 L0 100 Z M12.5 12.5 L12.5 87.5 L187.5 87.5 L187.5 12.5 Z'],
  ['halfFrame', 'M0 0 L200 0 L160 20 L30 20 L30 85 L0 100 Z', { adj1: 20000, adj2: 30000 }],
  ['corner', 'M0 0 L50 0 L50 50 L200 50 L200 100 L0 100 Z'],
  ['diagStripe', 'M0 50 L100 0 L200 0 L0 100 Z'],
  ['snip1Rect', 'M0 0 L183.333 0 L200 16.667 L200 100 L0 100 Z'],
  ['snip2SameRect', 'M16.667 0 L183.333 0 L200 16.667 L200 100 L200 100 L0 100 L0 100 L0 16.667 Z'],
  ['snip2DiagRect', 'M0 0 L183.333 0 L200 16.667 L200 100 L200 100 L16.667 100 L0 83.333 L0 0 Z'],
  ['round1Rect', 'M0 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 100 L0 100 Z'],
  [
    'round2SameRect',
    'M16.667 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 100 L200 100 L0 100 L0 100 L0 16.667 A16.667 16.667 0 0 1 16.667 0 Z',
  ],
  [
    'round2DiagRect',
    'M16.667 0 L200 0 L200 0 L200 83.333 A16.667 16.667 0 0 1 183.333 100 L0 100 L0 100 L0 16.667 A16.667 16.667 0 0 1 16.667 0 Z',
  ],
  [
    'roundRect',
    'M0 16.667 A16.667 16.667 0 0 1 16.667 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 83.333 A16.667 16.667 0 0 1 183.333 100 L16.667 100 A16.667 16.667 0 0 1 0 83.333 Z',
  ],
  ['ellipse', 'M0 50 A100 50 0 0 1 100 0 A100 50 0 0 1 200 50 A100 50 0 0 1 100 100 A100 50 0 0 1 0 50 Z'],
  ['flowChartProcess', 'M0 0 L200 0 L200 100 L0 100 Z'],
  ['flowChartDecision', 'M0 50 L100 0 L200 50 L100 100 Z'],
  ['flowChartInputOutput', 'M0 100 L40 0 L200 0 L160 100 Z'],
  ['flowChartManualInput', 'M0 20 L200 0 L200 100 L0 100 Z'],
  ['flowChartManualOperation', 'M0 0 L200 0 L160 100 L40 100 Z'],
  ['flowChartMerge', 'M0 0 L200 0 L100 100 Z'],
  ['flowChartExtract', 'M0 100 L100 0 L200 100 Z'],
  ['flowChartPreparation', 'M0 50 L40 0 L160 0 L200 50 L160 100 L40 100 Z'],
  ['flowChartPunchedCard', 'M0 20 L40 0 L200 0 L200 100 L0 100 Z'],
  ['flowChartOffpageConnector', 'M0 0 L200 0 L200 80 L100 100 L0 80 Z'],
  ['flowChartCollate', 'M0 0 L200 0 L100 50 L200 100 L0 100 L100 50 Z'],
  [
    'flowChartInternalStorage',
    'M0 0 L200 0 L200 100 L0 100 Z M25 0 L25 100 M0 12.5 L200 12.5 M0 0 L200 0 L200 100 L0 100 Z',
  ],
  [
    'flowChartPredefinedProcess',
    'M0 0 L200 0 L200 100 L0 100 Z M25 0 L25 100 M175 0 L175 100 M0 0 L200 0 L200 100 L0 100 Z',
  ],
  [
    'flowChartConnector',
    'M0 50 A100 50 0 0 1 100 0 A100 50 0 0 1 200 50 A100 50 0 0 1 100 100 A100 50 0 0 1 0 50 Z',
  ],
  ['flowChartDelay', 'M0 0 L100 0 A100 50 0 0 1 100 100 L0 100 Z'],
  ['homePlate', 'M0 0 L150 0 L200 50 L150 100 L0 100 Z'],
  ['chevron', 'M0 0 L150 0 L200 50 L150 100 L0 100 L50 50 Z'],
  ['rightArrow', 'M0 25 L150 25 L150 0 L200 50 L150 100 L150 75 L0 75 Z'],
  ['leftArrow', 'M0 50 L50 0 L50 25 L200 25 L200 75 L50 75 L50 100 Z'],
  ['upArrow', 'M0 50 L100 0 L200 50 L150 50 L150 100 L50 100 L50 50 Z'],
  ['downArrow', 'M0 50 L50 50 L50 0 L150 0 L150 50 L200 50 L100 100 Z'],
  ['leftRightArrow', 'M0 50 L50 0 L50 25 L150 25 L150 0 L200 50 L150 100 L150 75 L50 75 L50 100 Z'],
  [
    'upDownArrow',
    'M0 25 L100 0 L200 25 L150 25 L150 75 L200 75 L100 100 L0 75 L50 75 L50 25 Z',
    { adj2: 25000 },
  ],
  ['notchedRightArrow', 'M0 25 L150 25 L150 0 L200 50 L150 100 L150 75 L0 75 L25 50 Z'],
  ['mathMinus', 'M26.51 38.24 L173.49 38.24 L173.49 61.76 L26.51 61.76 Z'],
  [
    'mathPlus',
    'M26.51 38.24 L88.24 38.24 L88.24 13.255 L111.76 13.255 L111.76 38.24 L173.49 38.24 L173.49 61.76 L111.76 61.76 L111.76 86.745 L88.24 86.745 L88.24 61.76 L26.51 61.76 Z',
  ],
  ['line', 'M0 0 L200 100'],
  ['straightConnector1', 'M0 0 L200 100'],
  ['bentConnector2', 'M0 0 L200 0 L200 100'],
  ['bentConnector3', 'M0 0 L100 0 L100 100 L200 100'],
  ['bentConnector3', 'M0 0 L50 0 L50 100 L200 100', { adj1: 25000 }],
  // Pie from 0° to 270° (visual angles): split into two 135° arcs; the 135° point on the 100×50 ellipse
  // lies on the ray y = −x: (100 − 44.721, 50 + 44.721).
  ['pie', 'M200 50 A100 50 0 0 1 55.279 94.721 A100 50 0 0 1 100 0 L100 50 Z'],
];

describe('hand-computed preset paths (200 × 100)', () => {
  it.each(
    HAND.map(([n, d, adj], i) => [`${n}${adj ? ` ${JSON.stringify(adj)}` : ''} #${i}`, n, d, adj] as const),
  )('%s', (_label, name, d, adj) => {
    expect(presetToSvgPath(name, 200, 100, adj)).toBe(d);
  });

  it('covers at least 40 distinct presets', () => {
    expect(new Set(HAND.map(([n]) => n)).size).toBeGreaterThanOrEqual(40);
  });

  it('keeps per-path fill modes and stroke flags (cube, bevel)', () => {
    const cube = presetGeometry('cube', 200, 100);
    expect(cube.paths.map((p) => [p.fill, p.stroke])).toEqual([
      ['norm', false],
      ['darkenLess', false],
      ['lightenLess', false],
      ['none', true],
    ]);
    expect(cube.paths[0]?.d).toBe('M0 25 L175 25 L175 100 L0 100 Z');
    expect(cube.paths[1]?.d).toBe('M175 25 L200 0 L200 75 L175 100 Z');
    expect(cube.paths[2]?.d).toBe('M0 25 L25 0 L200 0 L175 25 Z');
    expect(cube.paths[3]?.d).toBe(
      'M0 25 L25 0 L200 0 L200 75 L175 100 L0 100 Z M0 25 L175 25 L200 0 M175 25 L175 100',
    );
    const bevel = presetGeometry('bevel', 200, 100);
    expect(bevel.paths[0]?.d).toBe('M12.5 12.5 L187.5 12.5 L187.5 87.5 L12.5 87.5 Z');
    expect(bevel.paths[1]).toMatchObject({ d: 'M0 0 L200 0 L187.5 12.5 L12.5 12.5 Z', fill: 'lightenLess' });
  });

  it('evaluates text rectangles, connection sites and adjust handles', () => {
    const tri = presetGeometry('triangle', 200, 100);
    // rect l="x1" t="vc" r="x3" b="b" with x1 = w·adj/200000 = 50 and x3 = x1 + w/2 = 150.
    expect(tri.textRect).toEqual({ l: 50, t: 50, r: 150, b: 100 });
    expect(tri.connectionSites).toContainEqual({ angle: 16200000, x: 100, y: 0 });
    expect(tri.handles).toEqual([
      { kind: 'xy', x: 100, y: 0, refs: ['adj'], limits: { minX: 0, maxX: 100000 } },
    ]);
    const rr = presetGeometry('roundRect', 200, 100, { adj: 50000 });
    expect(rr.guides.get('x1')).toBe(50);
  });
});

describe('guide formulas', () => {
  const env = new Map<string, number>([
    ['a', 3],
    ['b', 4],
    ['c', -2],
  ]);
  it.each([
    ['val 7', 7],
    ['*/ a b 2', 6],
    ['+- a b 1', 6],
    ['+/ a b 7', 1],
    ['?: c a b', 4],
    ['?: a a b', 3],
    ['abs c', 2],
    ['max a b', 4],
    ['min a c', -2],
    ['mod a b 0', 5],
    ['pin 0 c 10', 0],
    ['pin 0 b 3', 3],
    ['sqrt 16', 4],
    ['cat2 10 a b', 6],
    ['sat2 10 a b', 8],
    ['cos 2 0', 2],
    ['sin 2 5400000', 2],
    ['at2 1 1', 2700000],
  ])('%s = %d', (fmla, expected) => {
    expect(evalFormula(fmla, env)).toBeCloseTo(expected, 6);
  });

  it('rejects unknown operators and guides', () => {
    expect(() => evalFormula('pow a b', env)).toThrow();
    expect(() => evalFormula('+- zz 0 0', env)).toThrow();
  });
});

describe('a:custGeom', () => {
  const xml = `<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:avLst/><a:gdLst><a:gd name="q" fmla="*/ w 1 4"/></a:gdLst><a:ahLst/><a:cxnLst><a:cxn ang="0"><a:pos x="r" y="vc"/></a:cxn></a:cxnLst><a:rect l="q" t="t" r="r" b="b"/><a:pathLst><a:path w="100" h="100"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="100" y="0"/></a:lnTo><a:quadBezTo><a:pt x="100" y="100"/><a:pt x="50" y="100"/></a:quadBezTo><a:cubicBezTo><a:pt x="25" y="100"/><a:pt x="0" y="50"/><a:pt x="0" y="0"/></a:cubicBezTo><a:close/></a:path></a:pathLst></a:custGeom>`;

  it('parses, scales path coordinates and round-trips through XML', () => {
    const def = parseGeometry(parseXml(xml).root);
    expect(geometryToSvgPath(def, 200, 50)).toBe('M0 0 L200 0 Q200 50 100 50 C50 50 0 25 0 0 Z');
    const g = resolveGeometry(def, 200, 50);
    expect(g.textRect).toEqual({ l: 50, t: 0, r: 200, b: 50 });
    expect(g.connectionSites).toEqual([{ angle: 0, x: 200, y: 25 }]);
    const again = parseGeometry(parseXml(buildXml(serializeCustomGeometry(def))).root);
    expect(again).toEqual(def);
  });
});
