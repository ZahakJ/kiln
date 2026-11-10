/**
 * The thumbnail rail (PowerPoint's slide pane; the Deck mockup): numbered thumbnails of every
 * slide, the current one ringed in the app colour, hidden slides dimmed with a struck number,
 * sections as collapsible headers. Click selects (Shift extends, Ctrl toggles), dragging moves the
 * selected slides (`deck.slides.move`), the keyboard goes through the slides, Delete deletes them
 * and Enter or Ctrl+Enter adds one; right-click opens the slide or section menu.
 */
import { t } from '@kiln/i18n';
import type { DeckSectionInfo, DeckSlideInfo } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useDeck } from './hooks.ts';
import { railKeyAction } from './keys.ts';
import type { DeckEditorModel } from './model.ts';
import { ThumbCache } from './thumbs.ts';

/** CSS px of a thumbnail in the rail. */
export const THUMB_WIDTH = 168;
/** Pointer travel that turns a press into a drag. */
const DRAG_START = 5;

/** A row of the rail: a section header or a slide. */
export type RailRow =
  | { kind: 'section'; index: number; section: DeckSectionInfo; collapsed: boolean }
  | { kind: 'slide'; index: number; slide: DeckSlideInfo };

/** The rows of the rail: sections and the slides of the expanded ones. */
export function railRows(
  slides: readonly DeckSlideInfo[],
  sections: readonly DeckSectionInfo[],
  collapsed: ReadonlySet<number>,
): RailRow[] {
  if (sections.length === 0) return slides.map((slide, index) => ({ kind: 'slide', index, slide }));
  const out: RailRow[] = [];
  // Slides before the first section (a file may have them).
  const first = sections[0]?.start ?? 0;
  for (let i = 0; i < first; i++) {
    const slide = slides[i];
    if (slide) out.push({ kind: 'slide', index: i, slide });
  }
  sections.forEach((section, index) => {
    const isCollapsed = collapsed.has(index);
    out.push({ kind: 'section', index, section, collapsed: isCollapsed });
    if (isCollapsed) return;
    for (let i = section.start; i < section.start + section.count; i++) {
      const slide = slides[i];
      if (slide) out.push({ kind: 'slide', index: i, slide });
    }
  });
  return out;
}

/**
 * Where a drag drops: the slide index the moved slides go before (0…count), from the pointer's
 * y and the rows' vertical midpoints.
 */
export function dropIndex(y: number, mids: readonly { index: number; mid: number }[], count: number): number {
  for (const m of mids) if (y < m.mid) return m.index;
  return count;
}

/** A slide's thumbnail from the cache, drawn again only when the cache has a new one for it. */
export function ThumbCanvas({
  cache,
  slide,
  width,
  height,
  revision,
}: {
  cache: ThumbCache;
  slide: DeckSlideInfo;
  width: number;
  height: number;
  revision: number;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  const drawn = useRef<unknown>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    const t = cache.get(slide.id);
    if (!el || !t || drawn.current === t.canvas) return;
    drawn.current = t.canvas;
    el.width = t.canvas.width;
    el.height = t.canvas.height;
    el.getContext('2d')?.drawImage(t.canvas, 0, 0);
    // The revision says a new thumbnail may be there.
  }, [cache, slide.id, revision]);
  return (
    <canvas
      ref={ref}
      className="kiln-deckrail__canvas"
      style={{ width, height }}
      data-version={cache.get(slide.id)?.version ?? -1}
    />
  );
}

export function ThumbnailRail({ model, theme }: { model: DeckEditorModel; theme: 'light' | 'dark' }) {
  const deck = useDeck(model, (s) => s.deck);
  const railFocused = useDeck(model, (s) => s.rail);
  const cache = useMemo(() => new ThumbCache(model, THUMB_WIDTH, theme), [model]);
  const [revision, setRevision] = useState(0);
  const [collapsed, setCollapsed] = useState<ReadonlySet<number>>(new Set());
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const press = useRef<{ x: number; y: number; index: number; id: number; dragging: boolean } | null>(null);
  const list = useRef<HTMLDivElement>(null);
  /** The rail is being focused by a press (which selects slides itself). */
  const viaPointer = useRef(false);

  useEffect(() => cache.subscribe(() => setRevision(cache.revision)), [cache]);
  useEffect(() => {
    cache.setTheme(theme);
    if (model.state.deck) void cache.update(model.state.deck.slides);
  }, [cache, theme, model]);
  useEffect(() => {
    if (deck) void cache.update(deck.slides);
  }, [cache, deck?.slides, deck]);

  // The current slide stays in view.
  useEffect(() => {
    const el = list.current?.querySelector<HTMLElement>(`[data-slide-index="${deck?.current ?? 0}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  }, [deck?.current]);

  if (!deck) return <div className="kiln-deckrail" />;
  const height = (THUMB_WIDTH * deck.slide_height) / Math.max(1, deck.slide_width);
  const rows = railRows(deck.slides, deck.sections, collapsed);
  const selected = new Set(deck.selected);

  const mids = (): { index: number; mid: number }[] => {
    const out: { index: number; mid: number }[] = [];
    for (const el of list.current?.querySelectorAll<HTMLElement>('[data-slide-index]') ?? []) {
      const r = el.getBoundingClientRect();
      out.push({ index: Number(el.dataset.slideIndex), mid: r.top + r.height / 2 });
    }
    return out;
  };

  const onPointerDown = (e: React.PointerEvent, index: number, id: number) => {
    if (e.button !== 0) return;
    // The rail takes the focus itself (not the thumbnail under the pointer).
    e.preventDefault();
    viaPointer.current = true;
    list.current?.focus({ preventScroll: true });
    viaPointer.current = false;
    model.set({ rail: true });
    press.current = { x: e.clientX, y: e.clientY, index, id, dragging: false };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    // Shift and Ctrl change the selection at once; a plain press waits (it may start a drag of
    // the selected slides).
    const mod = e.ctrlKey || e.metaKey;
    if (e.shiftKey || mod || !selected.has(index))
      void model.core('deck.slides.select', { index, extend: e.shiftKey, toggle: mod && !e.shiftKey });
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const p = press.current;
    if (!p) return;
    if (!p.dragging && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_START) return;
    p.dragging = true;
    setDrag({ from: p.index, to: dropIndex(e.clientY, mids(), deck.slides.length) });
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const p = press.current;
    press.current = null;
    if (!p) return;
    if (p.dragging) {
      const to = dropIndex(e.clientY, mids(), deck.slides.length);
      setDrag(null);
      void model.core('deck.slides.move', { to });
    } else if (!(e.shiftKey || e.ctrlKey || e.metaKey) && selected.has(p.index)) {
      void model.core('deck.slides.select', { index: p.index, extend: false, toggle: false });
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Shift+F10 or the Menu key: the slide's menu, with its section's (Rename, Move, Remove) in
    // a submenu, as a right-click on the slide and on the section header open them.
    if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
      e.preventDefault();
      e.stopPropagation();
      const r = e.currentTarget.getBoundingClientRect();
      const item = e.currentTarget.querySelector<HTMLElement>(`#kiln-slide-${deck.current}`);
      const at = item?.getBoundingClientRect() ?? r;
      const section = (deck.sections ?? []).findIndex(
        (x) => deck.current >= x.start && deck.current < x.start + x.count,
      );
      model.set({
        menu: {
          x: at.left + 16,
          y: at.top + 16,
          kind: 'rail',
          ...(section >= 0 ? { section } : {}),
        },
      });
      return;
    }
    const a = railKeyAction(e, deck.current, deck.slides.length);
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    void model.core(a.id, a.args);
  };

  const onContextMenu = (e: React.MouseEvent, kind: 'rail' | 'section', section?: number) => {
    e.preventDefault();
    e.stopPropagation();
    model.set({ menu: { x: e.clientX, y: e.clientY, kind, ...(section !== undefined ? { section } : {}) } });
  };

  return (
    <div
      ref={list}
      className="kiln-deckrail"
      role="listbox"
      aria-label={t('Slides')}
      aria-multiselectable="true"
      aria-activedescendant={`kiln-slide-${deck.current}`}
      tabIndex={0}
      // An F6 stop of its own, as PowerPoint's thumbnail pane is.
      data-kiln-region="pane"
      data-testid="deck-rail"
      data-focused={railFocused || undefined}
      onFocus={(e) => {
        if (e.target !== e.currentTarget || model.state.rail) return;
        model.set({ rail: true });
        // The selection becomes the slides (Ctrl+D, Delete and the clipboard act on them).
        if (!viaPointer.current)
          void model.core('deck.slides.select', { index: deck.current, extend: false, toggle: false });
      }}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) model.set({ rail: false });
      }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => onContextMenu(e, 'rail')}
    >
      {rows.map((row) => {
        if (row.kind === 'section') {
          const toggle = () =>
            setCollapsed((c) => {
              const n = new Set(c);
              if (n.has(row.index)) n.delete(row.index);
              else n.add(row.index);
              return n;
            });
          return (
            // biome-ignore lint/a11y/noStaticElementInteractions: the section's context menu; its button is the control
            <div
              key={`section-${row.index}`}
              className="kiln-deckrail__section"
              data-testid="deck-section"
              onContextMenu={(e) => onContextMenu(e, 'section', row.index)}
            >
              <button
                type="button"
                className="kiln-deckrail__sectionbtn"
                aria-expanded={!row.collapsed}
                onClick={toggle}
              >
                <Icon icon={row.collapsed ? ChevronRight : ChevronDown} size={12} />
                <span className="kiln-deckrail__sectionname">{row.section.name}</span>
                <span className="kiln-deckrail__sectioncount">({row.section.count})</span>
              </button>
            </div>
          );
        }
        const { slide, index } = row;
        const current = index === deck.current;
        const isSelected = selected.has(index);
        const insertBefore = drag && drag.to === index;
        const insertAfter = drag && drag.to === deck.slides.length && index === deck.slides.length - 1;
        return (
          <div
            key={slide.id}
            id={`kiln-slide-${index}`}
            className="kiln-deckrail__item"
            role="option"
            tabIndex={-1}
            aria-selected={isSelected}
            aria-current={current ? 'true' : undefined}
            aria-label={`${
              slide.title
                ? t('Slide {n, number}, {title}', { n: index + 1, title: slide.title })
                : t('Slide {n, number}', { n: index + 1 })
            }${slide.hidden ? t(', hidden') : ''}`}
            data-slide-index={index}
            data-current={current || undefined}
            data-selected={isSelected || undefined}
            data-hidden={slide.hidden || undefined}
            data-drop={insertBefore ? 'before' : insertAfter ? 'after' : undefined}
            onPointerDown={(e) => onPointerDown(e, index, slide.id)}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={() => {
              press.current = null;
              setDrag(null);
            }}
            onDoubleClick={() => model.stage?.focus()}
            onContextMenu={(e) => {
              // The menu acts on the slide under the pointer (and the selection it is part of).
              if (!selected.has(index))
                void model.core('deck.slides.select', { index, extend: false, toggle: false });
              onContextMenu(e, 'rail');
            }}
          >
            <span className="kiln-deckrail__number">{index + 1}</span>
            <div className="kiln-deckrail__thumb" style={{ width: THUMB_WIDTH, height }}>
              <ThumbCanvas
                cache={cache}
                slide={slide}
                width={THUMB_WIDTH}
                height={height}
                revision={revision}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
