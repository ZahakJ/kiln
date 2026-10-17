/**
 * The grid surface component: the canvas, the native scroller over it, the input proxy and the
 * accessibility mirror, driven by a {@link GridSurfaceController}.
 */
import type { KilnClient } from '@kiln/protocol';
import { useEffect, useRef } from 'react';
import { type GridElements, GridSurfaceController } from './controller.ts';
import type { GridEditorModel } from './model.ts';

export interface GridSurfaceProps {
  client: KilnClient;
  model: GridEditorModel;
  theme: 'light' | 'dark';
  /** Blink the caret (screenshots turn it off). */
  blink?: boolean;
  /** Accessible name of the surface. */
  label: string;
  /** The controller, once it runs (tests, the e2e handle). */
  onController?: (c: GridSurfaceController | null) => void;
}

export function GridSurface({ client, model, theme, blink = true, label, onController }: GridSurfaceProps) {
  const root = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const spacer = useRef<HTMLDivElement>(null);
  const proxy = useRef<HTMLTextAreaElement>(null);
  const a11y = useRef<HTMLDivElement>(null);
  const controller = useRef<GridSurfaceController | null>(null);
  const report = useRef(onController);
  report.current = onController;

  useEffect(() => {
    const [r, cv, sc, sp, px, ay] = [
      root.current,
      canvas.current,
      scroller.current,
      spacer.current,
      proxy.current,
      a11y.current,
    ];
    if (!r || !cv || !sc || !sp || !px || !ay) return;
    const els: GridElements = { root: r, canvas: cv, scroller: sc, spacer: sp, proxy: px, a11y: ay };
    const c = new GridSurfaceController(client, model, els, {
      theme,
      blink,
    });
    controller.current = c;
    model.surface = c;
    c.start();
    report.current?.(c);
    // The grid has the keyboard when the editor opens.
    c.focus();
    return () => {
      report.current?.(null);
      if (model.surface === c) model.surface = null;
      c.dispose();
      controller.current = null;
    };
    // The controller lives as long as the document; the theme follows below.
  }, [client, model]);

  useEffect(() => {
    controller.current?.setOptions({ theme, blink });
  }, [theme, blink]);

  return (
    <div ref={root} className="kiln-grid-surface" data-kiln-grid-surface="">
      <canvas ref={canvas} className="kiln-grid-surface__canvas" />
      <div ref={scroller} className="kiln-grid-surface__scroller">
        <div ref={spacer} className="kiln-grid-surface__spacer" />
      </div>
      <textarea
        ref={proxy}
        className="kiln-grid-surface__proxy"
        aria-label={label}
        aria-multiline="true"
        autoCapitalize="off"
        autoComplete="off"
        spellCheck={false}
        wrap="off"
        data-kiln-grid-proxy=""
      />
      <div ref={a11y} className="kiln-grid-surface__a11y" />
    </div>
  );
}
