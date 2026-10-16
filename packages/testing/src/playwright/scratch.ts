/**
 * Driving a scratch document in the web build: the playground route (`#/playground`) exposes
 * `window.__kilnPlayground` (client, document id, surface controller). Text is typed through the
 * hidden input proxy like a user would, and IME composition goes through real CDP IME events.
 */
import type { Page } from '@playwright/test';

/** What the playground exposes (kept structurally in step with `apps/web/src/routes/playground`). */
export interface PlaygroundHandle {
  client: {
    query(doc: number, q: string): Promise<unknown>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  doc: number;
  controller: {
    getStats(): { frames: number; tiles: number };
    setBlink(b: boolean): void;
    focus(): void;
  } | null;
  open(text: string, name?: string): Promise<number>;
}

declare global {
  interface Window {
    __kilnPlayground?: PlaygroundHandle;
  }
}

/** The input proxy's test id. */
export const INPUT_PROXY = 'kiln-input-proxy';

export interface OpenScratchOptions {
  /** Initial text (paragraphs separated by `\n`); omitted keeps the playground's sample. */
  text?: string;
  /** File name shown as the title. */
  name?: string;
  /** Stop the caret from blinking (stable screenshots). Default true. */
  steadyCaret?: boolean;
  /** Time to wait for the first painted frame, in ms. Default 30 s. */
  timeout?: number;
}

/** Waits until no new frame has arrived for three consecutive checks. */
export async function settleFrames(page: Page, intervalMs = 80): Promise<void> {
  let last = -1;
  let stable = 0;
  for (let i = 0; i < 100 && stable < 3; i++) {
    const frames = await page.evaluate(() => window.__kilnPlayground?.controller?.getStats().frames ?? -1);
    stable = frames === last ? stable + 1 : 0;
    last = frames;
    await page.waitForTimeout(intervalMs);
  }
}

/** Opens the playground with a scratch document, focused and painted; returns the document id. */
export async function openScratch(page: Page, options: OpenScratchOptions = {}): Promise<number> {
  const timeout = options.timeout ?? 30_000;
  await page.goto('/#/playground?bare');
  await page.waitForFunction(() => (window.__kilnPlayground?.controller?.getStats().tiles ?? 0) > 0, null, {
    timeout,
  });
  let doc = await page.evaluate(() => window.__kilnPlayground?.doc ?? -1);
  if (options.text !== undefined) {
    doc = await page.evaluate(
      ([text, name]) => window.__kilnPlayground?.open(text as string, name as string) ?? Promise.resolve(-1),
      [options.text, options.name ?? 'scratch.txt'] as const,
    );
    await page.waitForFunction(
      (d) =>
        window.__kilnPlayground?.doc === d &&
        (window.__kilnPlayground?.controller?.getStats().tiles ?? 0) > 0,
      doc,
      { timeout },
    );
  }
  if (doc < 0) throw new Error('the playground did not open a document');
  const steady = options.steadyCaret ?? true;
  await page.evaluate((s) => {
    const c = window.__kilnPlayground?.controller;
    if (s) c?.setBlink(false);
    c?.focus();
  }, steady);
  await settleFrames(page);
  return doc;
}

/** Types text through the focused input proxy, key by key (`\n` presses Enter). */
export async function typeViaProxy(page: Page, text: string, delayMs = 0): Promise<void> {
  const proxy = page.getByTestId(INPUT_PROXY);
  await proxy.focus();
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] as string;
    if (line) await page.keyboard.type(line, { delay: delayMs });
    if (i < lines.length - 1) await page.keyboard.press('Enter');
  }
  await settleFrames(page);
}

/**
 * Composes text with an IME through the Chrome DevTools Protocol, as a Japanese or Chinese input
 * method does: each step replaces the composition (`に`, `にほ`, `にほん`), then `commit` is inserted
 * (or the composition is cancelled when `commit` is `null`). Chromium only.
 */
export async function composeIme(page: Page, steps: readonly string[], commit: string | null): Promise<void> {
  await page.getByTestId(INPUT_PROXY).focus();
  const cdp = await page.context().newCDPSession(page);
  try {
    for (const text of steps) {
      await cdp.send('Input.imeSetComposition', {
        text,
        selectionStart: text.length,
        selectionEnd: text.length,
      });
    }
    if (commit === null) {
      await cdp.send('Input.imeSetComposition', { text: '', selectionStart: 0, selectionEnd: 0 });
    } else {
      await cdp.send('Input.insertText', { text: commit });
    }
  } finally {
    await cdp.detach();
  }
  await settleFrames(page);
}

/** The scratch document's paragraphs, from the core. */
export async function scratchParagraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const pg = window.__kilnPlayground;
    if (!pg) return [];
    const r = (await pg.client.query(pg.doc, 'Paragraphs')) as { Paragraphs?: Array<{ text: string }> };
    return r.Paragraphs?.map((p) => p.text) ?? [];
  });
}
