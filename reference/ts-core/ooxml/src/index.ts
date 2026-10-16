/**
 * @kiln-ref/ooxml — the shared Office Open XML layer: OPC container, XML (de)serialisation, units and
 * simple types, DrawingML (colours, fills, lines, effects, text properties, themes, preset geometries),
 * opaque-part passthrough and document properties.
 */
export const PACKAGE_NAME = '@kiln-ref/ooxml' as const;

export * from './drawingml/color.ts';
export * from './drawingml/effects.ts';
export * from './drawingml/fill.ts';
export * from './drawingml/geometry.ts';
export * from './drawingml/line.ts';
export * from './drawingml/presets/index.ts';
export * from './drawingml/text.ts';
export * from './drawingml/theme.ts';
export * from './opc/content-types.ts';
export * from './opc/package.ts';
export * from './opc/part-name.ts';
export * from './opc/rel-types.ts';
export * from './opc/relationships.ts';
export * from './passthrough.ts';
export * from './props.ts';
export * from './st.ts';
export * from './units.ts';
export * from './xml/build.ts';
export * from './xml/namespaces.ts';
export * from './xml/node.ts';
export * from './xml/parse.ts';
export * from './xml/query.ts';
