/**
 * Layout scan (PLAN.md §5, WP-97): finds the layout defects a screenshot review catches — text cut
 * off without an ellipsis, elements pushed outside the viewport (horizontal scrolling), interactive
 * controls overlapping each other, and touch targets smaller than 24 × 24 CSS px on touch
 * viewports. The analysis is a pure function over element boxes, so it is unit-tested without a
 * browser; `collectLayout` gathers the boxes from a page.
 */

/** What the scan knows about one rendered element. */
export interface ElementBox {
  /** A selector-like label for reports (`button[aria-label="Bold"]`). */
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
  /** A button, link, input or element with an interactive role. */
  interactive: boolean;
  /** The element shows text directly. */
  hasText: boolean;
  /** `scrollWidth > clientWidth` while `overflow-x` hides the overflow. */
  clipsText: boolean;
  /** `text-overflow: ellipsis` is set (clipping is then intended). */
  ellipsis: boolean;
  /** Visible (non-zero size, not `visibility: hidden`, not `display: none`). */
  visible: boolean;
}

export type LayoutIssueKind = 'clipped-text' | 'outside-viewport' | 'overlap' | 'small-target';

export interface LayoutIssue {
  kind: LayoutIssueKind;
  label: string;
  detail: string;
}

export interface LayoutScanOptions {
  viewport: { width: number; height: number };
  /** Check touch target sizes (phone and tablet viewports). */
  touch?: boolean;
  /** Minimum touch target side in CSS px (default 24, WCAG 2.2 target size minimum). */
  minTarget?: number;
  /** Overlap below this many CSS px in either direction is ignored (default 1). */
  overlapTolerance?: number;
}

function intersection(a: ElementBox, b: ElementBox): { w: number; h: number } {
  const w = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return { w, h };
}

function contains(outer: ElementBox, inner: ElementBox): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

/** Finds layout issues among element boxes. */
export function analyzeLayout(boxes: readonly ElementBox[], options: LayoutScanOptions): LayoutIssue[] {
  const issues: LayoutIssue[] = [];
  const tolerance = options.overlapTolerance ?? 1;
  const minTarget = options.minTarget ?? 24;
  const visible = boxes.filter((b) => b.visible && b.width > 0 && b.height > 0);
  for (const b of visible) {
    if (b.hasText && b.clipsText && !b.ellipsis) {
      issues.push({ kind: 'clipped-text', label: b.label, detail: 'text is cut off without an ellipsis' });
    }
    if (b.x + b.width > options.viewport.width + tolerance || b.x < -tolerance) {
      issues.push({
        kind: 'outside-viewport',
        label: b.label,
        detail: `spans x ${Math.round(b.x)}–${Math.round(b.x + b.width)} in a ${options.viewport.width}px viewport`,
      });
    }
    if (options.touch && b.interactive && (b.width < minTarget || b.height < minTarget)) {
      issues.push({
        kind: 'small-target',
        label: b.label,
        detail: `${Math.round(b.width)}×${Math.round(b.height)} px is below ${minTarget}×${minTarget}`,
      });
    }
  }
  const controls = visible.filter((b) => b.interactive);
  for (let i = 0; i < controls.length; i++) {
    for (let j = i + 1; j < controls.length; j++) {
      const a = controls[i] as ElementBox;
      const b = controls[j] as ElementBox;
      // A control nested inside another (an icon button inside a split button) is not an overlap.
      if (contains(a, b) || contains(b, a)) continue;
      const { w, h } = intersection(a, b);
      if (w > tolerance && h > tolerance) {
        issues.push({
          kind: 'overlap',
          label: a.label,
          detail: `overlaps ${b.label} by ${Math.round(w)}×${Math.round(h)} px`,
        });
      }
    }
  }
  return issues;
}

/** The minimal page surface `collectLayout` needs (structurally a Playwright `Page`). */
export interface EvaluatesInPage {
  evaluate<R, A>(fn: (arg: A) => R | Promise<R>, arg: A): Promise<R>;
}

/** Gathers the boxes of every visible element under `root` (default `body`). */
export function collectLayout(page: EvaluatesInPage, root = 'body'): Promise<ElementBox[]> {
  return page.evaluate((rootSelector: string) => {
    const interactiveRoles = new Set([
      'button',
      'link',
      'menuitem',
      'tab',
      'checkbox',
      'radio',
      'switch',
      'option',
    ]);
    const describe = (el: Element): string => {
      const aria = el.getAttribute('aria-label');
      const testId = el.getAttribute('data-testid');
      const tag = el.tagName.toLowerCase();
      if (testId) return `${tag}[data-testid="${testId}"]`;
      if (aria) return `${tag}[aria-label="${aria}"]`;
      return el.id ? `${tag}#${el.id}` : tag;
    };
    const out: Array<{
      label: string;
      x: number;
      y: number;
      width: number;
      height: number;
      interactive: boolean;
      hasText: boolean;
      clipsText: boolean;
      ellipsis: boolean;
      visible: boolean;
    }> = [];
    const scope = document.querySelector(rootSelector) ?? document.body;
    for (const el of Array.from(scope.querySelectorAll('*'))) {
      const style = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      const tag = el.tagName.toLowerCase();
      const role = el.getAttribute('role') ?? '';
      const hasText = Array.from(el.childNodes).some(
        (n) => n.nodeType === Node.TEXT_NODE && (n.textContent ?? '').trim().length > 0,
      );
      const hides = style.overflowX === 'hidden' || style.overflowX === 'clip';
      out.push({
        label: describe(el),
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
        interactive:
          ['button', 'a', 'input', 'select', 'textarea'].includes(tag) || interactiveRoles.has(role),
        hasText,
        clipsText: hides && (el as HTMLElement).scrollWidth > (el as HTMLElement).clientWidth + 1,
        ellipsis: style.textOverflow === 'ellipsis',
        visible: style.visibility !== 'hidden' && style.display !== 'none' && r.width > 0 && r.height > 0,
      });
    }
    return out;
  }, root);
}

/** Collects and analyses a page in one call. */
export async function layoutScan(
  page: EvaluatesInPage,
  options: LayoutScanOptions & { root?: string },
): Promise<LayoutIssue[]> {
  return analyzeLayout(await collectLayout(page, options.root), options);
}
