import { expect, type Page } from '@playwright/test';

/** What `apps/web/src/routes/playground` exposes for these tests (kept structurally in step). */
interface PlaygroundHandle {
  client: {
    query(doc: number, q: string): Promise<unknown>;
    execute(doc: number, c: unknown): Promise<unknown>;
    a11y(doc: number): Promise<{ nodes: Array<{ id: number; text: string | null }>; active: number | null }>;
  };
  doc: number;
  controller: {
    getStats(): Stats;
    setBlink(b: boolean): void;
    setZoom(z: number): void;
    getZoom(): number;
    focus(): void;
    scrollToY(y: number): void;
  } | null;
  open(text: string, name?: string): Promise<number>;
}

declare global {
  interface Window {
    __kilnPlayground?: PlaygroundHandle;
  }
}

/**
 * Helpers for the surface e2e tests (`surface.spec.ts`, `painter-parity.spec.ts`): they drive the
 * playground route (`#/playground`) through `window.__kilnPlayground`.
 */

export interface Stats {
  frames: number;
  composites: number;
  tiles: number;
  tilesRendered: number;
  revision: number;
  zoom: number;
  scrollX: number;
  scrollY: number;
}

/** Opens the playground (without its toolbar) and waits for the first painted frame. */
export async function openPlayground(page: Page): Promise<void> {
  await page.goto('/#/playground?bare');
  await page.waitForFunction(() => (window.__kilnPlayground?.controller?.getStats().tiles ?? 0) > 0, null, {
    timeout: 30_000,
  });
}

/** Opens `text` as a scratch document in the playground and waits until it is painted. */
export async function openText(page: Page, text: string, name = 'test.txt'): Promise<number> {
  const doc = await page.evaluate(
    ([t, n]) => window.__kilnPlayground?.open(t as string, n as string),
    [text, name],
  );
  await page.waitForFunction(
    (d) =>
      window.__kilnPlayground?.doc === d && (window.__kilnPlayground?.controller?.getStats().tiles ?? 0) > 0,
    doc,
    { timeout: 30_000 },
  );
  await settle(page);
  return doc as number;
}

/** The surface's counters. */
export function stats(page: Page): Promise<Stats> {
  return page.evaluate(() => window.__kilnPlayground?.controller?.getStats() as Stats);
}

/** Waits until no frame has arrived for a few checks (the surface is up to date). */
export async function settle(page: Page): Promise<void> {
  let last = -1;
  let stable = 0;
  for (let i = 0; i < 100 && stable < 3; i++) {
    const s = await stats(page);
    if (s.frames === last) stable += 1;
    else stable = 0;
    last = s.frames;
    await page.waitForTimeout(80);
  }
}

/** The document's paragraphs, from the core. */
export async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const pg = window.__kilnPlayground;
    if (!pg) return [];
    const r = (await pg.client.query(pg.doc, 'Paragraphs')) as { Paragraphs?: Array<{ text: string }> };
    return r.Paragraphs?.map((p) => p.text) ?? [];
  });
}

/** The selected text, from the core. */
export async function selectedText(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const pg = window.__kilnPlayground;
    if (!pg) return '';
    const r = (await pg.client.query(pg.doc, 'SelectedText')) as { Text?: string };
    return r.Text ?? '';
  });
}

/** Client (CSS pixel) coordinates of a document point. */
export async function toClient(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = await page.getByTestId('kiln-surface-scroller').boundingBox();
  const s = await stats(page);
  expect(box).not.toBeNull();
  return { x: (box?.x ?? 0) + (x - s.scrollX) * s.zoom, y: (box?.y ?? 0) + (y - s.scrollY) * s.zoom };
}

/** Resizes the window so the surface's drawable area is exactly `width × height` CSS pixels. */
export async function sizeSurface(page: Page, width: number, height: number): Promise<void> {
  for (let i = 0; i < 3; i++) {
    const m = await page.evaluate(() => {
      const s = document.querySelector('[data-testid="kiln-surface-scroller"]') as HTMLElement;
      return {
        bw: s.offsetWidth - s.clientWidth,
        bh: s.offsetHeight - s.clientHeight,
        cw: s.clientWidth,
        ch: s.clientHeight,
      };
    });
    if (m.cw === width && m.ch === height) return;
    await page.setViewportSize({ width: width + m.bw, height: height + m.bh });
    await page.waitForTimeout(150);
    await settle(page);
  }
  throw new Error('could not size the surface');
}

/** The pixels the user sees (content canvas with the overlay on top), as RGBA. */
export async function surfacePixels(page: Page): Promise<{ width: number; height: number; data: Buffer }> {
  const r = await page.evaluate(() => {
    const content = document.querySelector('[data-testid="kiln-surface-content"]') as HTMLCanvasElement;
    const overlay = document.querySelector('[data-testid="kiln-surface-overlay"]') as HTMLCanvasElement;
    const c = document.createElement('canvas');
    c.width = content.width;
    c.height = content.height;
    const ctx = c.getContext('2d') as CanvasRenderingContext2D;
    ctx.drawImage(content, 0, 0);
    ctx.drawImage(overlay, 0, 0);
    const data = ctx.getImageData(0, 0, c.width, c.height).data;
    let bin = '';
    for (let i = 0; i < data.length; i += 0x8000) {
      bin += String.fromCharCode(...data.subarray(i, i + 0x8000));
    }
    return { width: c.width, height: c.height, b64: btoa(bin) };
  });
  return { width: r.width, height: r.height, data: Buffer.from(r.b64, 'base64') };
}

/** Decodes a PNG in the browser to RGBA. */
export async function decodePng(
  page: Page,
  png: Buffer,
): Promise<{ width: number; height: number; data: Buffer }> {
  const r = await page.evaluate(async (b64) => {
    const bytes = Uint8Array.from(atob(b64), (ch) => ch.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }), {
      premultiplyAlpha: 'none',
      colorSpaceConversion: 'none',
    });
    const c = document.createElement('canvas');
    c.width = bmp.width;
    c.height = bmp.height;
    const ctx = c.getContext('2d') as CanvasRenderingContext2D;
    ctx.drawImage(bmp, 0, 0);
    const data = ctx.getImageData(0, 0, c.width, c.height).data;
    let bin = '';
    for (let i = 0; i < data.length; i += 0x8000) bin += String.fromCharCode(...data.subarray(i, i + 0x8000));
    return { width: c.width, height: c.height, b64: btoa(bin) };
  }, png.toString('base64'));
  return { width: r.width, height: r.height, data: Buffer.from(r.b64, 'base64') };
}

/** Encodes RGBA to a PNG in the browser (for diff images). */
export async function encodePng(
  page: Page,
  img: { width: number; height: number; data: Uint8Array },
): Promise<Buffer> {
  const b64 = await page.evaluate(
    ([w, h, d]) => {
      const bytes = Uint8ClampedArray.from(atob(d as string), (ch) => ch.charCodeAt(0));
      const c = document.createElement('canvas');
      c.width = w as number;
      c.height = h as number;
      (c.getContext('2d') as CanvasRenderingContext2D).putImageData(
        new ImageData(bytes, w as number, h as number),
        0,
        0,
      );
      return c.toDataURL('image/png').split(',')[1] as string;
    },
    [img.width, img.height, Buffer.from(img.data).toString('base64')] as const,
  );
  return Buffer.from(b64, 'base64');
}
