/**
 * A small PNG codec for test rasters (pdftoppm output, diff images): 8-bit greyscale, RGB, palette,
 * grey+alpha and RGBA, non-interlaced. Decoding always yields RGBA.
 */
import { deflateSync, inflateSync } from 'node:zlib';

export interface RgbaImage {
  width: number;
  height: number;
  /** width × height × 4 bytes, row-major RGBA. */
  data: Uint8Array;
}

const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of bytes) c = (CRC_TABLE[(c ^ b) & 0xff] as number) ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export class PngError extends Error {
  override name = 'PngError';
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

export function decodePng(png: Uint8Array): RgbaImage {
  for (let i = 0; i < 8; i++) if (png[i] !== SIGNATURE[i]) throw new PngError('not a PNG');
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  let pos = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  let palette: Uint8Array | undefined;
  let trns: Uint8Array | undefined;
  const idat: Uint8Array[] = [];
  while (pos < png.length) {
    const len = view.getUint32(pos);
    const type = String.fromCharCode(...png.subarray(pos + 4, pos + 8));
    const data = png.subarray(pos + 8, pos + 8 + len);
    pos += 12 + len;
    if (type === 'IHDR') {
      width = view.getUint32(16);
      height = view.getUint32(20);
      bitDepth = data[8] as number;
      colorType = data[9] as number;
      if (data[12] !== 0) throw new PngError('interlaced PNGs are not supported');
    } else if (type === 'PLTE') palette = data;
    else if (type === 'tRNS') trns = data;
    else if (type === 'IDAT') idat.push(data);
    else if (type === 'IEND') break;
  }
  if (bitDepth !== 8) throw new PngError(`bit depth ${bitDepth} is not supported`);
  const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colorType];
  if (!channels) throw new PngError(`colour type ${colorType} is not supported`);
  const total = idat.reduce((n, d) => n + d.length, 0);
  const joined = new Uint8Array(total);
  let off = 0;
  for (const d of idat) {
    joined.set(d, off);
    off += d.length;
  }
  const raw = new Uint8Array(inflateSync(joined));
  const stride = width * channels;
  const pixels = new Uint8Array(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)] as number;
    const src = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const row = pixels.subarray(y * stride, (y + 1) * stride);
    const prev = y > 0 ? pixels.subarray((y - 1) * stride, y * stride) : undefined;
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? (row[x - channels] as number) : 0;
      const b = prev ? (prev[x] as number) : 0;
      const c = prev && x >= channels ? (prev[x - channels] as number) : 0;
      const v = src[x] as number;
      switch (filter) {
        case 0:
          row[x] = v;
          break;
        case 1:
          row[x] = (v + a) & 0xff;
          break;
        case 2:
          row[x] = (v + b) & 0xff;
          break;
        case 3:
          row[x] = (v + ((a + b) >> 1)) & 0xff;
          break;
        case 4:
          row[x] = (v + paeth(a, b, c)) & 0xff;
          break;
        default:
          throw new PngError(`bad filter ${filter}`);
      }
    }
  }
  const out = new Uint8Array(width * height * 4);
  for (let i = 0, j = 0; i < width * height; i++, j += channels) {
    let r: number;
    let g: number;
    let b: number;
    let al = 255;
    if (colorType === 0) {
      r = g = b = pixels[j] as number;
    } else if (colorType === 2) {
      r = pixels[j] as number;
      g = pixels[j + 1] as number;
      b = pixels[j + 2] as number;
    } else if (colorType === 3) {
      const idx = pixels[j] as number;
      r = palette?.[idx * 3] ?? 0;
      g = palette?.[idx * 3 + 1] ?? 0;
      b = palette?.[idx * 3 + 2] ?? 0;
      al = trns?.[idx] ?? 255;
    } else if (colorType === 4) {
      r = g = b = pixels[j] as number;
      al = pixels[j + 1] as number;
    } else {
      r = pixels[j] as number;
      g = pixels[j + 1] as number;
      b = pixels[j + 2] as number;
      al = pixels[j + 3] as number;
    }
    out[i * 4] = r;
    out[i * 4 + 1] = g;
    out[i * 4 + 2] = b;
    out[i * 4 + 3] = al;
  }
  return { width, height, data: out };
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/** Encodes RGBA pixels as a PNG (filter 0, deflate level 6). */
export function encodePng(img: RgbaImage): Uint8Array {
  const ihdr = new Uint8Array(13);
  const v = new DataView(ihdr.buffer);
  v.setUint32(0, img.width);
  v.setUint32(4, img.height);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const stride = img.width * 4;
  const raw = new Uint8Array(img.height * (stride + 1));
  for (let y = 0; y < img.height; y++)
    raw.set(img.data.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
  const parts = [
    new Uint8Array(SIGNATURE),
    chunk('IHDR', ihdr),
    chunk('IDAT', new Uint8Array(deflateSync(raw, { level: 6 }))),
    chunk('IEND', new Uint8Array()),
  ];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** A solid image, for tests. */
export function solidImage(width: number, height: number, rgba: [number, number, number, number]): RgbaImage {
  const data = new Uint8Array(width * height * 4);
  for (let i = 0; i < width * height; i++) data.set(rgba, i * 4);
  return { width, height, data };
}
