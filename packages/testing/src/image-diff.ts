/**
 * Raster comparison for visual tests: Playwright screenshots against reviewed baselines and, from
 * WP-54 on, canvas painter output against `kiln-raster` renderings of the same Scene (PLAN.md §5).
 * Built on pixelmatch; a page missing from either side scores 0.
 */
import pixelmatch from 'pixelmatch';
import { decodePng, type RgbaImage } from './png.ts';

function padTo(img: RgbaImage, width: number, height: number): Uint8Array {
  if (img.width === width && img.height === height) return img.data;
  const out = new Uint8Array(width * height * 4).fill(255);
  for (let y = 0; y < img.height; y++) {
    out.set(img.data.subarray(y * img.width * 4, (y + 1) * img.width * 4), y * width * 4);
  }
  return out;
}

/** 1 − differing-pixel ratio of two rasters (images of different sizes are padded with white). */
export function compareImages(
  a: RgbaImage,
  b: RgbaImage,
  threshold = 0.1,
): { score: number; differing: number } {
  const w = Math.max(a.width, b.width);
  const h = Math.max(a.height, b.height);
  if (w === 0 || h === 0) return { score: 1, differing: 0 };
  const differing = pixelmatch(padTo(a, w, h), padTo(b, w, h), undefined, w, h, { threshold });
  return { score: 1 - differing / (w * h), differing };
}

/** Average page score over the longer list of PNG pages; a page missing from either side scores 0. */
export function visualScore(
  pagesA: readonly Uint8Array[],
  pagesB: readonly Uint8Array[],
  threshold = 0.1,
): number {
  const n = Math.max(pagesA.length, pagesB.length);
  if (n === 0) return 1;
  let total = 0;
  for (let i = 0; i < n; i++) {
    const pa = pagesA[i];
    const pb = pagesB[i];
    if (!pa || !pb) continue;
    total += compareImages(decodePng(pa), decodePng(pb), threshold).score;
  }
  return total / n;
}
