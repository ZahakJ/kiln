// i18n-ignore-file: a developer route, not part of the product.
import { DEV_ROUTES } from './DevShell';
import { FrameRoute } from './FrameRoute';
import { ScratchRoute } from './ScratchRoute';

export { DEV_ROUTES } from './DevShell';
export { FrameRoute } from './FrameRoute';
export { commandForInput, commandForKey, ScratchRoute } from './ScratchRoute';

/** The bridge route for a location hash, or `null` for the rest of the app. */
export function bridgeRoute(hash: string) {
  if (hash === DEV_ROUTES.frame) return <FrameRoute />;
  if (hash === DEV_ROUTES.scratch) return <ScratchRoute />;
  return null;
}
