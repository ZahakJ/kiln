// The desktop app's frontend: the web build (apps/web/dist) without what only the browser uses.
// The desktop runs the native core, so the WebAssembly cores, their glue, the files only their
// Worker loads, the web app's icons and manifest and the source maps stay out of the executable; and the fonts the native core
// embeds are served to the interface from there (`/fonts/…`, src-tauri/src/fonts.rs)
// instead of being shipped twice. Writes apps/desktop/dist (the `frontendDist` of
// tauri.conf.json); `beforeBuildCommand` runs it after the web build.
import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const from = fileURLToPath(new URL('../../web/dist/', import.meta.url));
const to = fileURLToPath(new URL('../dist/', import.meta.url));
if (!existsSync(join(from, 'index.html'))) {
  console.error('apps/web/dist is missing: run `pnpm --filter @kiln/web build` first');
  process.exit(1);
}

/** Fonts the native core embeds (kiln_text::bundled_font and the brand fonts). */
const NATIVE_FONTS = [
  'LiberationMono-Regular.ttf',
  'LiberationMono-Bold.ttf',
  'LiberationMono-Italic.ttf',
  'LiberationMono-BoldItalic.ttf',
  'LiberationSans-Regular.ttf',
  'LiberationSans-Bold.ttf',
  'LiberationSans-Italic.ttf',
  'LiberationSans-BoldItalic.ttf',
  'LiberationSerif-Regular.ttf',
  'LiberationSerif-Bold.ttf',
  'LiberationSerif-Italic.ttf',
  'LiberationSerif-BoldItalic.ttf',
  'Carlito-Regular.ttf',
  'Carlito-Bold.ttf',
  'Carlito-Italic.ttf',
  'Carlito-BoldItalic.ttf',
  'Caladea-Regular.ttf',
  'Caladea-Bold.ttf',
  'Caladea-Italic.ttf',
  'Caladea-BoldItalic.ttf',
  'NotoNaskhArabic-Regular.ttf',
  'NotoNaskhArabic-Bold.ttf',
  'NotoSansSymbols2-Regular.ttf',
  'NotoSansMath-Regular.ttf',
  'Figtree-VF.ttf',
  'Fraunces-VF.ttf',
];
/** Where the shell serves them, on the app's own origin (src-tauri/src/fonts.rs). */
const FONT_BASE = '/fonts/';
/** `Carlito-Regular-BkJcQkis.ttf` → `Carlito-Regular.ttf` when the core embeds it. */
const nativeFont = (name) => {
  const m = /^(.+)-[A-Za-z0-9_-]{8}\.ttf$/.exec(name);
  return m && NATIVE_FONTS.includes(`${m[1]}.ttf`) ? `${m[1]}.ttf` : null;
};

const files = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else files.push(p);
  }
};
walk(from);

/** The core's Worker and the WebAssembly glue: the browser's core, never started on the desktop. */
const coreOnly = (p) =>
  /\/(worker|kiln_(bridge|collab)_wasm|grid-threads)[^/]*\.js$/.test(p.replaceAll('\\', '/'));
const text = /\.(js|css|html|json|webmanifest)$/;
/** What the interface itself loads: every text file but the core's. */
const page = files
  .filter((p) => text.test(p) && !coreOnly(p))
  .map((p) => readFileSync(p, 'utf8'))
  .join('\n');
const core = files
  .filter(coreOnly)
  .map((p) => readFileSync(p, 'utf8'))
  .join('\n');

const leaveOut = (p) => {
  if (/\.(wasm|map)$/.test(p) || coreOnly(p) || nativeFont(basename(p))) return true;
  // The installable web app's icons, screenshots and manifest (the desktop has its own icons).
  if (/[/\\](icons|screenshots)[/\\]/.test(p.slice(from.length - 1)) || /\.webmanifest$/.test(p)) return true;
  const name = basename(p);
  // A file only the core's Worker names (the fonts it gives the WebAssembly core).
  return core.includes(name) && !page.includes(name);
};

rmSync(to, { recursive: true, force: true });
let kept = 0;
let left = 0;
for (const p of files) {
  const size = statSync(p).size;
  if (leaveOut(p)) {
    left += size;
    continue;
  }
  const out = join(to, p.slice(from.length));
  if (/\.css$/.test(p)) {
    // The @font-face rules of the fonts the core embeds are served by the shell.
    const css = readFileSync(p, 'utf8').replace(/\/assets\/([A-Za-z0-9_.-]+\.ttf)/g, (whole, name) => {
      const font = nativeFont(name);
      return font ? `${FONT_BASE}${font}` : whole;
    });
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, css);
    kept += Buffer.byteLength(css);
    continue;
  }
  kept += size;
  cpSync(p, out);
}
const mb = (n) => (n / 1048576).toFixed(1);
console.log(
  `apps/desktop/dist: ${mb(kept)} MB (left out ${mb(left)} MB: the browser's core, and fonts the native core serves)`,
);
