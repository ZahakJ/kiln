/**
 * Kiln Grid in the app frame (`#/grid`): the editor of `@kiln/editor-grid` for the window's
 * workbook — the formula bar, the sheet surface, the sheet tabs, the menus and dialogs in the
 * frame's document area — and its status bar. The frame stays generic: it asks this hook for the
 * parts and falls back to its own while the app is not Grid or no workbook is shown yet.
 */
import type { CommandRegistry } from '@kiln/commands';
import {
  GridEditor,
  type GridEditorModel,
  GridStatusBar,
  type GridSurfaceController,
  useChartTabs,
  useGridEditorModel,
} from '@kiln/editor-grid';
import type { DocSummary, KilnClient } from '@kiln/protocol';
import { type RibbonSpec, useTheme } from '@kiln/ui-kit';
import { type ReactNode, useEffect, useMemo } from 'react';

declare global {
  interface Window {
    /** Development builds: the Grid editor's model of the window (e2e tests). */
    __kilnGrid?: GridEditorModel;
    /** Development builds: the grid surface (e2e tests read its frame timings). */
    __kilnGridSurface?: GridSurfaceController | null;
  }
}

export interface GridParts {
  /** The document area: formula bar, sheet, tabs, dialogs. */
  surface: ReactNode;
  statusBar: ReactNode;
  ribbon: RibbonSpec | undefined;
}

const onController = (c: GridSurfaceController | null) => {
  if (import.meta.env.DEV) window.__kilnGridSurface = c;
};

export function useGridParts(
  deps: {
    client: KilnClient | null;
    registry: CommandRegistry;
    summary: DocSummary | null;
    ribbon: RibbonSpec | undefined;
  } | null,
): GridParts | null {
  const client = deps?.client ?? null;
  const summary = deps?.summary ?? null;
  const registry = deps?.registry ?? null;
  const baseRibbon = deps?.ribbon;
  const model = useGridEditorModel(
    client && summary && registry ? { client, doc: summary.doc, registry } : null,
  );
  // Chart Design and Format join the ribbon while a chart is selected (WP-73's chart tabs).
  const chartTabs = useChartTabs(model);
  const ribbon = useMemo(
    () => (baseRibbon ? { ...baseRibbon, tabs: [...baseRibbon.tabs, ...chartTabs] } : undefined),
    [baseRibbon, chartTabs],
  );
  const theme = useTheme();
  const dark = theme.isDark;
  const title = summary?.title ?? '';
  const readOnly = summary?.read_only ?? false;
  const current = !!deps && !!model && !!summary && model.doc === summary.doc;
  // Screenshots and the e2e tests turn the caret's blink off (`?blink=0`).
  const blink = !/[?&]blink=0\b/.test(typeof window === 'undefined' ? '' : window.location.hash);
  useEffect(() => {
    if (!import.meta.env.DEV || !model) return;
    window.__kilnGrid = model;
    return () => {
      if (window.__kilnGrid === model) window.__kilnGrid = undefined;
    };
  }, [model]);
  return useMemo(
    () =>
      current && model
        ? {
            ribbon,
            surface: (
              <GridEditor
                model={model}
                title={title}
                theme={dark ? 'dark' : 'light'}
                blink={blink}
                onController={onController}
              />
            ),
            statusBar: <GridStatusBar model={model} readOnly={readOnly} />,
          }
        : null,
    [current, model, ribbon, title, dark, readOnly, blink],
  );
}
