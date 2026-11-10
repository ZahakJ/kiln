/**
 * The stage component: the canvas, the input proxy and the accessibility mirror, driven by a
 * {@link DeckStageController}.
 */
import { t } from '@kiln/i18n';
import type { KilnClient } from '@kiln/protocol';
import { useEffect, useRef } from 'react';
import type { DeckEditorModel } from './model.ts';
import { DeckStageController } from './stage.ts';

export interface DeckStageProps {
  client: KilnClient;
  model: DeckEditorModel;
  theme: 'light' | 'dark';
  /** Blink the caret (screenshots turn it off). */
  blink?: boolean;
  /** Accessible name of the stage. */
  label: string;
  /** The controller, once it runs (tests, the e2e handle). */
  onController?: (c: DeckStageController | null) => void;
}

export function DeckStage({ client, model, theme, blink = true, label, onController }: DeckStageProps) {
  const root = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const proxy = useRef<HTMLTextAreaElement>(null);
  const a11y = useRef<HTMLDivElement>(null);
  const controller = useRef<DeckStageController | null>(null);
  const report = useRef(onController);
  report.current = onController;

  useEffect(() => {
    const [r, cv, px, ay] = [root.current, canvas.current, proxy.current, a11y.current];
    if (!r || !cv || !px || !ay) return;
    const c = new DeckStageController(
      client,
      model,
      { root: r, canvas: cv, proxy: px, a11y: ay },
      { theme, blink },
    );
    controller.current = c;
    model.stage = c;
    c.start();
    report.current?.(c);
    c.focus();
    return () => {
      report.current?.(null);
      if (model.stage === c) model.stage = null;
      c.dispose();
      controller.current = null;
    };
    // The controller lives as long as the document; the theme follows below.
  }, [client, model]);

  useEffect(() => {
    controller.current?.setOptions({ theme, blink });
  }, [theme, blink]);

  return (
    // The slide canvas keeps its own direction, whatever the interface language.
    <div ref={root} className="kiln-deckstage" data-kiln-deck-stage="" data-testid="deck-stage" dir="ltr">
      <canvas ref={canvas} className="kiln-deckstage__canvas" />
      <textarea
        ref={proxy}
        className="kiln-deckstage__proxy"
        aria-label={label}
        aria-roledescription={t('slide')}
        aria-multiline="true"
        autoCapitalize="off"
        autoComplete="off"
        spellCheck={false}
        data-kiln-deck-proxy=""
      />
      <div ref={a11y} className="kiln-deckstage__a11y" aria-live="polite" />
    </div>
  );
}
