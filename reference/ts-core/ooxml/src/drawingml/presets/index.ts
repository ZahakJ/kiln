/**
 * The 187 DrawingML preset geometries (ST_ShapeType), transcribed from ECMA-376 Part 1
 * presetShapeDefinitions.xml by `scripts/gen-presets.ts`.
 */
import { type GeometryDef, geometryToSvgPath, type ResolvedGeometry, resolveGeometry } from '../geometry.ts';
import { actionButtons } from './action-buttons.ts';
import { arrows } from './arrows.ts';
import { basic } from './basic.ts';
import { callouts } from './callouts.ts';
import { flowchart } from './flowchart.ts';
import { linesConnectors } from './lines-connectors.ts';
import { math } from './math.ts';
import { starsBanners } from './stars-banners.ts';

/** Preset geometries grouped as in Office's Insert ▸ Shapes gallery. */
export const PRESET_CATEGORIES = {
  basic,
  linesConnectors,
  arrows,
  flowchart,
  starsBanners,
  callouts,
  math,
  actionButtons,
} as const satisfies Record<string, Record<string, GeometryDef>>;

export type PresetCategory = keyof typeof PRESET_CATEGORIES;

/** Every preset geometry by ST_ShapeType name. */
export const PRESET_GEOMETRIES: Readonly<Record<string, GeometryDef>> = Object.freeze(
  Object.assign(Object.create(null) as Record<string, GeometryDef>, ...Object.values(PRESET_CATEGORIES)),
);

/** ST_ShapeType values, sorted. */
export const PRESET_SHAPE_TYPES: readonly string[] = Object.freeze(Object.keys(PRESET_GEOMETRIES).sort());

export function isPresetShapeType(name: unknown): name is string {
  return typeof name === 'string' && Object.hasOwn(PRESET_GEOMETRIES, name);
}

/** The category a preset belongs to, for galleries. */
export function presetCategory(name: string): PresetCategory | undefined {
  for (const [cat, defs] of Object.entries(PRESET_CATEGORIES))
    if (Object.hasOwn(defs, name)) return cat as PresetCategory;
  return undefined;
}

export function presetDefinition(name: string): GeometryDef {
  const def = PRESET_GEOMETRIES[name];
  if (!def) throw new RangeError(`unknown preset geometry "${name}"`);
  return def;
}

/**
 * Evaluates a preset for a `w` × `h` shape with optional adjust values (`{ adj: 25000 }`, as stored in
 * `a:prstGeom/a:avLst`): paths with fill/stroke flags, text rectangle, connection sites and handles.
 */
export function presetGeometry(
  name: string,
  w: number,
  h: number,
  adjustments?: Readonly<Record<string, number>>,
): ResolvedGeometry {
  return resolveGeometry(presetDefinition(name), w, h, adjustments);
}

/** SVG path data (all of the preset's paths joined) for a `w` × `h` shape. */
export function presetToSvgPath(
  name: string,
  w: number,
  h: number,
  adjustments?: Readonly<Record<string, number>>,
): string {
  return geometryToSvgPath(presetDefinition(name), w, h, adjustments);
}
