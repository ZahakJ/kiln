/**
 * @kiln/editor-deck — Kiln Deck's editor views: the slide stage (Rust tiles
 * with the selection handles, smart guides and text editing drawn by the core; a DOM input proxy
 * and accessibility mirror), the thumbnail rail with sections and drag-to-reorder, the speaker
 * notes, the slide sorter, the context menus, the status bar and the dialogs (Slide Size, Format
 * Background, Insert Table, Header & Footer, Size and Position, Alt Text, Link, Find, Replace,
 * Shapes, Selection).
 *
 * Presentation only: every edit is a core command, and everything shown is an answer of the
 * core's `Query::Deck`.
 */
import type { CommandRegistry } from '@kiln/commands';
import type { DocId, KilnClient } from '@kiln/protocol';
import { useEffect, useState } from 'react';
import { registerDeckUi } from './commands.ts';
import { DeckEditorModel } from './model.ts';

export const PACKAGE_NAME = '@kiln/editor-deck' as const;

// charts on slides.
export {
  ChartDataDialog,
  columnName,
  FormatChartArea,
  InsertChartDialog,
  resized,
  useDeckChartHost,
  useDeckChartTabs,
} from './charts/DeckCharts.tsx';
export { DECK_DIALOG_COMMANDS, registerDeckUi, sectionOf } from './commands.ts';
export { DeckEditor, type DeckEditorProps } from './DeckEditor.tsx';
export { DeckMenus, deckMenuEntries, layoutEntries } from './DeckMenus.tsx';
export { DeckStage, type DeckStageProps } from './DeckStage.tsx';
export { DeckStatusBar, slideItems } from './DeckStatusBar.tsx';
export {
  browseThemes,
  DECK_DESIGN_CHROME,
  registerDesignUi,
  THEME_ACCEPT,
  variantImages,
} from './design/designCommands.ts';
export { DeckDialogs, presetLabel, ShapesGallery } from './dialogs/DeckDialogs.tsx';
// motion, the slide show and Presenter View.
export { AnimationPane, clickSelection, rowName } from './features/AnimationPane.tsx';
// Design Ideas, Outline View, Reuse Slides, pictures.
export { DesignIdeasPane, IDEA_WIDTH, ideaUrls } from './features/DesignIdeasPane.tsx';
export { download, ExportPicturesPane, PICTURE_WIDTHS } from './features/ExportPicturesPane.tsx';
export { DECK_FEATURE_CHROME, registerFeatureUi } from './features/featureCommands.ts';
export { audioPoster, insertMedia, MEDIA_ACCEPT, videoPoster } from './features/media.ts';
export { DECK_MOTION_CHROME, registerMotionUi, startShow } from './features/motionCommands.ts';
export {
  caretOffset,
  OutlinePane,
  type OutlineSpot,
  placeCaret,
  spotAfter,
} from './features/OutlinePane.tsx';
export { ReuseSlidesPane } from './features/ReuseSlidesPane.tsx';
export {
  animationIcon,
  deckRibbonWithMotion,
  effectMenu,
  TRANSITION_ICONS,
} from './features/ribbonMotion.ts';
export { SetUpShowDialog } from './features/SetUpShowDialog.tsx';
export { pickFile } from './files.ts';
export { useDeck } from './hooks.ts';
export {
  caretMotion,
  deckKeyAction,
  type KeyAction,
  railKeyAction,
  type StageMode,
  typesText,
} from './keys.ts';
export { isPhoneScreen, lockLandscape, unlockOrientation } from './mobile/PhoneShow.tsx';
export {
  type DeckDialog,
  DeckEditorModel,
  type DeckMenu,
  type DeckModelDeps,
  type DeckStageHandle,
  type DeckUiState,
  deckResult,
  messageOf,
  type PresentRequest,
  scopesOf,
} from './model.ts';
export { NotesPane } from './NotesPane.tsx';
export { formatElapsed, PresenterView, type PresenterViewProps } from './presenter/PresenterView.tsx';
export { newChannelName, type RelayMessage, relays, ShowRelay } from './presenter/relay.ts';
export { deckRibbonWithLayouts } from './ribbon.ts';
export { SlideSorter } from './SlideSorter.tsx';
export { ManualClock, realClock, type ShowClock } from './show/clock.ts';
export { composeFrame, fitSlide, multiply, type SlideFit, toSlidePoint } from './show/compose.ts';
export { ShowEngine, type ShowEngineOptions } from './show/engine.ts';
export { MediaLayer, type MediaLayerProps, mediaBox } from './show/MediaLayer.tsx';
export { ShowView, type ShowViewProps, showBinding } from './show/ShowView.tsx';
export { SlideGrid, ThumbCanvas, useSlideThumbs } from './show/SlideGrid.tsx';
export {
  clampScroll,
  cssCursor,
  DeckStageController,
  FIT_MARGIN,
  fitView,
  type StageElements,
} from './stage.ts';
export { dropIndex, type RailRow, railRows, THUMB_WIDTH, ThumbnailRail } from './ThumbnailRail.tsx';
export { paintScene, runs, ThumbCache, themeImages } from './thumbs.ts';

/**
 * The editor model of a presentation, with the editor's chrome commands registered on the
 * window's registry for as long as the component is mounted (`null` until one is shown).
 */
export function useDeckEditorModel(
  deps: { client: KilnClient; doc: DocId; registry: CommandRegistry } | null,
): DeckEditorModel | null {
  const client = deps?.client ?? null;
  const doc = deps?.doc ?? null;
  const registry = deps?.registry ?? null;
  const [model, setModel] = useState<DeckEditorModel | null>(null);
  useEffect(() => {
    if (!client || doc === null || !registry) {
      setModel(null);
      return;
    }
    const m = new DeckEditorModel({ client, doc, registry });
    const ui = registerDeckUi(m);
    setModel(m);
    return () => {
      ui.dispose();
      m.dispose();
      setModel(null);
    };
  }, [client, doc, registry]);
  return model;
}
