/**
 * Slide Master view's rail: each master and, under it, its layouts, drawn by the core
 * (`DeckQuery::MasterThumbnails`); the one on the stage ringed. A click shows it on the stage,
 * the arrows go through them, and the tooltip says which slides use a layout.
 */
import { t } from '@kiln/i18n';
import type { DeckMasterItem } from '@kiln/protocol';
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { paintScene, type Thumb } from '../thumbs.ts';

/** CSS px of a master's thumbnail (a layout's is smaller, under it). */
const MASTER_WIDTH = 168;
const LAYOUT_WIDTH = 136;

function Canvas({
  source,
  width,
  height,
}: {
  source: Thumb['canvas'] | undefined;
  width: number;
  height: number;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !source) return;
    el.width = source.width;
    el.height = source.height;
    el.getContext('2d')?.drawImage(source, 0, 0);
  }, [source]);
  return <canvas ref={ref} className="kiln-deckrail__canvas" style={{ width, height }} />;
}

/** What a thumbnail's tooltip says. */
export function usedBy(item: DeckMasterItem): string {
  const kind = item.master ? 'Slide Master' : `${item.name} Layout`;
  if (item.used === 0) return `${kind}: used by no slides`;
  return `${kind}: used by ${item.used} slide${item.used === 1 ? '' : 's'}`;
}

export function MasterRail({ model, theme }: { model: DeckEditorModel; theme: 'light' | 'dark' }) {
  const view = useDeck(model, (s) => s.deck?.master_view ?? null);
  const aspect = useDeck(model, (s) =>
    s.deck ? s.deck.slide_height / Math.max(1, s.deck.slide_width) : 9 / 16,
  );
  const [thumbs, setThumbs] = useState<Map<number, Thumb['canvas']>>(new Map());
  const list = useRef<HTMLDivElement>(null);
  const key = view ? view.items.map((i) => `${i.id}:${i.version}`).join(',') : '';
  useEffect(() => {
    if (!key) return;
    let live = true;
    const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
    void model.query({ MasterThumbnails: { width: MASTER_WIDTH, dpr } }).then((r) => {
      const scene = r && 'Scene' in r ? r.Scene : null;
      if (live && scene) setThumbs(paintScene(scene, theme));
    });
    return () => {
      live = false;
    };
  }, [model, key, theme]);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>('[aria-current="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [view?.current]);
  if (!view) return null;
  const select = (id: number) => void model.core('deck.master.select', { id });
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const i = view.items.findIndex((x) => x.id === view.current);
    const d = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = view.items[Math.min(view.items.length - 1, Math.max(0, i + d))];
    if (next) select(next.id);
  };
  return (
    <div
      ref={list}
      className="kiln-deckrail kiln-deckmasterrail"
      // An F6 stop, as the slide thumbnails are.
      data-kiln-region="pane"
      role="listbox"
      aria-label={t('Slide masters and layouts')}
      tabIndex={0}
      data-testid="deck-master-rail"
      onKeyDown={onKey}
    >
      {view.items.map((item, i) => {
        const width = item.master ? MASTER_WIDTH : LAYOUT_WIDTH;
        const current = item.id === view.current;
        return (
          <div
            key={item.id}
            className="kiln-deckrail__item"
            data-master={item.master || undefined}
            data-current={current || undefined}
            role="option"
            aria-selected={current}
            aria-current={current ? 'true' : undefined}
            aria-label={usedBy(item)}
            tabIndex={-1}
            title={usedBy(item)}
            data-testid="deck-master-item"
            onPointerDown={(e) => {
              if (e.button === 0) select(item.id);
            }}
          >
            <span className="kiln-deckrail__number">
              {item.master ? view.items.filter((x, k) => x.master && k <= i).length : ''}
            </span>
            <div className="kiln-deckrail__thumb" style={{ width, height: width * aspect }}>
              <Canvas source={thumbs.get(i)} width={width} height={width * aspect} />
            </div>
          </div>
        );
      })}
    </div>
  );
}
