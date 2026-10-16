#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Produces the raster variants the brand kit ships only as SVG (PLAN.md §1.2, WP-51). Everything the
// kit already ships as PNG is copied verbatim by tools/import-brand.sh; this script never redraws or
// rescales a kit PNG. It renders with headless Chromium, so a laptop and CI produce the same pixels.
//
//   brand/icons/svg/suite.svg                 -> apps/web/public/icons/icon-192.png (hero level, ≥ 128 px)
//   brand/android/suite-{background,foreground}.svg
//                                             -> apps/web/public/icons/icon-maskable-{192,512}.png
//                                                (full-bleed; the glyph stays in the 66 dp adaptive-icon
//                                                safe circle, inside the PWA 80 % safe zone)
//   brand/icons/png/suite-{16,24,32,48,64}.png + brand/icons/windows/suite-256.png
//                                             -> apps/desktop/src-tauri/icons/icon.ico (PNG-compressed
//                                                entries; each size keeps the kit's own drawing)
//   brand/icons/macos/suite-1024.svg + .png   -> apps/desktop/src-tauri/icons/icon.icns (the macOS canvas at
//                                                128, 256, 512 and 1024 px; the kit's 16/32/64 px drawings
//                                                for the list-view sizes). Built here rather than by
//                                                `tauri icon`, whose .icns entry order is not reproducible.
//
// Usage: pnpm media:render

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const BRAND = join(ROOT, 'brand');

const written = [];

function write(out, data, note) {
  const abs = join(ROOT, out);
  mkdirSync(dirname(abs), { recursive: true });
  let same = false;
  try {
    same = readFileSync(abs).equals(data);
  } catch {
    same = false;
  }
  if (!same) writeFileSync(abs, data);
  written.push(`${relative(ROOT, abs)} (${note})${same ? ', unchanged' : ''}`);
}

/** Resolve a dependency through the workspace package that declares it. */
function requireFrom(pkgJson, id) {
  return createRequire(join(ROOT, pkgJson))(id);
}

function loadChromium() {
  // `playwright` comes with the e2e package (through @playwright/test), which owns the pinned
  // browser build; reuse it rather than a second copy.
  try {
    return requireFrom('e2e/package.json', '@playwright/test').chromium;
  } catch {
    return requireFrom('packages/ui-kit/package.json', 'playwright').chromium;
  }
}

/** Strip the root element's fixed size and give it an explicit pixel size (the viewBox scales it). */
function sized(svg, size) {
  const open = svg.match(/<svg\b[^>]*>/);
  if (!open) throw new Error('render-media: not an SVG document');
  if (!/\sviewBox=/.test(open[0])) throw new Error('render-media: SVG needs a viewBox to be resized');
  const tag = open[0]
    .replace(/\swidth="[^"]*"/, '')
    .replace(/\sheight="[^"]*"/, '')
    .replace(/^<svg\b/, `<svg width="${size}" height="${size}" style="position:absolute;inset:0"`);
  return svg.replace(/<\?xml[^>]*>/, '').replace(open[0], tag);
}

const brand = (path) => readFileSync(join(BRAND, path));

/**
 * Windows .ico with PNG-compressed images (supported since Windows Vista). `images` are PNG buffers;
 * their size is read from the IHDR chunk.
 */
export function buildIco(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(images.length, 4);
  const entries = [];
  let offset = 6 + 16 * images.length;
  for (const png of images) {
    if (png.readUInt32BE(12) !== 0x49484452) throw new Error('render-media: not a PNG (no IHDR)');
    const w = png.readUInt32BE(16);
    const h = png.readUInt32BE(20);
    if (w > 256 || h > 256) throw new Error(`render-media: .ico images are at most 256 px, got ${w}×${h}`);
    const e = Buffer.alloc(16);
    e.writeUInt8(w === 256 ? 0 : w, 0);
    e.writeUInt8(h === 256 ? 0 : h, 1);
    e.writeUInt8(0, 2); // palette size
    e.writeUInt8(0, 3); // reserved
    e.writeUInt16LE(1, 4); // colour planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(png.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += png.length;
    entries.push(e);
  }
  return Buffer.concat([header, ...entries, ...images]);
}

/** Apple .icns with PNG payloads. `entries` are `[OSType, png]` pairs, written in the given order. */
export function buildIcns(entries) {
  const chunks = entries.map(([type, png]) => {
    if (!/^[a-z0-9]{4}$/i.test(type)) throw new Error(`render-media: bad icns type ${type}`);
    const head = Buffer.alloc(8);
    head.write(type, 0, 'latin1');
    head.writeUInt32BE(png.length + 8, 4);
    return Buffer.concat([head, png]);
  });
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write('icns', 0, 'latin1');
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

async function main() {
  const chromium = loadChromium();
  let browser;
  try {
    browser = await chromium.launch();
  } catch (err) {
    console.error('render-media: could not start Chromium. Run `pnpm exec playwright install chromium` in e2e/.');
    throw err;
  }
  const page = await browser.newPage({ deviceScaleFactor: 1 });

  /** Renders stacked SVG layers (bottom first) into a size × size transparent PNG. */
  async function render(layers, size) {
    await page.setViewportSize({ width: size, height: size });
    const body = layers.map((svg) => sized(svg, size)).join('');
    await page.setContent(
      `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;padding:0;background:transparent}body{position:relative;width:${size}px;height:${size}px}</style></head><body>${body}</body></html>`,
      { waitUntil: 'load' },
    );
    return page.screenshot({ clip: { x: 0, y: 0, width: size, height: size }, omitBackground: true, type: 'png' });
  }

  try {
    const hero = brand('icons/svg/suite.svg').toString('utf8');
    write('apps/web/public/icons/icon-192.png', await render([hero], 192), '192×192 from icons/svg/suite.svg');

    const bg = brand('android/suite-background.svg').toString('utf8');
    const fg = brand('android/suite-foreground.svg').toString('utf8');
    for (const size of [192, 512]) {
      write(
        `apps/web/public/icons/icon-maskable-${size}.png`,
        await render([bg, fg], size),
        `${size}×${size} maskable from android/suite-{background,foreground}.svg`,
      );
    }

    const mac = brand('icons/macos/suite-1024.svg').toString('utf8');
    const macPng = {};
    for (const size of [128, 256, 512]) macPng[size] = await render([mac], size);
    macPng[1024] = brand('icons/macos/suite-1024.png');
    const small = (s) => brand(`icons/png/suite-${s}.png`);
    const icns = buildIcns([
      ['icp4', small(16)],
      ['icp5', small(32)],
      ['ic11', small(32)],
      ['ic12', small(64)],
      ['ic07', macPng[128]],
      ['ic13', macPng[256]],
      ['ic08', macPng[256]],
      ['ic14', macPng[512]],
      ['ic09', macPng[512]],
      ['ic10', macPng[1024]],
    ]);
    write('apps/desktop/src-tauri/icons/icon.icns', icns, 'macOS canvas 16 … 1024');
  } finally {
    await browser.close();
  }

  const ico = buildIco([
    ...[16, 24, 32, 48, 64].map((s) => brand(`icons/png/suite-${s}.png`)),
    brand('icons/windows/suite-256.png'),
  ]);
  write('apps/desktop/src-tauri/icons/icon.ico', ico, '16, 24, 32, 48, 64, 256 from the kit PNGs');

  for (const line of written) console.log(`render-media: wrote ${line}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  });
}
