import { type ComponentPropsWithRef, useCallback, useEffect, useRef, useState } from 'react';
import { cx } from './cx.ts';

export interface ScrollAreaProps extends ComponentPropsWithRef<'div'> {
  /** Which axes scroll. Default `vertical`. */
  orientation?: 'vertical' | 'horizontal' | 'both';
  /** CSS max-height (vertical) of the viewport. */
  maxHeight?: number | string;
  /** Accessible name; when set the region is focusable so keyboard users can scroll it. */
  'aria-label'?: string;
}

/**
 * Scroll container with thin, theme-coloured overlay scrollbars and soft edge shadows that appear when
 * more content is hidden in that direction (panes, galleries, long menus). Native scrolling, so wheel,
 * touch, keyboard and screen-reader scrolling all keep working.
 */
export function ScrollArea({
  orientation = 'vertical',
  maxHeight,
  className,
  style,
  children,
  ref,
  ...rest
}: ScrollAreaProps) {
  const own = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ top: false, bottom: false, left: false, right: false });

  const update = useCallback(() => {
    const el = own.current;
    if (!el) return;
    const next = {
      top: el.scrollTop > 0,
      bottom: el.scrollTop + el.clientHeight < el.scrollHeight - 1,
      left: Math.abs(el.scrollLeft) > 0,
      right: Math.abs(el.scrollLeft) + el.clientWidth < el.scrollWidth - 1,
    };
    setEdges((prev) =>
      prev.top === next.top &&
      prev.bottom === next.bottom &&
      prev.left === next.left &&
      prev.right === next.right
        ? prev
        : next,
    );
  }, []);

  useEffect(() => {
    update();
    const el = own.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(update);
    ro.observe(el);
    for (const child of Array.from(el.children)) ro.observe(child);
    return () => ro.disconnect();
  }, [update]);

  const labelled = rest['aria-label'] !== undefined;
  return (
    <div
      ref={(node) => {
        own.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) ref.current = node;
      }}
      className={cx('kiln-scroll', `kiln-scroll--${orientation}`, className)}
      data-scroll-top={edges.top || undefined}
      data-scroll-bottom={edges.bottom || undefined}
      data-scroll-left={edges.left || undefined}
      data-scroll-right={edges.right || undefined}
      style={{ maxHeight, ...style }}
      onScroll={update}
      role={labelled ? 'region' : undefined}
      tabIndex={labelled ? 0 : undefined}
      {...rest}
    >
      {children}
    </div>
  );
}
