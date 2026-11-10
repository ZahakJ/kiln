/**
 * Regenerates @kiln/tokens from the brand kit: parses `brand/tokens.json` (strictly), writes the typed
 * kit object `src/generated/kit.ts` and renders `tokens.css`, `theme.css` and `fonts.css`, each
 * formatted with Biome. Run: pnpm --filter @kiln/tokens generate (tools/import-brand.sh runs it too).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseKit, renderKitModule } from '../src/kit.ts';
import { formatGenerated } from './format.ts';

const pkg = (name: string) => fileURLToPath(new URL(`../${name}`, import.meta.url));
const kitPath = fileURLToPath(new URL('../../../brand/tokens.json', import.meta.url));

const kit = parseKit(JSON.parse(readFileSync(kitPath, 'utf8')));
const emit = (name: string, source: string) => writeFileSync(pkg(name), formatGenerated(source, name));

// The kit module first: css.ts imports tokens.ts, which imports it, so it must be current.
emit('src/generated/kit.ts', renderKitModule(kit));
const css = await import('../src/css.ts');
emit('tokens.css', css.renderTokensCss(kit));
emit('theme.css', css.renderThemeCss(kit));
emit('fonts.css', css.renderFontsCss());
console.log('tokens: wrote src/generated/kit.ts, tokens.css, theme.css, fonts.css from brand/tokens.json');
