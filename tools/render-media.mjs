#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Renders the brand rasters from their SVG sources with headless Chromium, so
// no PNG in the repository is ever edited by hand:
//
//   docs/media/icon.svg         -> apps/web/public/icons/icon-<size>.png (16 … 1024)
//                                  apps/web/public/icons/icon-maskable-<size>.png (192, 512)
//                                  apps/desktop/src-tauri/icons/source.png (1024, input of `pnpm tauri icon`)
//                                  docs/media/icon.png (256, for README/landing)
//   docs/media/banner.svg       -> docs/media/banner.png (2000×600)
//   docs/media/banner-light.svg -> docs/media/banner-light.png (2000×600)
//   banner.svg on a card        -> docs/media/social-preview.png (1280×640)
//
// The SVGs are neutral placeholders until WP-51 imports the brand kit. Liberation
// Sans (bundled, OFL) is embedded in each SVG's <style> as a data: URI before
// rendering, so CI and a laptop produce the same pixels.
//
// Usage: pnpm media:render            (all outputs)
//        pnpm media:render --only icons|banners

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MEDIA = join(ROOT, 'docs/media');

const ICON_SIZES = [16, 32, 48, 64, 128, 192, 256, 512, 1024];
const MASKABLE_SIZES = [192, 512];

const args = process.argv.slice(2);
const only = args.includes('--only') ? args[args.indexOf('--only') + 1] : null;
if (only && only !== 'icons' && only !== 'banners') {
  console.error(`render-media: unknown --only value "${only}" (expected icons or banners)`);
  process.exit(2);
}

/** Resolve a dependency through the workspace package that declares it. */
function requireFrom(pkgJson, id) {
  const req = createRequire(join(ROOT, pkgJson));
  return req(id);
}

function loadChromium() {
  // `playwright` is a dependency of the e2e package (through @playwright/test),
  // which owns the pinned browser build; reuse it rather than a second copy.
  try {
    return requireFrom('e2e/package.json', '@playwright/test').chromium;
  } catch {
    return requireFrom('package.json', 'playwright').chromium;
  }
}

/** The bundled document font Liberation Sans (OFL), so every machine renders identical pixels. */
function fontDataUri(file) {
  return `data:font/ttf;base64,${readFileSync(join(ROOT, 'packages/tokens/fonts', file)).toString('base64')}`;
}

const FONT_CSS = [
  `@font-face{font-family:"Liberation Sans";font-weight:400;src:url(${fontDataUri('LiberationSans-Regular.ttf')}) format('truetype')}`,
  `@font-face{font-family:"Liberation Sans";font-weight:700;src:url(${fontDataUri('LiberationSans-Bold.ttf')}) format('truetype')}`,
].join('');

/** Embed the font in the SVG's own <style> and force an explicit pixel size. */
function prepareSvg(svg, width, height) {
  const open = svg.match(/<svg\b[^>]*>/);
  if (!open) throw new Error('render-media: not an SVG document');
  let tag = open[0]
    .replace(/\swidth="[^"]*"/, '')
    .replace(/\sheight="[^"]*"/, '')
    .replace(/^<svg\b/, `<svg width="${width}" height="${height}"`);
  if (!/\sviewBox=/.test(tag)) throw new Error('render-media: SVG needs a viewBox to be resized');
  tag += `<style>${FONT_CSS}</style>`;
  return svg.replace(open[0], tag);
}

function readMedia(name) {
  return readFileSync(join(MEDIA, name), 'utf8');
}

/** Wrap the 1280×640 social card around the dark banner, centred. */
function socialPreviewSvg() {
  const banner = readMedia('banner.svg');
  const inner = banner.replace(
    /<svg\b[^>]*>/,
    '<svg x="60" y="143" width="1160" height="348" viewBox="0 0 2000 600">',
  );
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="640" viewBox="0 0 1280 640">
  <rect width="1280" height="640" fill="#17181B"/>
  ${inner.replace(/<\?xml[^>]*>/, '')}
</svg>`;
}

/** Full-bleed variant for Android/PWA "maskable" icons: the wordmark inside the 80 % safe zone. */
function maskableIconSvg() {
  const icon = readMedia('icon.svg');
  const text = icon.match(/<text\b[\s\S]*?<\/text>/);
  if (!text) throw new Error('render-media: icon.svg has no <text> wordmark');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <rect width="512" height="512" fill="#26272B"/>
  <g transform="translate(256 256) scale(0.8) translate(-256 -256)">${text[0]}</g>
</svg>`;
}

async function main() {
  const chromium = loadChromium();
  let browser;
  try {
    browser = await chromium.launch();
  } catch (err) {
    console.error(
      'render-media: could not start Chromium. Run `pnpm exec playwright install chromium` in e2e/.',
    );
    throw err;
  }
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  const written = [];

  async function render(svgSource, width, height, outputs) {
    const svg = prepareSvg(svgSource, width, height);
    await page.setViewportSize({ width, height });
    await page.setContent(
      `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;padding:0;background:transparent}svg{display:block}</style></head><body>${svg}</body></html>`,
      { waitUntil: 'load' },
    );
    const fontOk = await page.evaluate(async () => {
      await document.fonts.ready;
      return document.fonts.check('700 40px "Liberation Sans"');
    });
    if (svgSource.includes('<text') && !fontOk) {
      throw new Error('render-media: Liberation Sans failed to load');
    }
    const png = await page.screenshot({
      clip: { x: 0, y: 0, width, height },
      omitBackground: true,
      type: 'png',
    });
    for (const out of outputs) {
      const abs = join(ROOT, out);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, png);
      written.push(`${relative(ROOT, abs)} (${width}×${height})`);
    }
  }

  try {
    if (!only || only === 'icons') {
      const icon = readMedia('icon.svg');
      for (const size of ICON_SIZES) {
        const outs = [`apps/web/public/icons/icon-${size}.png`];
        if (size === 1024) outs.push('apps/desktop/src-tauri/icons/source.png');
        if (size === 256) outs.push('docs/media/icon.png');
        await render(icon, size, size, outs);
      }
      const maskable = maskableIconSvg();
      for (const size of MASKABLE_SIZES) {
        await render(maskable, size, size, [`apps/web/public/icons/icon-maskable-${size}.png`]);
      }
    }
    if (!only || only === 'banners') {
      await render(readMedia('banner.svg'), 2000, 600, ['docs/media/banner.png']);
      await render(readMedia('banner-light.svg'), 2000, 600, ['docs/media/banner-light.png']);
      await render(socialPreviewSvg(), 1280, 640, ['docs/media/social-preview.png']);
    }
  } finally {
    await browser.close();
  }

  for (const line of written) console.log(`render-media: wrote ${line}`);
  if (!existsSync(join(ROOT, 'apps/desktop/src-tauri/icons/source.png')) && only !== 'banners') {
    throw new Error('render-media: apps/desktop/src-tauri/icons/source.png was not written');
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
