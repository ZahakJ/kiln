/**
 * Transitions ▸ Preview and Animations ▸ Preview: the core plays the slide's transition (from the
 * slide before it) or its effects one click after another, and this paints that show over the
 * slide on the stage until it ends. A click or Esc stops it.
 */
import { createDefaultKeymap } from '@kiln/commands';
import { useEffect, useMemo, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { ShowView } from '../show/ShowView.tsx';

export function PreviewOverlay({ model }: { model: DeckEditorModel }) {
  const preview = useDeck(model, (s) => s.preview);
  const current = useDeck(model, (s) => s.deck?.current ?? 0);
  const [running, setRunning] = useState(false);
  const [rect, setRect] = useState<{ left: number; top: number; width: number; height: number } | null>(null);
  const keymap = useMemo(() => createDefaultKeymap(), []);

  useEffect(() => {
    if (!preview) {
      setRunning(false);
      return;
    }
    let live = true;
    const v = model.stage?.view?.();
    setRect(v ? { left: v.left, top: v.top, width: v.width, height: v.height } : null);
    void model
      .core('deck.show.start', {
        from: current,
        mode: preview === 'transition' ? 'PreviewTransition' : 'PreviewAnimations',
      })
      .then((ok) => {
        if (!live) return;
        if (ok) setRunning(true);
        else model.set({ preview: null });
      });
    return () => {
      live = false;
    };
  }, [preview, current, model]);

  useEffect(() => {
    if (!running) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        void model.core('deck.show.end');
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [running, model]);

  if (!preview || !running || !rect) return null;
  const stop = () => {
    void model.core('deck.show.end');
  };
  return (
    <div
      className="kiln-deck__preview"
      data-testid="deck-preview"
      style={{ left: rect.left, top: rect.top, width: rect.width, height: rect.height }}
      onPointerDown={stop}
    >
      <ShowView
        client={model.client}
        doc={model.doc}
        keymap={keymap}
        send={(id, args) => model.core(id, args)}
        embedded
        onEnd={() => {
          setRunning(false);
          model.set({ preview: null });
          model.refresh();
          model.stage?.focus();
        }}
      />
    </div>
  );
}
