/**
 * Helpers of the Deck specs: a new or opened presentation in the web app, the core's state and
 * commands through the editor model of development builds (`window.__kilnDeck`), and saving and
 * reopening through the core's own `.pptx` writer (`DeckQuery::Snapshot`).
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page } from '@playwright/test';

export interface DeckShape {
  id: number;
  name: string;
  kind: string;
  hidden: boolean;
}

/** The parts of `DeckState` the specs read. */
export interface DeckStateView {
  current: number;
  slides: { id: number; title: string; layout: string; hidden: boolean }[];
  shapes: DeckShape[];
  selection: number[];
  contexts: string[];
  editing: boolean;
  notes: string;
  theme: string;
  chart: null | {
    kind: string | null;
    style: number | null;
    series: { name: string }[];
    has_cells: boolean;
  };
  [key: string]: unknown;
}

interface DeckHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  fetch(): Promise<unknown>;
  set(patch: Record<string, unknown>): void;
  openDialog(id: string): void;
  stage: { focus(): void } | null;
  state: Record<string, unknown>;
}

export type DeckWindow = {
  __kilnDeck: DeckHandle;
  __kilnDeckStage?: {
    stats: { frames: number };
    view(): { left: number; top: number; zoom: number };
  } | null;
  __kiln?: { client?: { open(bytes: Uint8Array, hint: object): Promise<{ doc: number }> } | null };
};

const CORE_START = 60_000;

/** A file of the repository. */
export const repoFile = (rel: string) => fileURLToPath(new URL(`../../${rel}`, import.meta.url));

/**
 * Waits until the stage has painted and the fonts are in. A phone opens a presentation on its slide
 * list: the first slide is opened to edit, as a tap on it does.
 */
export async function waitForStage(page: Page) {
  await page.waitForFunction(
    () =>
      ((window as unknown as DeckWindow).__kilnDeckStage?.stats.frames ?? 0) > 0 ||
      !!document.querySelector('[data-testid="deck-phone-list"] [data-slide-index]'),
    null,
    { timeout: CORE_START },
  );
  await openPhoneSlide(page);
  await page.waitForFunction(
    () => ((window as unknown as DeckWindow).__kilnDeckStage?.stats.frames ?? 0) > 0,
    null,
    { timeout: CORE_START },
  );
  await page.evaluate(() => document.fonts.ready);
}

/** On a phone, opens the current slide from the slide list (the stage shows it full width). */
export async function openPhoneSlide(page: Page) {
  const slide = page.locator('[data-testid="deck-phone-list"] [data-current]');
  if (await slide.count()) await slide.first().click();
}

/** A new presentation. */
export async function newDeck(page: Page) {
  await page.goto('/#/deck?blink=0');
  await waitForStage(page);
}

/** Opens `.pptx` bytes in the web core and shows them in Kiln Deck. */
export async function openBytes(page: Page, bytes: Uint8Array | Buffer, name: string, format = 'Pptx') {
  if (!page.url().includes('#/')) await newDeck(page);
  const doc = await page.evaluate(
    async ([b, file, fmt]) => {
      const client = (window as unknown as DeckWindow).__kiln?.client;
      if (!client) throw new Error('window.__kiln.client is missing (development build only)');
      const s = await client.open(Uint8Array.from(b as number[]), {
        name: file as string,
        path: null,
        format: fmt as string,
        read_only: false,
      });
      return s.doc;
    },
    [Array.from(bytes), name, format] as const,
  );
  await page.goto(`/#/deck?doc=${doc}&blink=0`);
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.[a-z]+$/, ''), {
    timeout: 30_000,
  });
  await waitForStage(page);
}

/** Opens a fixture of `fixtures/pptx/features/`. */
export async function openFixture(page: Page, name: string) {
  await newDeck(page);
  await openBytes(page, readFileSync(repoFile(`fixtures/pptx/features/${name}`)), name);
}

/** Runs a Deck command on the core. */
export async function deck(page: Page, command: unknown) {
  await page.evaluate(async (c) => {
    const d = (window as unknown as DeckWindow).__kilnDeck;
    await d.client.execute(d.doc, { Deck: c });
    await d.fetch();
  }, command);
}

/** Runs a suite command (a chart command) on the core. */
export async function suite(page: Page, command: unknown) {
  await page.evaluate(async (c) => {
    const d = (window as unknown as DeckWindow).__kilnDeck;
    await d.client.execute(d.doc, { Suite: c });
    await d.fetch();
  }, command);
}

/** Asks the Deck core. */
export async function query<T = unknown>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (q) => {
    const d = (window as unknown as DeckWindow).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: q })) as { Deck: Record<string, unknown> };
    return Object.values(r.Deck)[0] as T;
  }, q) as Promise<T>;
}

/** The core's state of the presentation. */
export function state(page: Page): Promise<DeckStateView> {
  return query<DeckStateView>(page, 'State');
}

/** The presentation as the core saves it (`.pptx` bytes). */
export async function saved(page: Page): Promise<Buffer> {
  const bytes = await query<number[]>(page, 'Snapshot');
  return Buffer.from(bytes);
}

/** Saves the presentation and opens what was saved in a new window of the app. */
export async function saveAndReopen(page: Page, name = 'Reopened.pptx'): Promise<Buffer> {
  const bytes = await saved(page);
  await openBytes(page, bytes, name);
  return bytes;
}

/** Selects the first shape of the current slide whose kind is `kind`. */
export async function selectKind(page: Page, kind: string): Promise<number> {
  const st = await state(page);
  const shape = st.shapes.find((s) => s.kind === kind);
  if (!shape) throw new Error(`no ${kind} on the slide: ${st.shapes.map((s) => s.kind).join(', ')}`);
  await deck(page, { SelectShapes: { ids: [shape.id] } });
  return shape.id;
}

/** Shows a ribbon tab by its name. */
export async function ribbonTab(page: Page, name: string) {
  await page.getByRole('tab', { name, exact: true }).click();
}

/** Page coordinates of a point of the slide (slide px). */
export async function onSlide(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = await page.getByTestId('deck-stage').boundingBox();
  const v = await page.evaluate(() => (window as unknown as DeckWindow).__kilnDeckStage?.view());
  if (!box || !v) throw new Error('no stage');
  return { x: box.x + v.left + x * v.zoom, y: box.y + v.top + y * v.zoom };
}

/** Drags on the slide from one point to another (slide px). */
export async function dragOnSlide(
  page: Page,
  from: [number, number],
  to: [number, number],
  modifiers: { shift?: boolean } = {},
) {
  const a = await onSlide(page, ...from);
  const b = await onSlide(page, ...to);
  if (modifiers.shift) await page.keyboard.down('Shift');
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 4 });
  await page.mouse.move(b.x, b.y, { steps: 4 });
  await page.mouse.up();
  if (modifiers.shift) await page.keyboard.up('Shift');
}

/** One entry of a zip package (a saved `.pptx`), inflated, or `''`. */
export function zipEntry(zip: Buffer, name: string): string {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    if (zip.subarray(at + 46, at + 46 + nameLen).toString('utf8') === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return (method === 0 ? Buffer.from(data) : inflateRawSync(data)).toString('utf8');
    }
    at += 46 + nameLen + extra + comment;
  }
  return '';
}

/** A PNG drawn in the page: `w` × `h`, the left half orange, the right half blue. */
export async function twoTonePng(page: Page, w = 400, h = 200): Promise<number[]> {
  return page.evaluate(
    async ([w, h]) => {
      const c = document.createElement('canvas');
      c.width = w;
      c.height = h;
      const g = c.getContext('2d');
      if (!g) throw new Error('no canvas');
      g.fillStyle = '#E05A27';
      g.fillRect(0, 0, w / 2, h);
      g.fillStyle = '#2B5FB4';
      g.fillRect(w / 2, 0, w / 2, h);
      const blob = await new Promise<Blob | null>((r) => c.toBlob(r, 'image/png'));
      if (!blob) throw new Error('no png');
      return Array.from(new Uint8Array(await blob.arrayBuffer()));
    },
    [w, h] as const,
  );
}
