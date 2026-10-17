import type { A11yTree, DocId, KilnClient } from '@kiln/protocol';
import { type CSSProperties, useEffect, useRef, useState } from 'react';
import { A11yMirror, mirrorId } from './A11yMirror';
import { SurfaceController, type SurfaceOptions } from './controller';

/** Props of {@link Surface}. */
export interface SurfaceProps {
  /** The bridge to the core. */
  client: KilnClient;
  /** The document to show. */
  doc: DocId;
  /** Accessible name of the editing area (the document title). */
  label?: string;
  /** Colour theme (default: follow the system). */
  theme?: SurfaceOptions['theme'];
  /** Blink the caret (default true). */
  blink?: boolean;
  /** Focus the surface when it mounts. */
  autoFocus?: boolean;
  /** Receives the controller (tests, zoom controls). */
  onController?: (controller: SurfaceController | null) => void;
  /** Receives bridge errors. */
  onError?: (error: unknown) => void;
  /** Receives the zoom factor whenever it changes. */
  onZoom?: (zoom: number) => void;
  /** Receives scroll, zoom and size changes (rulers beside the surface). */
  onViewChange?: SurfaceOptions['onViewChange'];
  /** Ctrl+= / Ctrl+- / Ctrl+0 zoom (default true; Write gives them to subscript and superscript). */
  zoomKeys?: boolean;
}

const ROOT: CSSProperties = { position: 'relative', width: '100%', height: '100%', overflow: 'hidden' };
const CANVAS: CSSProperties = { position: 'absolute', left: 0, top: 0, pointerEvents: 'none' };
const SCROLLER: CSSProperties = {
  position: 'absolute',
  inset: 0,
  overflow: 'auto',
  touchAction: 'pan-x pan-y',
  cursor: 'text',
  userSelect: 'none',
  WebkitUserSelect: 'none',
  overscrollBehavior: 'contain',
};
const SPACER: CSSProperties = { position: 'relative' };
const PROXY: CSSProperties = {
  position: 'absolute',
  left: 0,
  top: 0,
  width: 1,
  height: 16,
  overflow: 'hidden',
  whiteSpace: 'pre',
  color: 'transparent',
  caretColor: 'transparent',
  background: 'transparent',
  outline: 'none',
  border: 0,
  padding: 0,
  margin: 0,
  opacity: 0.01,
  userSelect: 'text',
  WebkitUserSelect: 'text',
  pointerEvents: 'none',
};

/**
 * A document surface: the Scene painted on a content canvas and an overlay canvas, native
 * scrolling and zoom, a 1 × 1 px `contenteditable` input proxy at the caret (keyboard, IME,
 * clipboard), selection by pointer and touch, and the accessibility mirror.
 */
export function Surface({
  client,
  doc,
  label = 'Document',
  theme = 'system',
  blink = true,
  autoFocus = true,
  onController,
  onError,
  onZoom,
  onViewChange,
  zoomKeys = true,
}: SurfaceProps) {
  const root = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const spacer = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLCanvasElement>(null);
  const proxy = useRef<HTMLDivElement>(null);
  const controller = useRef<SurfaceController | null>(null);
  const [tree, setTree] = useState<A11yTree | null>(null);
  const callbacks = useRef({ onController, onError, onZoom, onViewChange });
  callbacks.current = { onController, onError, onZoom, onViewChange };

  useEffect(() => {
    if (
      !root.current ||
      !scroller.current ||
      !spacer.current ||
      !content.current ||
      !overlay.current ||
      !proxy.current
    ) {
      return;
    }
    const c = new SurfaceController(
      client,
      doc,
      {
        root: root.current,
        scroller: scroller.current,
        spacer: spacer.current,
        content: content.current,
        overlay: overlay.current,
        proxy: proxy.current,
      },
      {
        theme,
        blink,
        zoomKeys,
        onA11y: setTree,
        onError: (e) => callbacks.current.onError?.(e),
        onZoom: (z) => callbacks.current.onZoom?.(z),
        onViewChange: (v) => callbacks.current.onViewChange?.(v),
      },
    );
    controller.current = c;
    c.start();
    if (autoFocus) c.focus();
    callbacks.current.onController?.(c);
    return () => {
      callbacks.current.onController?.(null);
      controller.current = null;
      c.dispose();
    };
  }, [client, doc]);

  useEffect(() => controller.current?.setTheme(theme), [theme]);
  useEffect(() => controller.current?.setBlink(blink), [blink]);

  const active = tree?.active !== undefined && tree?.active !== null ? mirrorId(doc, tree.active) : undefined;
  return (
    <div ref={root} style={ROOT} data-testid="kiln-surface" className="kiln-surface">
      <canvas ref={content} style={CANVAS} data-testid="kiln-surface-content" />
      <canvas ref={overlay} style={CANVAS} data-testid="kiln-surface-overlay" />
      <div
        ref={scroller}
        style={SCROLLER}
        data-testid="kiln-surface-scroller"
        className="kiln-surface-scroller"
      >
        <div ref={spacer} style={SPACER}>
          {/* biome-ignore lint/a11y/useSemanticElements: the input proxy is a contenteditable (PLAN.md §3.6): beforeinput, composition events and caret placement for IME windows behave alike on every webview, which a textarea does not guarantee. */}
          <div
            ref={proxy}
            contentEditable
            suppressContentEditableWarning
            role="textbox"
            tabIndex={0}
            aria-multiline="true"
            aria-label={label}
            aria-activedescendant={active}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            data-gramm="false"
            translate="no"
            style={PROXY}
            data-testid="kiln-input-proxy"
          />
        </div>
      </div>
      <A11yMirror doc={doc} tree={tree} />
    </div>
  );
}
