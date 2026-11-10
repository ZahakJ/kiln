/** Touch on the document surfaces: gestures, the touch selection, the long-press menu. */
export { OWN_LONG_PRESS, openContextMenuAt, ownLongPress, swallowRelease } from './contextMenu';
export {
  type AttachOptions,
  attachGestures,
  DOUBLE_TAP_MS,
  DOUBLE_TAP_SLOP,
  type GestureHandlers,
  type GesturePoint,
  GestureRecognizer,
  type GestureTimers,
  LONG_PRESS_MS,
  SWIPE_MAX_MS,
  SWIPE_MIN_PX,
  TAP_SLOP,
} from './gestures';
export { type ClientRect, type TouchSelection, touchSelection, unionRect } from './selection';
export { doubleTapZoom } from './zoom';
