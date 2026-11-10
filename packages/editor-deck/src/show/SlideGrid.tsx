/**
 * See All Slides: the slides of the show as thumbnails over the show (and Presenter View's slide
 * grid); a click goes to the slide. Thumbnails are the core's (`Query::Deck(Thumbnails)`).
 */
import { readingStep, t } from '@kiln/i18n';
import type { DocId, KilnClient } from '@kiln/protocol';
import { useEffect, useRef, useState } from 'react';
import { deckResult } from '../model.ts';
import { paintScene } from '../thumbs.ts';

/** Thumbnails of slides by slide index, `width` CSS px wide. */
export function useSlideThumbs(
  client: KilnClient,
  doc: DocId,
  indices: readonly number[],
  width: number,
): Map<number, CanvasImageSource & { width: number; height: number }> {
  const [thumbs, setThumbs] = useState(
    new Map<number, CanvasImageSource & { width: number; height: number }>(),
  );
  const key = indices.join(',');
  useEffect(() => {
    let live = true;
    const list = key === '' ? [] : key.split(',').map(Number);
    if (list.length === 0) return;
    const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
    const sorted = [...new Set(list)].sort((a, b) => a - b);
    // The core draws at most 64 thumbnails a time: ask in runs.
    const runs: [number, number][] = [];
    for (const i of sorted) {
      const last = runs.at(-1);
      if (last && i - last[0] < 64) last[1] = i;
      else runs.push([i, i]);
    }
    void (async () => {
      const out = new Map<number, CanvasImageSource & { width: number; height: number }>();
      for (const [from, to] of runs) {
        const r = await client
          .query(doc, { Deck: { Thumbnails: { from, count: to - from + 1, width, dpr } } })
          .catch(() => null);
        const d = r ? deckResult(r) : null;
        if (!live) return;
        if (!d || !('Scene' in d)) continue;
        for (const [i, c] of paintScene(d.Scene, 'light')) if (list.includes(i)) out.set(i, c);
      }
      if (live) setThumbs(out);
    })();
    return () => {
      live = false;
    };
  }, [client, doc, key, width]);
  return thumbs;
}

/** A thumbnail drawn into a canvas of its own. */
export function ThumbCanvas({
  source,
  label,
  className,
}: {
  source: (CanvasImageSource & { width: number; height: number }) | undefined;
  label: string;
  className?: string;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c || !source) return;
    c.width = source.width;
    c.height = source.height;
    c.getContext('2d')?.drawImage(source, 0, 0);
  }, [source]);
  return <canvas ref={ref} className={className} role="img" aria-label={label} />;
}

export function SlideGrid({
  client,
  doc,
  current,
  onPick,
  onClose,
}: {
  client: KilnClient;
  doc: DocId;
  current: number;
  onPick: (index: number) => void;
  onClose: () => void;
}) {
  const [slides, setSlides] = useState<number[]>([]);
  useEffect(() => {
    let live = true;
    void client
      .query(doc, { Deck: 'ShowInfo' })
      .then((r) => {
        const d = deckResult(r);
        if (live && d && 'ShowInfo' in d) setSlides(d.ShowInfo.slides);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [client, doc]);
  const thumbs = useSlideThumbs(client, doc, slides, 220);
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    first.current?.focus();
  }, [slides.length]);
  return (
    <div
      className="kiln-show__grid"
      role="dialog"
      aria-label={t('All slides')}
      data-testid="deck-show-grid"
      data-kiln-shortcuts="off"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          onClose();
          return;
        }
        // Left and Right move between slides as Tab and Shift+Tab do.
        const by = readingStep(e.key, e.currentTarget);
        if (by === 0) return;
        const items = [...e.currentTarget.querySelectorAll<HTMLButtonElement>('.kiln-show__grid-item')];
        const at = items.indexOf(e.target as HTMLButtonElement);
        const to = items[Math.max(0, Math.min(items.length - 1, at < 0 ? 0 : at + by))];
        if (to) {
          e.preventDefault();
          to.focus();
        }
      }}
    >
      <div className="kiln-show__grid-head">
        <span>{t('All slides')}</span>
        <button type="button" className="kiln-show__btn" onClick={onClose} aria-label={t('Close')}>
          ✕
        </button>
      </div>
      <div className="kiln-show__grid-list">
        {slides.map((s, k) => (
          <button
            key={s}
            ref={s === current || (k === 0 && !slides.includes(current)) ? first : undefined}
            type="button"
            className="kiln-show__grid-item"
            data-current={s === current || undefined}
            onClick={() => onPick(s)}
          >
            <ThumbCanvas
              source={thumbs.get(s)}
              label={t('Slide {value}', { value: s + 1 })}
              className="kiln-show__grid-thumb"
            />
            <span>{s + 1}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
