/**
 * Video and audio during the show: the clips of the slide on screen played over the core's
 * slide, each placed on its shape (the poster the core paints stays under it). A click on a clip
 * plays or pauses it — PowerPoint's When Clicked On — without advancing the show; a video shows
 * its controls while the pointer is over it and keeps its last frame when it ends. The clips'
 * bytes come from the core once each, as blob URLs.
 */
import { t } from '@kiln/i18n';
import type { DeckMediaItem, DocId, KilnClient } from '@kiln/protocol';
import { useEffect, useRef, useState } from 'react';
import type { ShowEngine } from './engine.ts';

export interface MediaLayerProps {
  client: KilnClient;
  doc: DocId;
  /** The slide on screen (index); `null` while no slide shows (black, white, end screens). */
  slide: number | null;
  engine: () => ShowEngine | null;
}

/** A clip's place on the screen, CSS px. */
export function mediaBox(
  item: Pick<DeckMediaItem, 'rect'>,
  fit: { left: number; top: number; k: number },
): { left: number; top: number; width: number; height: number } {
  return {
    left: fit.left + item.rect.x * fit.k,
    top: fit.top + item.rect.y * fit.k,
    width: item.rect.width * fit.k,
    height: item.rect.height * fit.k,
  };
}

/** A clip with its URL. */
interface Clip {
  item: DeckMediaItem;
  url: string;
}

async function query(client: KilnClient, doc: DocId, q: unknown): Promise<Record<string, unknown> | null> {
  try {
    const r = (await client.query(doc, { Deck: q } as never)) as unknown;
    return typeof r === 'object' && r !== null && 'Deck' in r
      ? (r as { Deck: Record<string, unknown> }).Deck
      : null;
  } catch {
    return null;
  }
}

function ClipView({ clip, fit }: { clip: Clip; fit: { left: number; top: number; k: number } }) {
  const ref = useRef<HTMLVideoElement & HTMLAudioElement>(null);
  const [started, setStarted] = useState(false);
  const { item } = clip;
  const box = mediaBox(item, fit);
  const toggle = () => {
    const el = ref.current;
    if (!el) return;
    if (el.paused) {
      if (!started || el.ended) el.currentTime = item.trim_start_ms / 1000;
      setStarted(true);
      void el.play().catch(() => undefined);
    } else {
      el.pause();
    }
  };
  // Trim from the end: stop there.
  const onTime = () => {
    const el = ref.current;
    if (!el || item.trim_end_ms <= 0 || !Number.isFinite(el.duration)) return;
    if (el.currentTime >= el.duration - item.trim_end_ms / 1000) el.pause();
  };
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();
  const video = item.kind === 'Video';
  return (
    // biome-ignore lint/a11y/useSemanticElements: the clip's area on the slide, over the show's canvas
    <div
      className="kiln-show__clip"
      data-kind={item.kind}
      data-playing={started || undefined}
      data-testid="deck-show-clip"
      role="button"
      tabIndex={-1}
      aria-label={video ? t('Play or pause the video') : t('Play or pause the audio')}
      style={{ left: box.left, top: box.top, width: box.width, height: box.height }}
      onPointerDown={stop}
      onPointerUp={stop}
      onClick={(e) => {
        e.stopPropagation();
        if (!video || !started) toggle();
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          e.stopPropagation();
          toggle();
        }
      }}
    >
      {video ? (
        // biome-ignore lint/a11y/useMediaCaption: the presentation's own clip; captions are not part of the file
        <video
          ref={ref}
          src={clip.url}
          preload="metadata"
          playsInline
          controls={started}
          onTimeUpdate={onTime}
          style={{ visibility: started ? 'visible' : 'hidden' }}
        />
      ) : (
        // biome-ignore lint/a11y/useMediaCaption: the presentation's own clip
        <audio ref={ref} src={clip.url} preload="metadata" onTimeUpdate={onTime} />
      )}
    </div>
  );
}

export function MediaLayer({ client, doc, slide, engine }: MediaLayerProps) {
  const [clips, setClips] = useState<Clip[]>([]);
  const [fit, setFit] = useState({ left: 0, top: 0, k: 1 });
  const urls = useRef(new Map<string, string>());
  useEffect(() => {
    setClips([]);
    if (slide === null) return;
    let live = true;
    void (async () => {
      const r = await query(client, doc, 'SlideMedia');
      const media = r?.SlideMedia as { slide: number; items: DeckMediaItem[] } | undefined;
      if (!live || !media || media.slide !== slide) return;
      const out: Clip[] = [];
      for (const item of media.items) {
        if (!item.part) continue;
        let url = urls.current.get(item.part);
        if (!url) {
          const b = await query(client, doc, { MediaBytes: { part: item.part } });
          const bytes = b?.Bytes as number[] | undefined;
          if (!bytes) continue;
          url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: item.mime || undefined }));
          urls.current.set(item.part, url);
        }
        out.push({ item, url });
      }
      if (live) {
        setFit(engine()?.cssFit() ?? { left: 0, top: 0, k: 1 });
        setClips(out);
      }
    })();
    return () => {
      live = false;
    };
  }, [client, doc, slide, engine]);
  // The clips follow the slide's place when the window changes size.
  useEffect(() => {
    if (clips.length === 0) return;
    const follow = () => setFit(engine()?.cssFit() ?? { left: 0, top: 0, k: 1 });
    const t = setTimeout(follow, 50);
    window.addEventListener('resize', follow);
    return () => {
      clearTimeout(t);
      window.removeEventListener('resize', follow);
    };
  }, [clips, engine]);
  useEffect(
    () => () => {
      for (const u of urls.current.values()) URL.revokeObjectURL(u);
      urls.current.clear();
    },
    [],
  );
  if (clips.length === 0) return null;
  return (
    <div className="kiln-show__media" data-testid="deck-show-media">
      {clips.map((c) => (
        <ClipView key={`${slide}:${c.item.shape}`} clip={c} fit={fit} />
      ))}
    </div>
  );
}
