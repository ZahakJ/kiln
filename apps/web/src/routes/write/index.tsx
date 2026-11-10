/**
 * Kiln Write in the app frame (`#/write`): the editor of `@kiln/editor-write` for the window's
 * document — its surface, panes and dialogs in the frame's document area, its status bar, and
 * Write's ribbon with Insert ▸ Table's grid picker. The frame stays generic: it asks this hook for
 * the parts and falls back to its own while the app is not Write or no document is shown yet.
 */

import type { CommandRegistry } from '@kiln/commands';
import {
  usePhoneReflow,
  useWriteEditorModel,
  WRITE_PHONE_MENU,
  WriteCommentsButton,
  WriteEditor,
  WriteStatusBar,
  withTablePicker,
} from '@kiln/editor-write';
import { useLanguage } from '@kiln/i18n/react';
import type { DocSummary, KilnClient, Rect } from '@kiln/protocol';
import { type MenuEntry, type RibbonSpec, useFormFactor, useTheme } from '@kiln/ui-kit';
import { type ReactNode, useEffect, useMemo } from 'react';

declare global {
  interface Window {
    /** Development builds: the Write editor's model of the window (e2e tests). */
    __kilnWrite?: import('@kiln/editor-write').WriteEditorModel;
  }
}

export interface WriteParts {
  /** The document area: surface, rulers, panes, dialogs. */
  surface: ReactNode;
  statusBar: ReactNode;
  /** Write's ribbon with the editor's own controls. */
  ribbon: RibbonSpec | undefined;
  /** The tab strip's Comments button with the open-comment count. */
  comments: ReactNode;
  /** where a rectangle of the document is on the screen (others' name flags). */
  locate: (r: Rect) => ClientRect | null;
  /** scrolls a rectangle of the document into view (going to where someone is). */
  reveal: (r: Rect) => void;
  /** Write's entries in the phone's overflow menu (Print layout). */
  phoneMenu: readonly MenuEntry[];
}

/** A rectangle on the screen (client coordinates). */
export interface ClientRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function useWriteParts(
  deps: {
    client: KilnClient | null;
    registry: CommandRegistry;
    summary: DocSummary | null;
    ribbon: RibbonSpec | undefined;
  } | null,
): WriteParts | null {
  const client = deps?.client ?? null;
  const summary = deps?.summary ?? null;
  const registry = deps?.registry ?? null;
  const baseRibbon = deps?.ribbon;
  const model = useWriteEditorModel(
    client && summary && registry ? { client, doc: summary.doc, registry } : null,
  );
  const theme = useTheme();
  const ribbon = useMemo(() => (baseRibbon ? withTablePicker(baseRibbon) : undefined), [baseRibbon]);
  const dark = theme.isDark;
  const title = summary?.title ?? '';
  const readOnly = summary?.read_only ?? false;
  const current = !!deps && !!model && !!summary && model.doc === summary.doc;
  // a phone opens each document reflowed to its width (Web Layout).
  usePhoneReflow(current ? registry : null, summary?.doc ?? null, useFormFactor() === 'phone');
  useEffect(() => {
    // Development builds: the e2e tests read the editor's model (and through it the surface).
    if (!import.meta.env.DEV || !model) return;
    window.__kilnWrite = model;
    return () => {
      if (window.__kilnWrite === model) window.__kilnWrite = undefined;
    };
  }, [model]);
  // One object while nothing it shows changes: the frame is not rebuilt for a keystroke.
  const language = useLanguage();
  return useMemo(
    () =>
      current && model
        ? {
            ribbon,
            surface: (
              <WriteEditor model={model} title={title} theme={dark ? 'dark' : 'light'} ribbon={ribbon} />
            ),
            comments: <WriteCommentsButton model={model} />,
            locate: (r: Rect) => model.controller?.clientRectOf(r) ?? null,
            reveal: (r: Rect) => model.controller?.scrollToY(Math.max(0, r.y - 120)),
            phoneMenu: WRITE_PHONE_MENU,
            statusBar: (
              <WriteStatusBar
                model={model}
                readOnly={readOnly}
                onFit={(kind) => void registry?.execute(`suite.view.${kind}`)}
              />
            ),
          }
        : null,
    [current, model, ribbon, title, dark, readOnly, registry, language],
  );
}
