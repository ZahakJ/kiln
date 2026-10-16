import { describe, expect, it } from 'vitest';
import { compareImages, decodePng, encodePng, solidImage, visualScore } from '../src/index.ts';

describe('PNG codec', () => {
  it('round-trips RGBA images', () => {
    const img = solidImage(7, 5, [10, 20, 30, 255]);
    img.data.set([255, 0, 0, 128], (2 * 7 + 3) * 4);
    const back = decodePng(encodePng(img));
    expect(back.width).toBe(7);
    expect(back.height).toBe(5);
    expect(Buffer.from(back.data).equals(Buffer.from(img.data))).toBe(true);
  });

  it('rejects other data', () => {
    expect(() => decodePng(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]))).toThrow();
  });
});

describe('image comparison', () => {
  it('compares rasters pixel by pixel', () => {
    const white = solidImage(10, 10, [255, 255, 255, 255]);
    const half = solidImage(10, 10, [255, 255, 255, 255]);
    for (let i = 0; i < 50; i++) half.data.set([0, 0, 0, 255], i * 4);
    expect(compareImages(white, white).score).toBe(1);
    expect(compareImages(white, half).score).toBeCloseTo(0.5);
    // A smaller image is padded with white.
    expect(compareImages(white, solidImage(10, 5, [255, 255, 255, 255])).score).toBe(1);
  });

  it('averages pages and counts missing pages as 0', () => {
    const page = encodePng(solidImage(4, 4, [255, 255, 255, 255]));
    const black = encodePng(solidImage(4, 4, [0, 0, 0, 255]));
    expect(visualScore([page, page], [page, page])).toBe(1);
    expect(visualScore([page, page], [page])).toBe(0.5);
    expect(visualScore([page], [black])).toBe(0);
    expect(visualScore([], [])).toBe(1);
  });
});
