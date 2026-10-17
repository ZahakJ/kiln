/**
 * @kiln/home — Kiln Home, the start screen (PLAN.md §1.6): app doors, recent/pinned/shared files,
 * templates and search. Presentation only: data comes from the core's recents (through the host)
 * and actions go out through callbacks.
 */
export const PACKAGE_NAME = '@kiln/home' as const;

export { DOOR_MASK, DOOR_PATH } from './door-path.ts';
export { greeting } from './greeting.ts';
export {
  type HomeList,
  type HomeStorage,
  type HomeTemplate,
  KilnHome,
  type KilnHomeProps,
} from './KilnHome.tsx';
export { appKind, HOME_APPS, type HomeApp, kindApp, relativeTime } from './model.ts';
