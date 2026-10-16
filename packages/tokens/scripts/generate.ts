/**
 * Writes tokens.css, theme.css and fonts.css from src/tokens.ts.
 * Run: pnpm --filter @kiln/tokens generate
 */
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { renderFontsCss, renderThemeCss, renderTokensCss } from '../src/css.ts';

const out = (name: string) => fileURLToPath(new URL(`../${name}`, import.meta.url));

writeFileSync(out('tokens.css'), renderTokensCss());
writeFileSync(out('theme.css'), renderThemeCss());
writeFileSync(out('fonts.css'), renderFontsCss());
console.log('tokens: wrote tokens.css, theme.css, fonts.css');
