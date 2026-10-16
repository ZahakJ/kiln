import type { A11yNode, A11yTree, DocId } from '@kiln/protocol';
import type { CSSProperties, ReactNode } from 'react';

/** Visually hidden but present for assistive technology (never `display: none`). */
export const VISUALLY_HIDDEN: CSSProperties = {
  position: 'absolute',
  width: 1,
  height: 1,
  margin: -1,
  padding: 0,
  border: 0,
  overflow: 'hidden',
  clip: 'rect(0 0 0 0)',
  clipPath: 'inset(50%)',
  whiteSpace: 'pre-wrap',
};

/** The DOM id of a mirrored node. */
export function mirrorId(doc: DocId, node: number): string {
  return `kiln-a11y-${doc}-${node}`;
}

function renderNode(doc: DocId, node: A11yNode, byId: Map<number, A11yNode>, depth: number): ReactNode {
  const id = mirrorId(doc, node.id);
  const children = depth > 32 ? [] : node.children.map((c) => byId.get(c)).filter((c): c is A11yNode => !!c);
  const kids = children.map((c) => renderNode(doc, c, byId, depth + 1));
  switch (node.role) {
    case 'Heading': {
      const level = Math.min(6, Math.max(1, node.level ?? 1));
      const Tag = `h${level}` as 'h1';
      return (
        <Tag key={id} id={id}>
          {node.text}
        </Tag>
      );
    }
    case 'Paragraph':
      return (
        <p key={id} id={id}>
          {node.text}
        </p>
      );
    case 'List':
      return (
        <ul key={id} id={id}>
          {kids}
        </ul>
      );
    case 'ListItem':
      return (
        <li key={id} id={id}>
          {node.text}
          {kids}
        </li>
      );
    case 'Image':
      return <span key={id} id={id} role="img" aria-label={node.name ?? ''} />;
    default:
      return node.name ? (
        <fieldset key={id} id={id} aria-label={node.name}>
          {node.text}
          {kids}
        </fieldset>
      ) : (
        <div key={id} id={id}>
          {node.text}
          {kids}
        </div>
      );
  }
}

/**
 * The accessibility mirror (PLAN.md §3.6): the core's tree of the visible pages as an off-screen
 * DOM subtree next to the canvas, so screen readers read the document; the input proxy points at
 * the active node with `aria-activedescendant`, and announcements go to a polite live region.
 */
export function A11yMirror({ doc, tree }: { doc: DocId; tree: A11yTree | null }) {
  const root = tree?.nodes[0];
  const byId = new Map<number, A11yNode>((tree?.nodes ?? []).map((n) => [n.id, n]));
  const children = (root?.children ?? []).map((c) => byId.get(c)).filter((c): c is A11yNode => !!c);
  return (
    <>
      <div
        role="document"
        aria-label={root?.name ?? undefined}
        data-testid="kiln-a11y-mirror"
        style={VISUALLY_HIDDEN}
      >
        {children.map((n) => renderNode(doc, n, byId, 0))}
      </div>
      <div aria-live="polite" aria-atomic="true" data-testid="kiln-live" style={VISUALLY_HIDDEN}>
        {tree?.announce ?? ''}
      </div>
    </>
  );
}
