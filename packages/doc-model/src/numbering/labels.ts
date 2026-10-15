/**
 * List label computation over a whole document, in document order (body text including table cells;
 * headers, footers and notes are excluded as Word numbers those stories separately).
 */
import type { Node as PMNode } from '@tiptap/pm/model';
import {
  type DocContext,
  effectiveNumbering,
  forEachBodyParagraph,
  paragraphPropsFromNode,
} from '../styles/resolve.ts';
import { ListCounter, type ListLabel } from './numbering.ts';

export interface ParagraphListLabel extends ListLabel {
  /** Position of the paragraph node in the document. */
  pos: number;
}

/**
 * Computes the list label of every numbered paragraph. Returns a map keyed by the paragraph's
 * document position, iterating in document order.
 */
export function computeListLabels(doc: PMNode, ctx: DocContext): Map<number, ParagraphListLabel> {
  const out = new Map<number, ParagraphListLabel>();
  if (!ctx.numbering) return out;
  const counter = new ListCounter(ctx.numbering);
  forEachBodyParagraph(doc, (paragraph, pos) => {
    const num = effectiveNumbering(ctx, paragraphPropsFromNode(paragraph));
    if (!num) return;
    const label = counter.next(num.numId, num.level);
    if (label) out.set(pos, { ...label, pos });
  });
  return out;
}

/** Labels for a sequence of paragraphs not in a document (e.g. a fragment or a test). */
export function computeLabelsForParagraphs(
  paragraphs: readonly PMNode[],
  ctx: DocContext,
): (ListLabel | undefined)[] {
  if (!ctx.numbering) return paragraphs.map(() => undefined);
  const counter = new ListCounter(ctx.numbering);
  return paragraphs.map((p) => {
    const num = effectiveNumbering(ctx, paragraphPropsFromNode(p));
    return num ? counter.next(num.numId, num.level) : undefined;
  });
}
