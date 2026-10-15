#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Renders the brand rasters from their SVG sources with headless Chromium, so
// no PNG in the repository is ever edited by hand:
//
//   docs/media/icon.svg         -> apps/web/public/icons/icon-<size>.png (16 … 1024)
//                                  apps/web/public/icons/icon-maskable-<size>.png (192, 512)
//                                  apps/desktop/build/icon.png (512)
//                                  docs/media/icon.png (256, for README/landing)
//   docs/media/banner.svg       -> docs/media/banner.png (2000×600)
//   docs/media/banner-light.svg -> docs/media/banner-light.png (2000×600)
//   banner.svg on a card        -> docs/media/social-preview.png (1280×640)
//
// Inter comes from @fontsource-variable/inter and is embedded in each SVG's
// <style> as a data: URI before rendering, so the output does not depend on the
// fonts installed on the machine (CI and a laptop produce the same pixels).
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

function resolveFrom(pkgJson, id) {
  const req = createRequire(join(ROOT, pkgJson));
  return req.resolve(id);
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

function interDataUri() {
  let pkgDir;
  for (const owner of ['apps/web/package.json', 'packages/tokens/package.json', 'package.json']) {
    try {
      pkgDir = dirname(resolveFrom(owner, '@fontsource-variable/inter/package.json'));
      break;
    } catch {
      // try the next workspace package
    }
  }
  if (!pkgDir)
    throw new Error('render-media: @fontsource-variable/inter is not installed (run pnpm install)');
  const file = join(pkgDir, 'files/inter-latin-wght-normal.woff2');
  return `data:font/woff2;base64,${readFileSync(file).toString('base64')}`;
}

const FONT_CSS = `@font-face{font-family:Inter;font-style:normal;font-weight:100 900;src:url(${interDataUri()}) format('woff2')}`;

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
  <defs>
    <radialGradient id="sp-glow" cx="0.3" cy="0.35" r="0.75">
      <stop offset="0" stop-color="#3F6FE0" stop-opacity="0.16"/>
      <stop offset="1" stop-color="#3F6FE0" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1280" height="640" fill="#0b0d10"/>
  <rect width="1280" height="640" fill="url(#sp-glow)"/>
  ${inner.replace(/<\?xml[^>]*>/, '')}
</svg>`;
}

/** Full-bleed variant for Android/PWA "maskable" icons: glyph inside the 80 % safe zone. */
function maskableIconSvg() {
  const icon = readMedia('icon.svg');
  const glyph = icon.match(/<path\b[^>]*\/>/);
  if (!glyph) throw new Error('render-media: icon.svg has no glyph <path>');
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <defs>
    <linearGradient id="m-tile" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#4F7FF2"/><stop offset="1" stop-color="#2B58C7"/>
    </linearGradient>
  </defs>
  <rect width="512" height="512" fill="url(#m-tile)"/>
  <g transform="translate(256 256) scale(0.72) translate(-256 -256)">${glyph[0]}</g>
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
      return document.fonts.check('800 40px Inter');
    });
    if (svgSource.includes('<text') && !fontOk) throw new Error('render-media: Inter failed to load');
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
        if (size === 512) outs.push('apps/desktop/build/icon.png');
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
  if (!existsSync(join(ROOT, 'apps/desktop/build/icon.png')) && only !== 'banners') {
    throw new Error('render-media: apps/desktop/build/icon.png was not written');
  }
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : err);
  process.exit(1);
});
