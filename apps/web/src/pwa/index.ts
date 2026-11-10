/**
 * The installable web app: the service worker, its update and offline notices, the
 * install card, and files launched or shared to Kiln. Only the GitHub Pages build
 * (`vite build --mode pages`, `__KILN_PWA__`) loads this; the dev server and the Tauri shells
 * have no service worker.
 */
import { startLaunches } from './launch.ts';
import { startPwa } from './register.ts';

export function start(): void {
  startPwa();
  startLaunches();
}
