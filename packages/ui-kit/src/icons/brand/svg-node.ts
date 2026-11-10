/** A compact SVG element tree: `[tag, attributes, children]` (attribute names as in SVG markup). */
export type SvgNode = readonly [
  tag: string,
  attrs: Readonly<Record<string, string>>,
  children: readonly SvgNode[],
];
