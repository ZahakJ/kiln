/**
 * DrawingML shape geometry (ECMA-376 Part 1 §20.1.9): guide formulas, adjust values, paths, text rectangle,
 * connection sites and adjust handles. One engine serves both `a:prstGeom` (via the transcribed preset
 * table) and `a:custGeom`.
 */
import type { ST_PathFillMode } from '../st.ts';
import { roundTo } from '../units.ts';
import { el, type XmlElement } from '../xml/node.ts';
import { attr, child, children } from '../xml/query.ts';

/** A formula operand: a guide name or a numeric literal, kept as written. */
export type Operand = string;

/** `[name, formula]`, e.g. `['adj', 'val 25000']`. */
export type GuideDef = readonly [name: string, fmla: string];

export type PathCommand =
  | readonly ['M', Operand, Operand]
  | readonly ['L', Operand, Operand]
  | readonly ['A', wR: Operand, hR: Operand, stAng: Operand, swAng: Operand]
  | readonly ['Q', Operand, Operand, Operand, Operand]
  | readonly ['C', Operand, Operand, Operand, Operand, Operand, Operand]
  | readonly ['Z'];

export interface PathDef {
  /** Path coordinate-space width; coordinates are scaled by shape width / `w`. */
  w?: number;
  h?: number;
  fill?: ST_PathFillMode;
  /** `false` = do not stroke this path. */
  stroke?: boolean;
  extrusionOk?: boolean;
  cmds: readonly PathCommand[];
}

export type AdjustHandleDef =
  | {
      kind: 'xy';
      gdRefX?: string;
      minX?: Operand;
      maxX?: Operand;
      gdRefY?: string;
      minY?: Operand;
      maxY?: Operand;
      pos: readonly [Operand, Operand];
    }
  | {
      kind: 'polar';
      gdRefR?: string;
      minR?: Operand;
      maxR?: Operand;
      gdRefAng?: string;
      minAng?: Operand;
      maxAng?: Operand;
      pos: readonly [Operand, Operand];
    };

/** A geometry definition: the shape of `a:custGeom` and of every preset. */
export interface GeometryDef {
  av: readonly GuideDef[];
  gd: readonly GuideDef[];
  ah?: readonly AdjustHandleDef[];
  /** Connection sites: `[angle, x, y]`. */
  cxn?: readonly (readonly [Operand, Operand, Operand])[];
  /** Text rectangle `[l, t, r, b]`; defaults to the whole shape. */
  rect?: readonly [Operand, Operand, Operand, Operand];
  path: readonly PathDef[];
}

export interface ResolvedPath {
  /** SVG path data in shape coordinates. */
  d: string;
  fill: ST_PathFillMode;
  stroke: boolean;
  /** True when every subpath ends with `Z`. */
  closed: boolean;
}

export interface ResolvedHandle {
  kind: 'xy' | 'polar';
  x: number;
  y: number;
  /** Adjust value names this handle drives. */
  refs: string[];
  /** Evaluated limits (x/y for `xy`, r/ang for `polar`). */
  limits: Record<string, number>;
}

export interface ResolvedGeometry {
  paths: ResolvedPath[];
  textRect: { l: number; t: number; r: number; b: number };
  connectionSites: { angle: number; x: number; y: number }[];
  handles: ResolvedHandle[];
  /** Every guide value after evaluation (adjusts, built-ins, guides). */
  guides: Map<string, number>;
}

const DEG = 60000;
const toRad = (a: number) => (a / DEG) * (Math.PI / 180);
const toAng = (rad: number) => (rad * 180 * DEG) / Math.PI;

export class GeometryError extends Error {
  override name = 'GeometryError';
}

/** Built-in guide values for a shape of size `w` × `h` (ECMA-376 §20.1.9.11). */
export function builtinGuides(w: number, h: number): Map<string, number> {
  const ss = Math.min(w, h);
  const ls = Math.max(w, h);
  const g = new Map<string, number>([
    ['w', w],
    ['h', h],
    ['l', 0],
    ['t', 0],
    ['r', w],
    ['b', h],
    ['hc', w / 2],
    ['vc', h / 2],
    ['ss', ss],
    ['ls', ls],
    ['cd2', 10800000],
    ['cd3', 7200000],
    ['cd4', 5400000],
    ['cd8', 2700000],
    ['3cd4', 16200000],
    ['3cd8', 8100000],
    ['5cd8', 13500000],
    ['7cd8', 18900000],
  ]);
  for (const d of [2, 3, 4, 5, 6, 8, 10, 12, 32]) {
    g.set(`wd${d}`, w / d);
    g.set(`hd${d}`, h / d);
  }
  for (const d of [2, 4, 6, 8, 16, 32]) g.set(`ssd${d}`, ss / d);
  return g;
}

function operand(v: Operand, env: Map<string, number>): number {
  const n = env.get(v);
  if (n !== undefined) return n;
  const lit = Number(v);
  if (v.trim() === '' || !Number.isFinite(lit)) throw new GeometryError(`unknown guide "${v}"`);
  return lit;
}

/** Evaluates one guide formula (for example `+- w 0 adj`) in `env`. */
export function evalFormula(fmla: string, env: Map<string, number>): number {
  const [op, ...args] = fmla.trim().split(/\s+/);
  const a = (i: number) => operand(args[i] ?? '', env);
  switch (op) {
    case 'val':
      return a(0);
    case '*/':
      return (a(0) * a(1)) / a(2);
    case '+-':
      return a(0) + a(1) - a(2);
    case '+/':
      return (a(0) + a(1)) / a(2);
    case '?:':
      return a(0) > 0 ? a(1) : a(2);
    case 'abs':
      return Math.abs(a(0));
    case 'at2':
      return toAng(Math.atan2(a(1), a(0)));
    case 'cat2':
      return a(0) * Math.cos(Math.atan2(a(2), a(1)));
    case 'sat2':
      return a(0) * Math.sin(Math.atan2(a(2), a(1)));
    case 'cos':
      return a(0) * Math.cos(toRad(a(1)));
    case 'sin':
      return a(0) * Math.sin(toRad(a(1)));
    case 'tan':
      return a(0) * Math.tan(toRad(a(1)));
    case 'max':
      return Math.max(a(0), a(1));
    case 'min':
      return Math.min(a(0), a(1));
    case 'mod':
      return Math.sqrt(a(0) * a(0) + a(1) * a(1) + a(2) * a(2));
    case 'pin': {
      const lo = a(0);
      const x = a(1);
      const hi = a(2);
      return x < lo ? lo : x > hi ? hi : x;
    }
    case 'sqrt':
      return Math.sqrt(a(0));
    default:
      throw new GeometryError(`unknown formula operator "${op}"`);
  }
}

/** Evaluates adjust values (with overrides) and guides into one environment. */
export function evaluateGuides(
  def: GeometryDef,
  w: number,
  h: number,
  adjustments: Readonly<Record<string, number>> = {},
): Map<string, number> {
  const env = builtinGuides(w, h);
  for (const [name, fmla] of def.av) {
    const override = adjustments[name];
    env.set(name, override !== undefined && Number.isFinite(override) ? override : evalFormula(fmla, env));
  }
  for (const [name, fmla] of def.gd) env.set(name, evalFormula(fmla, env));
  return env;
}

const fmt = (n: number): string => {
  const r = roundTo(n, 3);
  return String(r);
};

/** Point on an ellipse at *visual* angle `ang` (60000ths of a degree), relative to its centre. */
function ellipsePoint(wR: number, hR: number, ang: number): [number, number] {
  const th = toRad(ang);
  const t = Math.atan2(wR * Math.sin(th), hR * Math.cos(th));
  return [wR * Math.cos(t), hR * Math.sin(t)];
}

function resolvePath(p: PathDef, env: Map<string, number>, w: number, h: number): ResolvedPath {
  const sx = p.w ? w / p.w : 1;
  const sy = p.h ? h / p.h : 1;
  const parts: string[] = [];
  let cx = 0;
  let cy = 0;
  let open = false;
  let allClosed = true;
  const X = (v: Operand) => operand(v, env) * sx;
  const Y = (v: Operand) => operand(v, env) * sy;
  for (const c of p.cmds) {
    switch (c[0]) {
      case 'M':
        if (open) allClosed = false;
        cx = X(c[1]);
        cy = Y(c[2]);
        parts.push(`M${fmt(cx)} ${fmt(cy)}`);
        open = true;
        break;
      case 'L':
        cx = X(c[1]);
        cy = Y(c[2]);
        parts.push(`L${fmt(cx)} ${fmt(cy)}`);
        break;
      case 'Q':
        parts.push(`Q${fmt(X(c[1]))} ${fmt(Y(c[2]))} ${fmt(X(c[3]))} ${fmt(Y(c[4]))}`);
        cx = X(c[3]);
        cy = Y(c[4]);
        break;
      case 'C':
        parts.push(
          `C${fmt(X(c[1]))} ${fmt(Y(c[2]))} ${fmt(X(c[3]))} ${fmt(Y(c[4]))} ${fmt(X(c[5]))} ${fmt(Y(c[6]))}`,
        );
        cx = X(c[5]);
        cy = Y(c[6]);
        break;
      case 'A': {
        const wR = X(c[1]);
        const hR = Y(c[2]);
        const st = operand(c[3], env);
        const sw = operand(c[4], env);
        const [sx0, sy0] = ellipsePoint(wR, hR, st);
        const ox = cx - sx0;
        const oy = cy - sy0;
        if (wR === 0 || hR === 0 || sw === 0) {
          const [ex, ey] = ellipsePoint(wR, hR, st + sw);
          cx = ox + ex;
          cy = oy + ey;
          parts.push(`L${fmt(cx)} ${fmt(cy)}`);
          break;
        }
        // SVG cannot draw a full ellipse in one arc command, so long sweeps are split.
        const steps = Math.ceil(Math.abs(sw) / (DEG * 180) - 1e-9) || 1;
        const sweepFlag = sw > 0 ? 1 : 0;
        for (let i = 1; i <= steps; i++) {
          const segSweep = sw / steps;
          const [ex, ey] = ellipsePoint(wR, hR, st + segSweep * i);
          const large = Math.abs(segSweep) > DEG * 180 ? 1 : 0;
          cx = ox + ex;
          cy = oy + ey;
          parts.push(`A${fmt(wR)} ${fmt(hR)} 0 ${large} ${sweepFlag} ${fmt(cx)} ${fmt(cy)}`);
        }
        break;
      }
      case 'Z':
        parts.push('Z');
        open = false;
        break;
    }
  }
  if (open) allClosed = false;
  return {
    d: parts.join(' '),
    fill: p.fill ?? 'norm',
    stroke: p.stroke ?? true,
    closed: allClosed && parts.length > 0,
  };
}

/** Evaluates a geometry for a shape of size `w` × `h` (any unit; EMU or px). */
export function resolveGeometry(
  def: GeometryDef,
  w: number,
  h: number,
  adjustments: Readonly<Record<string, number>> = {},
): ResolvedGeometry {
  const env = evaluateGuides(def, w, h, adjustments);
  const paths = def.path.map((p) => resolvePath(p, env, w, h));
  const rect = def.rect;
  const textRect = rect
    ? {
        l: operand(rect[0], env),
        t: operand(rect[1], env),
        r: operand(rect[2], env),
        b: operand(rect[3], env),
      }
    : { l: 0, t: 0, r: w, b: h };
  const connectionSites = (def.cxn ?? []).map(([a, x, y]) => ({
    angle: operand(a, env),
    x: operand(x, env),
    y: operand(y, env),
  }));
  const handles: ResolvedHandle[] = (def.ah ?? []).map((ah) => {
    const limits: Record<string, number> = {};
    const lim = (k: string, v: Operand | undefined) => {
      if (v !== undefined) limits[k] = operand(v, env);
    };
    if (ah.kind === 'xy') {
      lim('minX', ah.minX);
      lim('maxX', ah.maxX);
      lim('minY', ah.minY);
      lim('maxY', ah.maxY);
      return {
        kind: 'xy',
        x: operand(ah.pos[0], env),
        y: operand(ah.pos[1], env),
        refs: [ah.gdRefX, ah.gdRefY].filter((r): r is string => !!r),
        limits,
      };
    }
    lim('minR', ah.minR);
    lim('maxR', ah.maxR);
    lim('minAng', ah.minAng);
    lim('maxAng', ah.maxAng);
    return {
      kind: 'polar',
      x: operand(ah.pos[0], env),
      y: operand(ah.pos[1], env),
      refs: [ah.gdRefR, ah.gdRefAng].filter((r): r is string => !!r),
      limits,
    };
  });
  return { paths, textRect, connectionSites, handles, guides: env };
}

/** All paths of a geometry joined into one SVG path string (the shape's outline). */
export function geometryToSvgPath(
  def: GeometryDef,
  w: number,
  h: number,
  adjustments?: Readonly<Record<string, number>>,
): string {
  return resolveGeometry(def, w, h, adjustments)
    .paths.map((p) => p.d)
    .filter(Boolean)
    .join(' ');
}

// ------------------------------------------------------------- XML mapping

const PT = (e: XmlElement | undefined): [Operand, Operand] => [attr(e, 'x') ?? '0', attr(e, 'y') ?? '0'];

function guideList(list: XmlElement | undefined): GuideDef[] {
  return children(list, 'a:gd').map((g) => [attr(g, 'name') ?? '', attr(g, 'fmla') ?? 'val 0'] as const);
}

/** Parses `a:custGeom` (or any element with `a:avLst`/`a:gdLst`/`a:ahLst`/`a:cxnLst`/`a:rect`/`a:pathLst`). */
export function parseGeometry(node: XmlElement): GeometryDef {
  const def: {
    av: GuideDef[];
    gd: GuideDef[];
    ah?: AdjustHandleDef[];
    cxn?: [Operand, Operand, Operand][];
    rect?: [Operand, Operand, Operand, Operand];
    path: PathDef[];
  } = { av: guideList(child(node, 'a:avLst')), gd: guideList(child(node, 'a:gdLst')), path: [] };

  const ahLst = child(node, 'a:ahLst');
  if (ahLst) {
    def.ah = children(ahLst, ['a:ahXY', 'a:ahPolar']).map((h): AdjustHandleDef => {
      const pos = PT(child(h, 'a:pos'));
      const pick = (o: Record<string, string | undefined>) =>
        Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as Record<string, string>;
      if (h.name === 'a:ahXY') {
        return {
          kind: 'xy',
          ...pick({
            gdRefX: attr(h, 'gdRefX'),
            minX: attr(h, 'minX'),
            maxX: attr(h, 'maxX'),
            gdRefY: attr(h, 'gdRefY'),
            minY: attr(h, 'minY'),
            maxY: attr(h, 'maxY'),
          }),
          pos,
        };
      }
      return {
        kind: 'polar',
        ...pick({
          gdRefR: attr(h, 'gdRefR'),
          minR: attr(h, 'minR'),
          maxR: attr(h, 'maxR'),
          gdRefAng: attr(h, 'gdRefAng'),
          minAng: attr(h, 'minAng'),
          maxAng: attr(h, 'maxAng'),
        }),
        pos,
      };
    });
  }
  const cxnLst = child(node, 'a:cxnLst');
  if (cxnLst) {
    def.cxn = children(cxnLst, 'a:cxn').map((c) => {
      const [x, y] = PT(child(c, 'a:pos'));
      return [attr(c, 'ang') ?? '0', x, y];
    });
  }
  const rect = child(node, 'a:rect');
  if (rect) {
    def.rect = [
      attr(rect, 'l') ?? 'l',
      attr(rect, 't') ?? 't',
      attr(rect, 'r') ?? 'r',
      attr(rect, 'b') ?? 'b',
    ];
  }
  for (const p of children(child(node, 'a:pathLst'), 'a:path')) {
    const cmds: PathCommand[] = [];
    for (const c of children(p)) {
      switch (c.name) {
        case 'a:moveTo':
          cmds.push(['M', ...PT(child(c, 'a:pt'))]);
          break;
        case 'a:lnTo':
          cmds.push(['L', ...PT(child(c, 'a:pt'))]);
          break;
        case 'a:arcTo':
          cmds.push([
            'A',
            attr(c, 'wR') ?? '0',
            attr(c, 'hR') ?? '0',
            attr(c, 'stAng') ?? '0',
            attr(c, 'swAng') ?? '0',
          ]);
          break;
        case 'a:quadBezTo': {
          const [p1, p2] = children(c, 'a:pt');
          cmds.push(['Q', ...PT(p1), ...PT(p2)]);
          break;
        }
        case 'a:cubicBezTo': {
          const [p1, p2, p3] = children(c, 'a:pt');
          cmds.push(['C', ...PT(p1), ...PT(p2), ...PT(p3)]);
          break;
        }
        case 'a:close':
          cmds.push(['Z']);
          break;
      }
    }
    const pd: PathDef = { cmds };
    const w = attr(p, 'w');
    const h = attr(p, 'h');
    if (w !== undefined) pd.w = Number(w);
    if (h !== undefined) pd.h = Number(h);
    const fill = attr(p, 'fill');
    if (fill !== undefined) pd.fill = fill as ST_PathFillMode;
    const stroke = attr(p, 'stroke');
    if (stroke !== undefined) pd.stroke = !(stroke === 'false' || stroke === '0');
    const ext = attr(p, 'extrusionOk');
    if (ext !== undefined) pd.extrusionOk = !(ext === 'false' || ext === '0');
    def.path.push(pd);
  }
  return def;
}

const gdEl = ([name, fmla]: GuideDef) => el('a:gd', { name, fmla });
const ptEl = (x: Operand, y: Operand) => el('a:pt', { x, y });

/** Serialises a geometry as `a:custGeom`. */
export function serializeCustomGeometry(def: GeometryDef): XmlElement {
  return el(
    'a:custGeom',
    {},
    el('a:avLst', {}, def.av.map(gdEl)),
    el('a:gdLst', {}, def.gd.map(gdEl)),
    el(
      'a:ahLst',
      {},
      (def.ah ?? []).map((h) =>
        h.kind === 'xy'
          ? el(
              'a:ahXY',
              { gdRefX: h.gdRefX, minX: h.minX, maxX: h.maxX, gdRefY: h.gdRefY, minY: h.minY, maxY: h.maxY },
              el('a:pos', { x: h.pos[0], y: h.pos[1] }),
            )
          : el(
              'a:ahPolar',
              {
                gdRefR: h.gdRefR,
                minR: h.minR,
                maxR: h.maxR,
                gdRefAng: h.gdRefAng,
                minAng: h.minAng,
                maxAng: h.maxAng,
              },
              el('a:pos', { x: h.pos[0], y: h.pos[1] }),
            ),
      ),
    ),
    el(
      'a:cxnLst',
      {},
      (def.cxn ?? []).map(([ang, x, y]) => el('a:cxn', { ang }, el('a:pos', { x, y }))),
    ),
    el('a:rect', {
      l: def.rect?.[0] ?? 'l',
      t: def.rect?.[1] ?? 't',
      r: def.rect?.[2] ?? 'r',
      b: def.rect?.[3] ?? 'b',
    }),
    el(
      'a:pathLst',
      {},
      def.path.map((p) =>
        el(
          'a:path',
          {
            w: p.w,
            h: p.h,
            fill: p.fill,
            stroke: p.stroke === false ? 'false' : undefined,
            extrusionOk: p.extrusionOk === false ? 'false' : undefined,
          },
          p.cmds.map((c) => {
            switch (c[0]) {
              case 'M':
                return el('a:moveTo', {}, ptEl(c[1], c[2]));
              case 'L':
                return el('a:lnTo', {}, ptEl(c[1], c[2]));
              case 'A':
                return el('a:arcTo', { wR: c[1], hR: c[2], stAng: c[3], swAng: c[4] });
              case 'Q':
                return el('a:quadBezTo', {}, ptEl(c[1], c[2]), ptEl(c[3], c[4]));
              case 'C':
                return el('a:cubicBezTo', {}, ptEl(c[1], c[2]), ptEl(c[3], c[4]), ptEl(c[5], c[6]));
              default:
                return el('a:close');
            }
          }),
        ),
      ),
    ),
  );
}
