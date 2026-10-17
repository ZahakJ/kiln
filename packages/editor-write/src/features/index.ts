/**
 * Kiln Write's advanced features (WP-66) on the UI side: comments and their balloons, the
 * Reviewing, Selection and Spelling panes, track changes in the ribbon and status bar, the
 * reference and layout dialogs, the object frame, focus mode, Format Painter and the spelling
 * context menu. Presentation only: every change is a core command.
 */
export { CommentBox, CommentCard, type Thread, threadsOf } from './Comments.tsx';
export { WriteCommentsButton } from './CommentsButton.tsx';
export { FEATURE_DIALOGS, registerWriteFeatures } from './commands.ts';
export { FeatureDialogs } from './dialogs.tsx';
export { addToDictionary, ensureDictionary, loadUserWords } from './dictionary.ts';
export { draggedRect, WriteFeatureLayer } from './Layer.tsx';
export { PASTE_MARKDOWN_ENTRY, spellingEntries } from './menu.ts';
export {
  avatarText,
  type FeatureDialog,
  type FeaturePane,
  type FeatureUiState,
  featureState,
  featuresOf,
  WriteFeatures,
  whenLabel,
} from './model.ts';
export { featureStatusItems, goalPercent } from './status.tsx';
