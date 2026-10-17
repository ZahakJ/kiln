/**
 * The feature layer over Kiln Write's surface (WP-66): comment cards in the markup area beside
 * the page, the new-comment composer, the frame and handles of a selected picture, shape or text
 * box (drag to move, handles to resize — `write.arrange.transform`), focus mode's veils and
 * typewriter scrolling, and the pointer and key behaviour of Format Painter, Ctrl+click on links,
 * the spelling context menu, Ctrl+Alt+F and Ctrl+Alt+D. It also keeps the core in step with the
 * user's options (reviewer name, AutoFormat, spelling) and, in the web build, gives the core its
 * spelling dictionary. Geometry comes from the core in document pixels; this layer only maps it
 * to the screen.
 */
import type { CommentInfo, FeatureState, NodeId, ObjectInfo, Rect } from '@kiln/protocol';
import type { SurfaceController } from '@kiln/surface';
import { Icon, useOfficeOptions } from '@kiln/ui-kit';
import { MessageSquare } from 'lucide-react';
import {
  type CSSProperties,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useStore } from 'zustand';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';
import { CommentBox, CommentCard, threadsOf } from './Comments.tsx';
import { ensureDictionary, loadUserWords } from './dictionary.ts';
import { featuresOf } from './model.ts';

const GAP = 8;

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Client rect of a document rect, relative to the layer. */
function toLayer(controller: SurfaceController, layer: HTMLElement | null, r: Rect): Box | null {
  if (!layer) return null;
  const c = controller.clientRectOf(r);
  const base = layer.getBoundingClientRect();
  return { left: c.left - base.left, top: c.top - base.top, width: c.width, height: c.height };
}

function useFeatureState(model: WriteEditorModel): FeatureState | null {
  return useWrite(model, (s) => s.format?.features ?? null);
}

// ── Comment cards ────────────────────────────────────────────────────────────────────────────

/** A card's width: up to 300 px, never past the layer's right edge (a pane may be open). */
function cardWidth(layer: HTMLElement | null, left: number, wanted: number): number {
  const room = layer ? layer.clientWidth - left - 12 : wanted;
  return Math.max(160, Math.min(300, wanted, room));
}

function Balloons({
  model,
  controller,
  layer,
}: {
  model: WriteEditorModel;
  controller: SurfaceController;
  layer: HTMLElement | null;
}) {
  const features = featuresOf(model);
  const comments = useStore(features.store, (s) => s.comments);
  const active = useStore(features.store, (s) => s.activeComment);
  const pane = useStore(features.store, (s) => s.pane);
  useWrite(model, (s) => s.view);
  const fs = useFeatureState(model);
  const current = fs?.comment ?? null;
  const [heights, setHeights] = useState<Record<string, number>>({});
  const observer = useRef<ResizeObserver | null>(null);
  const els = useRef(new Map<string, HTMLElement>());
  useEffect(() => {
    if (typeof ResizeObserver === 'undefined') return;
    observer.current = new ResizeObserver((entries) => {
      setHeights((h) => {
        let next = h;
        for (const e of entries) {
          const id = (e.target as HTMLElement).dataset.comment;
          if (!id) continue;
          const v = Math.round((e.target as HTMLElement).offsetHeight);
          if (next[id] !== v) next = { ...next, [id]: v };
        }
        return next;
      });
    });
    return () => observer.current?.disconnect();
  }, []);
  const shown =
    !!fs && fs.show_comments && (fs.markup === 'Simple' || fs.markup === 'All') && pane !== 'comments';
  const threads = useMemo(
    () => threadsOf(comments ?? []).filter((t) => t.root.balloon && !t.root.resolved),
    [comments],
  );
  if (!shown || threads.length === 0) return null;
  // Desired places from the core; cards that grew push the next ones down.
  const placed: { id: NodeId; box: Box; thread: ReturnType<typeof threadsOf>[number] }[] = [];
  for (const t of threads) {
    const b = t.root.balloon ? toLayer(controller, layer, t.root.balloon) : null;
    if (b) placed.push({ id: t.root.id, box: b, thread: t });
  }
  placed.sort((a, b) => a.box.top - b.box.top);
  let bottom = Number.NEGATIVE_INFINITY;
  const zoom = controller.getZoom();
  // No room beside the page (a phone): a marker per thread at the edge opens the Comments pane.
  const room = layer ? layer.clientWidth - (placed[0]?.box.left ?? 0) : 300;
  if (room < 180) {
    const right = layer ? layer.clientWidth - 40 : 0;
    let last = Number.NEGATIVE_INFINITY;
    return (
      <>
        {placed.map(({ id, box, thread }) => {
          const top = Math.max(box.top, last + 34);
          last = top;
          const n = 1 + thread.replies.length;
          return (
            <button
              key={id}
              type="button"
              className={`kiln-comment-marker${active === id || current === id ? ' is-active' : ''}`}
              style={{ left: right, top }}
              data-testid="write-comment-marker"
              aria-label={`Comment by ${thread.root.author}${n > 1 ? `, ${n - 1} ${n === 2 ? 'reply' : 'replies'}` : ''}`}
              onClick={() => {
                void model.runCore('write.comment.select', { id });
                void model.run('ui.write.comments_pane');
              }}
            >
              <Icon icon={MessageSquare} size={14} />
              {n > 1 ? <span className="kiln-comment-marker__count">{n}</span> : null}
            </button>
          );
        })}
      </>
    );
  }
  return (
    <>
      {placed.map(({ id, box, thread }) => {
        const top = Math.max(box.top, bottom + GAP);
        const h = heights[String(id)] ?? box.height;
        bottom = top + h;
        const width = cardWidth(layer, box.left, box.width);
        const isActive = active === id || current === id;
        return (
          <CommentCard
            key={id}
            model={model}
            thread={thread}
            active={isActive}
            cardRef={(el) => {
              const key = String(id);
              const prev = els.current.get(key);
              if (prev && prev !== el) observer.current?.unobserve(prev);
              if (el) {
                els.current.set(key, el);
                observer.current?.observe(el);
              } else els.current.delete(key);
            }}
            style={{ left: box.left, top, width, '--zoom': zoom } as CSSProperties}
          />
        );
      })}
    </>
  );
}

/** The new-comment card, beside the selection (when the Comments pane is closed). */
function Composer({
  model,
  controller,
  layer,
}: {
  model: WriteEditorModel;
  controller: SurfaceController;
  layer: HTMLElement | null;
}) {
  const features = featuresOf(model);
  const composer = useStore(features.store, (s) => s.composer);
  const pane = useStore(features.store, (s) => s.pane);
  const selection = useWrite(model, (s) => s.selection);
  const comments = useStore(features.store, (s) => s.comments);
  useWrite(model, (s) => s.view);
  const [pageRight, setPageRight] = useState<number | null>(null);
  useEffect(() => {
    if (!composer) return;
    void model.query('Pages').then((r) => {
      if (r && 'Pages' in r) {
        const p = r.Pages.rects[r.Pages.current] ?? r.Pages.rects[0];
        if (p) setPageRight(p.x + p.width);
      }
    });
  }, [composer, model]);
  if (!composer || composer.parent !== null || pane === 'comments') return null;
  const anchor = selection?.rects[0] ?? selection?.caret?.rect ?? null;
  const column =
    (comments ?? []).find((c) => c.balloon)?.balloon?.x ?? (pageRight !== null ? pageRight + 16 : null);
  if (!anchor || column === null) return null;
  const box = toLayer(controller, layer, { x: column, y: anchor.y - 6, width: 240, height: 10 });
  if (!box) return null;
  return (
    <article
      className="kiln-comment is-active kiln-comment--new"
      style={{ left: box.left, top: box.top, width: cardWidth(layer, box.left, box.width) }}
      data-testid="write-comment-composer"
      aria-label="New comment"
    >
      <CommentBox
        label="New comment"
        placeholder="Add a comment… (Ctrl+Enter to post)"
        submitLabel="Comment"
        onSubmit={async (text) => {
          const ok = await model.run('write.comment.new', { text });
          if (ok) features.set({ composer: null });
          return ok;
        }}
        onCancel={() => {
          features.set({ composer: null });
          model.controller?.focus();
        }}
      />
    </article>
  );
}

// ── Object frame ─────────────────────────────────────────────────────────────────────────────

type Handle = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw' | 'move';
const HANDLES: readonly Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

/** The rectangle after dragging `handle` by (dx, dy) document pixels. */
export function draggedRect(r: Rect, handle: Handle, dx: number, dy: number, keepAspect: boolean): Rect {
  if (handle === 'move') return { ...r, x: r.x + dx, y: r.y + dy };
  let { x, y, width, height } = r;
  if (handle.includes('e')) width += dx;
  if (handle.includes('w')) {
    x += dx;
    width -= dx;
  }
  if (handle.includes('s')) height += dy;
  if (handle.includes('n')) {
    y += dy;
    height -= dy;
  }
  width = Math.max(8, width);
  height = Math.max(8, height);
  if (keepAspect && handle.length === 2 && r.width > 0 && r.height > 0) {
    const ratio = r.width / r.height;
    if (width / height > ratio) width = height * ratio;
    else height = width / ratio;
    if (handle.includes('w')) x = r.x + r.width - width;
    if (handle.includes('n')) y = r.y + r.height - height;
  }
  return { x, y, width, height };
}

function ObjectFrame({
  model,
  controller,
  layer,
  object,
}: {
  model: WriteEditorModel;
  controller: SurfaceController;
  layer: HTMLElement | null;
  object: ObjectInfo;
}) {
  useWrite(model, (s) => s.view);
  const [drag, setDrag] = useState<{ handle: Handle; x: number; y: number; rect: Rect } | null>(null);
  const [preview, setPreview] = useState<Rect | null>(null);
  const rect = object.rect;
  if (!rect) return null;
  const shown = preview ?? rect;
  const box = toLayer(controller, layer, shown);
  if (!box) return null;
  const floating = object.wrap !== 'InLine';
  const start = (handle: Handle) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    if (handle === 'move' && !floating) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setDrag({ handle, x: e.clientX, y: e.clientY, rect });
  };
  const move = (e: React.PointerEvent) => {
    if (!drag) return;
    const z = controller.getZoom() || 1;
    const keep = object.kind === 'Picture' ? !e.shiftKey : e.shiftKey;
    setPreview(draggedRect(drag.rect, drag.handle, (e.clientX - drag.x) / z, (e.clientY - drag.y) / z, keep));
  };
  const end = (e: React.PointerEvent) => {
    if (!drag) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    const next = preview;
    setDrag(null);
    if (
      next &&
      (Math.abs(next.x - rect.x) > 0.5 ||
        Math.abs(next.y - rect.y) > 0.5 ||
        Math.abs(next.width - rect.width) > 0.5 ||
        Math.abs(next.height - rect.height) > 0.5)
    ) {
      void model
        .run('write.arrange.transform', { id: object.id, rect: next })
        .finally(() => setPreview(null));
    } else setPreview(null);
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a positioned frame with drag handles, not a form group
    <div
      className={`kiln-object-frame${floating ? ' is-floating' : ''}${drag ? ' is-dragging' : ''}`}
      style={{
        left: box.left,
        top: box.top,
        width: box.width,
        height: box.height,
        transform: object.rotation ? `rotate(${object.rotation}deg)` : undefined,
      }}
      data-testid="write-object-frame"
      aria-label={`${object.name} selected`}
      role="group"
      onPointerDown={start('move')}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
    >
      {HANDLES.filter((h) => floating || h.length === 2).map((h) => (
        <span
          key={h}
          className={`kiln-object-frame__handle kiln-object-frame__handle--${h}`}
          data-handle={h}
          onPointerDown={start(h)}
          onPointerMove={move}
          onPointerUp={end}
          onPointerCancel={end}
        />
      ))}
      {drag ? (
        <span className="kiln-object-frame__size">
          {Math.round((shown.width * 3) / 4)} × {Math.round((shown.height * 3) / 4)} pt
        </span>
      ) : null}
    </div>
  );
}

// ── Focus mode ───────────────────────────────────────────────────────────────────────────────

function FocusVeils({
  model,
  controller,
  layer,
}: {
  model: WriteEditorModel;
  controller: SurfaceController;
  layer: HTMLElement | null;
}) {
  const caret = useWrite(model, (s) => s.selection?.caret?.rect ?? s.selection?.rects[0] ?? null);
  useWrite(model, (s) => s.view);
  if (!caret || !layer) return null;
  const box = toLayer(controller, layer, caret);
  if (!box) return null;
  const h = layer.clientHeight;
  const band = Math.max(28, box.height * 3.2);
  const top = Math.max(0, box.top + box.height / 2 - band / 2);
  return (
    <>
      <div className="kiln-focus-veil kiln-focus-veil--top" style={{ height: top }} />
      <div
        className="kiln-focus-veil kiln-focus-veil--bottom"
        style={{ top: top + band, height: Math.max(0, h - top - band) }}
      />
    </>
  );
}

// ── The layer ────────────────────────────────────────────────────────────────────────────────

export function WriteFeatureLayer({
  model,
  controller,
}: {
  model: WriteEditorModel;
  controller: SurfaceController | null;
}) {
  const features = featuresOf(model);
  const ref = useRef<HTMLDivElement>(null);
  const [, force] = useState(0);
  const fs = useFeatureState(model);
  const painter = fs?.painter ?? null;
  const focus = fs?.focus ?? false;
  const [options] = useOfficeOptions();
  // The layer element exists after the first render: render again so children can measure.
  useLayoutEffect(() => force((n) => n + 1), []);

  // The user's options: reviewer name, AutoFormat, spelling, the user dictionary.
  const proofing = options.proofing;
  const userName = options.general.userName;
  useEffect(() => {
    void model.runCore('write.review.set_reviewer', { name: userName.trim() || 'Author' });
  }, [model, userName]);
  useEffect(() => {
    void model.runCore('write.options.autoformat', {
      options: {
        markdown: proofing.markdownShortcuts,
        smart_quotes: proofing.smartQuotes,
        dashes: proofing.smartDashes,
        ellipsis: proofing.smartEllipsis,
        fractions: proofing.smartFractions,
      },
    });
  }, [
    model,
    proofing.markdownShortcuts,
    proofing.smartQuotes,
    proofing.smartDashes,
    proofing.smartEllipsis,
    proofing.smartFractions,
  ]);
  useEffect(() => {
    void model.runCore('write.spell.check_as_you_type', { on: proofing.checkSpellingAsYouType });
  }, [model, proofing.checkSpellingAsYouType]);
  useEffect(() => {
    void model.runCore('write.spell.user_dictionary', { words: loadUserWords() });
  }, [model]);
  // The web core loads the spelling dictionary from the page.
  const needsDictionary = fs !== null && !fs.dictionary;
  useEffect(() => {
    if (needsDictionary) void ensureDictionary(model);
  }, [model, needsDictionary]);

  // Focus mode: the chrome fades (CSS on the root), typewriter scrolling keeps the line centred.
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    if (focus) {
      root.dataset.kilnFocus = 'on';
      // Back to the page: the ribbon button that turned focus on would keep the ribbon lit.
      model.controller?.focus();
    } else delete root.dataset.kilnFocus;
    return () => {
      delete root.dataset.kilnFocus;
    };
  }, [focus, model]);
  const caretY = useWrite(model, (s) => (focus ? (s.selection?.caret?.rect.y ?? null) : null));
  useEffect(() => {
    if (!focus || caretY === null || !controller) return;
    const v = controller.viewState();
    const visible = v.height / (v.zoom || 1);
    controller.scrollToY(Math.max(0, caretY - visible * 0.42));
  }, [focus, caretY, controller]);

  // Pointer and keys over the surface.
  const followLink = useCallback(async () => {
    try {
      await model.client.execute(model.doc, { Write: 'FollowLink' });
      model.controller?.reveal();
    } catch {
      const link = model.state.format?.link;
      if (link && /^(https?:|mailto:)/i.test(link)) window.open(link, '_blank', 'noopener,noreferrer');
    }
  }, [model]);
  useEffect(() => {
    const surface = ref.current?.parentElement;
    if (!surface || !controller) return;
    const onUp = (e: PointerEvent) => {
      if (e.button !== 0 || (e.target as HTMLElement).closest('.kiln-comment, .kiln-object-frame')) return;
      if (e.ctrlKey || e.metaKey) {
        // Ctrl+click follows a link (after the click has placed the caret in it).
        setTimeout(() => void followLink(), 0);
        return;
      }
      if (model.state.format?.features.painter != null) {
        setTimeout(() => void model.run('write.format.paste_format'), 0);
      }
    };
    const onContext = (e: MouseEvent) => {
      const p = controller.documentPoint(e.clientX, e.clientY);
      features.set({ spellMenu: null });
      const sel = model.state.selection;
      const inside = sel?.rects.some(
        (r) => p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height,
      );
      const after = inside
        ? Promise.resolve()
        : model.client
            .execute(model.doc, { Write: { SelectAt: { point: p, extend: false, unit: 'Char' } } })
            .then(() => undefined);
      void after
        .then(() => model.client.query(model.doc, 'Selection'))
        .then((r) => {
          const kind = r && typeof r === 'object' && 'Selection' in r ? r.Selection.kind : null;
          const head = kind && typeof kind === 'object' && 'Text' in kind ? kind.Text.head : null;
          if (head) void features.spellingAt(head.block, head.offset);
        })
        .catch(() => undefined);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing || e.defaultPrevented) return;
      const ctrlAlt = (e.ctrlKey || e.metaKey) && e.altKey && !e.shiftKey;
      if (ctrlAlt && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        e.stopPropagation();
        void model.run('write.references.insert_footnote');
      } else if (ctrlAlt && e.key.toLowerCase() === 'd') {
        e.preventDefault();
        e.stopPropagation();
        void model.run('write.references.insert_endnote');
      } else if (e.key === 'Escape' && model.state.format?.features.painter != null) {
        e.preventDefault();
        e.stopPropagation();
        void model.run('write.format.cancel_painter');
      } else if (e.key === 'Escape' && model.state.format?.features.focus) {
        e.preventDefault();
        e.stopPropagation();
        void model.run('write.view.focus_mode');
      }
    };
    surface.addEventListener('pointerup', onUp, true);
    surface.addEventListener('contextmenu', onContext, true);
    surface.addEventListener('keydown', onKey, true);
    return () => {
      surface.removeEventListener('pointerup', onUp, true);
      surface.removeEventListener('contextmenu', onContext, true);
      surface.removeEventListener('keydown', onKey, true);
    };
  }, [controller, model, features, followLink]);

  const object = fs?.object ?? null;
  const layer = ref.current;
  return (
    <div
      ref={ref}
      className={`kiln-write-features${painter != null ? ' is-painting' : ''}`}
      data-testid="write-feature-layer"
      data-painter={painter === null ? undefined : painter ? 'sticky' : 'once'}
    >
      {controller && focus ? <FocusVeils model={model} controller={controller} layer={layer} /> : null}
      {controller ? <Balloons model={model} controller={controller} layer={layer} /> : null}
      {controller ? <Composer model={model} controller={controller} layer={layer} /> : null}
      {controller && object ? (
        <ObjectFrame key={object.id} model={model} controller={controller} layer={layer} object={object} />
      ) : null}
    </div>
  );
}

export type { CommentInfo };
