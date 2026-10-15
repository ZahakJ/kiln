/**
 * Conversion from `RunProps` (as read from `w:rPr`) to schema marks — the inverse of
 * `runPropsFromMarks`. Properties without a dedicated mark are kept in a `runProps` mark so they
 * survive editing and round-trip to DOCX.
 */
import type { Mark, Schema } from '@tiptap/pm/model';
import { docSchema } from './schema.ts';
import type { RunProps } from './types.ts';

const TOGGLE_MARKS = ['bold', 'italic', 'strike', 'dstrike', 'caps', 'smallCaps', 'hidden'] as const;

/** Keys with dedicated marks; everything else goes into the `runProps` mark. */
const DEDICATED = new Set<string>([
  ...TOGGLE_MARKS,
  'styleId',
  'underline',
  'vertAlign',
  'color',
  'highlight',
  'shading',
  'fonts',
  'size',
  'sizeCs',
  'spacing',
]);

export function marksFromRunProps(props: RunProps, schema: Schema = docSchema): Mark[] {
  const m = schema.marks;
  let marks: readonly Mark[] = [];
  const add = (name: string, attrs?: Record<string, unknown>) => {
    const type = m[name];
    if (type) marks = type.create(attrs).addToSet(marks);
  };
  if (props.styleId) add('runStyle', { styleId: props.styleId });
  for (const t of TOGGLE_MARKS) if (props[t] !== undefined) add(t, { value: props[t] });
  if (props.underline)
    add('underline', { style: props.underline.style, color: props.underline.color ?? null });
  if (props.vertAlign) add('vertAlign', { value: props.vertAlign });
  if (props.color) add('color', { color: props.color });
  if (props.highlight) add('highlight', { color: props.highlight });
  if (props.shading) add('runShading', { shading: props.shading });
  if (props.fonts) add('fontFamily', { fonts: props.fonts });
  if (props.size !== undefined || props.sizeCs !== undefined)
    add('fontSize', { size: props.size ?? props.sizeCs, sizeCs: props.sizeCs ?? null });
  if (props.spacing !== undefined) add('charSpacing', { value: props.spacing });
  const rest: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(props)) if (!DEDICATED.has(k) && v !== undefined) rest[k] = v;
  if (Object.keys(rest).length) add('runProps', { props: rest });
  return [...marks];
}
