/** Small builders for doc-model tests. */
import type { Mark, Node as PMNode } from '@tiptap/pm/model';
import { EditorState, TextSelection } from '@tiptap/pm/state';
import { marksFromRunProps } from '../src/marks.ts';
import { docSchema as schema } from '../src/schema.ts';
import { defaultSectionProps } from '../src/sections.ts';
import type { ParagraphProps, RunProps } from '../src/types.ts';

export { schema };

type Inline = PMNode | string;

export function text(value: string, props?: RunProps): PMNode {
  return schema.text(value, props ? marksFromRunProps(props) : undefined);
}

export function p(attrsOrContent?: ParagraphProps | Inline, ...content: Inline[]): PMNode {
  let attrs: ParagraphProps = {};
  const kids: Inline[] = [];
  if (
    typeof attrsOrContent === 'string' ||
    (attrsOrContent && 'type' in attrsOrContent && 'nodeSize' in attrsOrContent)
  ) {
    kids.push(attrsOrContent as Inline);
  } else if (attrsOrContent) {
    attrs = attrsOrContent;
  }
  kids.push(...content);
  return schema.node(
    'paragraph',
    attrs,
    kids.filter((k) => k !== '').map((k) => (typeof k === 'string' ? schema.text(k) : k)),
  );
}

export function cell(...blocks: PMNode[]): PMNode {
  return schema.node('tableCell', null, blocks.length ? blocks : [p()]);
}

export function row(...cells: PMNode[]): PMNode {
  return schema.node('tableRow', null, cells);
}

export function table(attrs: Record<string, unknown>, ...rows: PMNode[]): PMNode {
  return schema.node('table', attrs, rows);
}

export function section(...blocks: PMNode[]): PMNode {
  return schema.node('section', { props: defaultSectionProps() }, blocks);
}

export function doc(...blocks: PMNode[]): PMNode {
  if (blocks.length && blocks[0]?.type.name === 'section') return schema.node('doc', null, blocks);
  return schema.node('doc', null, [section(...blocks)]);
}

/** Position of the first occurrence of `needle` in the document text (inside its text node). */
export function posOf(d: PMNode, needle: string): number {
  let found = -1;
  d.descendants((n, pos) => {
    if (found >= 0) return false;
    if (n.isText && n.text?.includes(needle)) {
      found = pos + (n.text.indexOf(needle) ?? 0);
      return false;
    }
    return true;
  });
  if (found < 0) throw new Error(`text not found: ${needle}`);
  return found;
}

/** Editor state with the selection spanning `needle` (or a cursor before it when `cursor`). */
export function stateWith(
  d: PMNode,
  needle?: string,
  opts: { cursor?: boolean; to?: string } = {},
): EditorState {
  let state = EditorState.create({ doc: d, schema });
  if (needle) {
    const from = posOf(d, needle);
    const to = opts.cursor ? from : opts.to ? posOf(d, opts.to) + opts.to.length : from + needle.length;
    state = state.apply(state.tr.setSelection(TextSelection.create(d, from, to)));
  }
  return state;
}

/** Applies a command and returns the new state (throws if the command was not applicable). */
export function run(
  state: EditorState,
  cmd: (s: EditorState, d?: (tr: import('@tiptap/pm/state').Transaction) => void) => boolean,
): EditorState {
  let next: EditorState | undefined;
  const ok = cmd(state, (tr) => {
    next = state.apply(tr);
  });
  if (!ok || !next) throw new Error('command not applicable');
  return next;
}

export const markNames = (marks: readonly Mark[]): string[] => marks.map((m) => m.type.name).sort();

/** All paragraphs in document order. */
export function paragraphs(d: PMNode): PMNode[] {
  const out: PMNode[] = [];
  d.descendants((n) => {
    if (n.type.name === 'paragraph') out.push(n);
    return true;
  });
  return out;
}
