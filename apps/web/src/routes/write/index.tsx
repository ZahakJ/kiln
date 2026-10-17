/**
 * Kiln Write in the app frame (`#/write`): the editor of `@kiln/editor-write` for the window's
 * document — its surface, panes and dialogs in the frame's document area, its status bar, and
 * Write's ribbon with Insert ▸ Table's grid picker. The frame stays generic: it asks this hook for
 * the parts and falls back to its own while the app is not Write or no document is shown yet.
 */
import type { CommandRegistry } from '@kiln/commands';
import {
  useWriteEditorModel,
  WriteCommentsButton,
  WriteEditor,
  WriteStatusBar,
  withTablePicker,
} from '@kiln/editor-write';
import type { DocSummary, KilnClient } from '@kiln/protocol';
import { type RibbonSpec, useTheme } from '@kiln/ui-kit';
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
  /** The tab strip's Comments button with the open-comment count (WP-66). */
  comments: ReactNode;
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
  useEffect(() => {
    // Development builds: the e2e tests read the editor's model (and through it the surface).
    if (!import.meta.env.DEV || !model) return;
    window.__kilnWrite = model;
    return () => {
      if (window.__kilnWrite === model) window.__kilnWrite = undefined;
    };
  }, [model]);
  // One object while nothing it shows changes: the frame is not rebuilt for a keystroke.
  return useMemo(
    () =>
      current && model
        ? {
            ribbon,
            surface: (
              <WriteEditor model={model} title={title} theme={dark ? 'dark' : 'light'} ribbon={ribbon} />
            ),
            comments: <WriteCommentsButton model={model} />,
            statusBar: (
              <WriteStatusBar
                model={model}
                readOnly={readOnly}
                onFit={(kind) => void registry?.execute(`suite.view.${kind}`)}
              />
            ),
          }
        : null,
    [current, model, ribbon, title, dark, readOnly, registry],
  );
}
