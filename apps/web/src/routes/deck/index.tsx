/**
 * Kiln Deck in the app frame (`#/deck`): the editor of `@kiln/editor-deck` for the window's
 * presentation — the thumbnail rail, the slide stage, the speaker notes, the menus and dialogs in
 * the frame's document area — its status bar, and the Deck ribbon with the presentation's layouts
 * in New Slide ▾ and Layout ▾. The frame stays generic: it asks this hook for the parts and falls
 * back to its own while the app is not Deck or no presentation is shown yet.
 */

import { type CommandRegistry, DECK_SLIDE_MASTER_TAB, deckRibbon } from '@kiln/commands';
import {
  DeckEditor,
  type DeckEditorModel,
  type DeckStageController,
  DeckStatusBar,
  deckRibbonWithLayouts,
  deckRibbonWithMotion,
  ExportPicturesPane,
  lockLandscape,
  newChannelName,
  type PresentRequest,
  themeImages,
  useDeck,
  useDeckChartTabs,
  useDeckEditorModel,
  variantImages,
} from '@kiln/editor-deck';
import { useLanguage } from '@kiln/i18n/react';
import {
  type DeckLayoutInfo,
  type DeckMotionState,
  type DocSummary,
  isTauri,
  type KilnClient,
  type Rect,
} from '@kiln/protocol';
import { type RibbonSpec, type RibbonTab, ribbonFromData, useTheme } from '@kiln/ui-kit';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import type { ClientRect } from '../write/index.tsx';

declare global {
  interface Window {
    /** Development builds: the Deck editor's model of the window (e2e tests). */
    __kilnDeck?: DeckEditorModel;
    /** Development builds: the slide stage (e2e tests read its view and frame timings). */
    __kilnDeckStage?: DeckStageController | null;
  }
}

export interface DeckParts {
  /** The document area: rail, stage, notes, dialogs. */
  surface: ReactNode;
  statusBar: ReactNode;
  ribbon: RibbonSpec | undefined;
  /** File ▸ Export: slides as pictures. */
  exportPane: ReactNode;
  /** where a rectangle of the slide shown is on the screen (others' name flags). */
  locate: (r: Rect) => ClientRect | null;
  /** shows someone's slide (going to where they are). */
  showSlide: (slide: number) => void;
}

const onController = (c: DeckStageController | null) => {
  if (import.meta.env.DEV) window.__kilnDeckStage = c;
};

const NO_LAYOUTS: readonly DeckLayoutInfo[] = [];

export function useDeckParts(
  deps: {
    client: KilnClient | null;
    registry: CommandRegistry;
    summary: DocSummary | null;
    ribbon: RibbonSpec | undefined;
  } | null,
): DeckParts | null {
  const client = deps?.client ?? null;
  const summary = deps?.summary ?? null;
  const registry = deps?.registry ?? null;
  const model = useDeckEditorModel(
    client && summary && registry ? { client, doc: summary.doc, registry } : null,
  );
  const layouts = useDeckLayouts(model);
  const theme = useTheme();
  const dark = theme.isDark;
  const images = useThemeImages(model, dark ? 'dark' : 'light');
  const variants = useVariantImages(model, dark ? 'dark' : 'light');
  const motion = useDeckMotion(model);
  const chartTabs = useDeckChartTabs(model);
  // New Slide ▾ and Layout ▾ list the presentation's layouts; Themes shows each theme's sample;
  // Effect Options lists the core's options for the transition and the selected effect; Chart
  // Design and Format join while a chart is selected.
  const ribbon = useMemo(() => {
    if (!deps?.ribbon) return undefined;
    const spec = ribbonFromData(
      deckRibbonWithMotion(deckRibbonWithLayouts(deckRibbon, layouts, images, variants), motion),
    );
    return { ...spec, tabs: [MASTER_TAB, ...spec.tabs, ...chartTabs] };
  }, [deps?.ribbon, layouts, images, variants, motion, chartTabs]);
  // Slide Show ▸ From Beginning and the rest show the core's show at `#/present`.
  useEffect(() => {
    if (!model) return;
    model.present = (req) => void present(model, req);
    return () => {
      model.present = null;
    };
  }, [model]);
  // A PowerPoint Show (.ppsx) starts its slide show as soon as it opens, as in PowerPoint; Esc
  // at the end comes back to the editor.
  const showFormat = summary?.format === 'Ppsx' ? summary.doc : null;
  useEffect(() => {
    if (!model || showFormat === null || model.doc !== showFormat || SHOWN.has(showFormat)) return;
    SHOWN.add(showFormat);
    void present(model, { from: null, mode: 'Show', tab: false, presenter: false });
  }, [model, showFormat]);
  const title = summary?.title ?? '';
  const readOnly = summary?.read_only ?? false;
  const current = !!deps && !!model && !!summary && model.doc === summary.doc;
  // Screenshots and the e2e tests turn the caret's blink off (`?blink=0`).
  const blink = !/[?&]blink=0\b/.test(typeof window === 'undefined' ? '' : window.location.hash);
  useEffect(() => {
    if (!import.meta.env.DEV || !model) return;
    window.__kilnDeck = model;
    return () => {
      if (window.__kilnDeck === model) window.__kilnDeck = undefined;
    };
  }, [model]);
  const language = useLanguage();
  return useMemo(
    () =>
      current && model
        ? {
            ribbon,
            surface: (
              <DeckEditor
                model={model}
                title={title}
                theme={dark ? 'dark' : 'light'}
                blink={blink}
                onController={onController}
              />
            ),
            statusBar: <DeckStatusBar model={model} readOnly={readOnly} />,
            exportPane: <ExportPicturesPane model={model} />,
            locate: (r: Rect) => {
              const view = model.stage?.view?.();
              const stage = document.querySelector('[data-kiln-deck-stage]')?.getBoundingClientRect();
              if (!view || !stage) return null;
              return {
                left: stage.left + view.left + r.x * view.zoom,
                top: stage.top + view.top + r.y * view.zoom,
                width: r.width * view.zoom,
                height: r.height * view.zoom,
              };
            },
            showSlide: (slide: number) => {
              const index = model.state.deck?.slides.findIndex((s) => s.id === slide) ?? -1;
              if (index >= 0)
                void model.client.execute(model.doc, {
                  Deck: { SelectSlide: { index, extend: false, toggle: false } },
                });
            },
          }
        : null,
    [current, model, ribbon, title, dark, readOnly, blink, language],
  );
}

const NO_IMAGES: Readonly<Record<string, string>> = {};

/** The Slide Master tab, in front of the others while Slide Master view is on. */
const MASTER_TAB: RibbonTab = {
  ...(ribbonFromData({ app: 'deck', tabs: [DECK_SLIDE_MASTER_TAB] }).tabs[0] as RibbonTab),
  when: (ctx) => !!(ctx.deckState as { master_view?: unknown } | undefined)?.master_view,
};

/** PowerPoint Shows whose slide show has started on opening (once each). */
const SHOWN = new Set<number>();

/** The bundled themes' samples, rendered once per model and colour scheme. */
function useThemeImages(
  model: DeckEditorModel | null,
  scheme: 'light' | 'dark',
): Readonly<Record<string, string>> {
  const names = useDeck(model ?? NO_MODEL, (s) => (s.deck?.themes ?? []).join('\n'));
  const [images, setImages] = useState(NO_IMAGES);
  useEffect(() => {
    if (!model || !names) return;
    let live = true;
    void themeImages(model, names.split('\n'), scheme).then((m) => {
      if (live) setImages(m);
    });
    return () => {
      live = false;
    };
  }, [model, names, scheme]);
  return images;
}

/** The current theme's variants as pictures, drawn again when the theme or its colours change. */
function useVariantImages(
  model: DeckEditorModel | null,
  scheme: 'light' | 'dark',
): Readonly<Record<string, string>> {
  const key = useDeck(model ?? NO_MODEL, (s) => s.deck?.theme ?? '');
  const [images, setImages] = useState(NO_IMAGES);
  useEffect(() => {
    if (!model || !key) return;
    let live = true;
    void variantImages(model, scheme).then((m) => {
      if (live) setImages(m);
    });
    return () => {
      live = false;
    };
  }, [model, key, scheme]);
  return images;
}

/** The motion part of the state, stable while it does not change. */
function useDeckMotion(model: DeckEditorModel | null): DeckMotionState | null {
  const key = useDeck(model ?? NO_MODEL, (s) => {
    const m = s.deck?.motion;
    return m ? JSON.stringify({ t: m.transition, o: m.options, p: m.option }) : '';
  });
  return useMemo(() => model?.state.deck?.motion ?? null, [key, model]);
}

/**
 * Starts the show and goes to `#/present`: full screen (from the ribbon's click), in the tab, or
 * as Presenter View here with the show in a new window (opened now, while the click still counts
 * as the user's; on the desktop the shell opens it once the show runs).
 */
async function present(model: DeckEditorModel, req: PresentRequest): Promise<void> {
  const doc = model.doc;
  let query = `doc=${doc}`;
  // Desktop: the audience's window is the shell's, on the same core, opened once the show runs.
  const nativeWindow = isTauri() && req.presenter && req.mode === 'Show';
  if (nativeWindow) {
    query += '&view=presenter';
  } else if (req.presenter && req.mode === 'Show') {
    const channel = newChannelName();
    window.open(
      `${window.location.pathname}#/present?view=audience&channel=${encodeURIComponent(channel)}`,
      '_blank',
      'popup=yes,width=1280,height=720',
    );
    query += `&view=presenter&channel=${encodeURIComponent(channel)}`;
  } else if (req.tab) {
    query += '&tab=1';
  } else if (document.documentElement.requestFullscreen) {
    // A phone turns to landscape for the show, where the browser lets it.
    void document.documentElement
      .requestFullscreen()
      .then(() => lockLandscape())
      .catch(() => undefined);
  }
  if (import.meta.env.DEV && /[?&]clock=manual\b/.test(window.location.hash)) query += '&clock=manual';
  const ok = await model.core('deck.show.start', { from: req.from, mode: req.mode });
  if (!ok) return;
  if (nativeWindow) await model.core('suite.view.open_window', { window: 'Audience' });
  if (req.tool) {
    const id = {
      laser: 'deck.show.laser',
      pen: 'deck.show.pen',
      highlighter: 'deck.show.highlighter',
      black: 'deck.show.black',
    }[req.tool];
    await model.core(id);
  }
  window.location.hash = `#/present?${query}`;
}

/** The layouts of the presentation (stable while they do not change). */
function useDeckLayouts(model: DeckEditorModel | null): readonly DeckLayoutInfo[] {
  const key = useDeck(model ?? NO_MODEL, (s) => JSON.stringify(s.deck?.layouts ?? []));
  return useMemo(() => (key === '[]' ? NO_LAYOUTS : (JSON.parse(key) as DeckLayoutInfo[])), [key]);
}

/** A stand-in for hooks before the model exists. */
const NO_MODEL = {
  store: {
    getState: () => ({ deck: null }),
    getInitialState: () => ({ deck: null }),
    subscribe: () => () => undefined,
    setState: () => undefined,
  },
} as unknown as DeckEditorModel;
