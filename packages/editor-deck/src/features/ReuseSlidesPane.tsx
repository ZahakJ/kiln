/**
 * The Reuse Slides pane (Home ▸ New Slide ▸ Reuse Slides, Insert ▸ Reuse Slides): another
 * presentation's slides, drawn by the core; a click inserts one after the current slide, Insert
 * All Slides every one, on this presentation's layouts and theme (Use Destination Theme).
 */
import { t } from '@kiln/i18n';
import type { DeckReuseSlide } from '@kiln/protocol';
import { Button, Icon } from '@kiln/ui-kit';
import { FolderOpen, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { DeckEditorModel } from '../model.ts';

/** CSS px of a slide in the pane. */
const REUSE_WIDTH = 220;

interface Offered {
  index: number;
  title: string;
  url: string;
}

export function ReuseSlidesPane({ model }: { model: DeckEditorModel }) {
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<{ name: string; bytes: number[] } | null>(null);
  const [slides, setSlides] = useState<Offered[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const urls = useRef<Offered[]>([]);
  useEffect(
    () => () => {
      for (const s of urls.current) URL.revokeObjectURL(s.url);
    },
    [],
  );

  const open = async (f: File) => {
    setBusy(true);
    setProblem(null);
    const bytes = Array.from(new Uint8Array(await f.arrayBuffer()));
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const r = await model.query({ ReuseList: { bytes, width: REUSE_WIDTH, dpr } });
    for (const s of urls.current) URL.revokeObjectURL(s.url);
    if (!r || !('ReuseList' in r)) {
      urls.current = [];
      setSlides([]);
      setFile(null);
      setProblem(t('{name} could not be opened as a presentation.', { name: f.name }));
    } else {
      urls.current = r.ReuseList.map((s: DeckReuseSlide) => ({
        index: s.index,
        title: s.title,
        url: URL.createObjectURL(new Blob([new Uint8Array(s.png)], { type: 'image/png' })),
      }));
      setSlides(urls.current);
      setFile({ name: f.name, bytes });
    }
    setBusy(false);
  };
  const insert = (indices: number[]) => {
    if (file) void model.core('deck.insert.reuse_slides_from', { bytes: file.bytes, slides: indices });
  };
  const close = () => {
    model.set({ reusePane: false });
    model.stage?.focus();
  };

  return (
    <aside className="kiln-sidepane" aria-label={t('Reuse Slides')} data-testid="deck-reuse-pane">
      <div className="kiln-sidepane__head">
        <h2>{t('Reuse Slides')}</h2>
        <button
          type="button"
          className="kiln-sidepane__close"
          aria-label={t('Close Reuse Slides')}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </div>
      <div className="kiln-reusepane__open">
        <Button
          size="sm"
          icon={<Icon icon={FolderOpen} size={14} />}
          onClick={() => input.current?.click()}
          busy={busy}
        >
          {t('Browse…')}
        </Button>
        <input
          ref={input}
          type="file"
          hidden
          accept=".pptx,.potx,.pptm,.ppsx"
          data-testid="deck-reuse-file"
          onChange={(e) => {
            const f = e.currentTarget.files?.[0];
            e.currentTarget.value = '';
            if (f) void open(f);
          }}
        />
        {file ? <span className="kiln-reusepane__name">{file.name}</span> : null}
      </div>
      {problem ? (
        <p className="kiln-sidepane__empty" role="alert">
          {problem}
        </p>
      ) : null}
      {file ? (
        <>
          <p className="kiln-sidepane__note">{t('Click a slide to insert it after the current one.')}</p>
          <div className="kiln-reusepane__list">
            {slides.map((s) => (
              <button
                key={s.index}
                type="button"
                className="kiln-reusepane__slide"
                aria-label={
                  s.title
                    ? t('Insert slide {n, number}: {title}', { n: s.index + 1, title: s.title })
                    : t('Insert slide {n, number}', { n: s.index + 1 })
                }
                onClick={() => insert([s.index])}
              >
                <img src={s.url} alt="" width={REUSE_WIDTH} draggable={false} />
                <span className="kiln-reusepane__title">
                  {s.index + 1}. {s.title || t('Untitled')}
                </span>
              </button>
            ))}
          </div>
          <div className="kiln-reusepane__all">
            <Button size="sm" onClick={() => insert([])} data-testid="deck-reuse-all">
              {t('Insert All Slides')}
            </Button>
          </div>
        </>
      ) : (
        <p className="kiln-sidepane__empty">{t('Choose a presentation to see its slides.')}</p>
      )}
    </aside>
  );
}
