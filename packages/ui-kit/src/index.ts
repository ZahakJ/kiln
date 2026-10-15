/**
 * Public entry point of @officina/ui-kit: the Horizon UI primitives (PLAN.md §1.2, §3.10), icons
 * (Lucide + Officina Glyphs), theme bindings, and the Office chrome (PLAN.md §1.3–1.5): AppFrame,
 * Ribbon, Backstage, StatusBar, CommandPalette, task panes, mini toolbar, context menus, Key Tips and
 * the Options dialog, all driven by the @officina/commands registry. Import
 * `@officina/tokens/index.css` once in the app for fonts and tokens; component styles are bundled.
 */
export const PACKAGE_NAME = '@officina/ui-kit' as const;

export {
  BACKSTAGE_PAGE_TITLES,
  Backstage,
  type BackstageActionId,
  type BackstagePageId,
  type BackstageProps,
} from './backstage/Backstage.tsx';
export * from './chrome/index.ts';
export { CommandMenuItems, isCommandEntry, type MenuEntry } from './contextmenu/CommandMenuItems.tsx';
export { ContextMenu, type ContextMenuProps } from './contextmenu/ContextMenu.tsx';
export * from './icons/index.ts';
export { KeyTipsLayer, type KeyTipsLayerProps } from './keytips/KeyTipsLayer.tsx';
export {
  type AnchorRect,
  MiniToolbar,
  type MiniToolbarProps,
  type SelectionMiniToolbar,
  useSelectionMiniToolbar,
} from './minitoolbar/MiniToolbar.tsx';
export { CommandPalette, type CommandPaletteProps } from './palette/CommandPalette.tsx';
export { type PaneSide, type TaskPane, TaskPaneHost, type TaskPaneHostProps } from './panes/TaskPaneHost.tsx';
export * from './primitives/index.ts';
export * from './ribbon/index.ts';
export {
  StatusBar,
  type StatusBarItem,
  type StatusBarProps,
  type StatusBarView,
  type StatusBarZoom,
  ZoomControl,
} from './statusbar/StatusBar.tsx';
export * from './theme/index.ts';
