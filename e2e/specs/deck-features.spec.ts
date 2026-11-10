/**
 * Kiln Deck's Design Ideas, exports, Outline View and Reuse Slides in the browser: the
 * Designer pane's pictures and one applied (and undone in one step), File ▸ Export as PNG (one
 * slide, and every slide in a .zip) and as PDF — slides, handouts three per page, notes pages —
 * with the bytes checked (PNG signature and size, PDF page count), File ▸ Print's handout preview,
 * Outline View typed into, split, demoted and promoted with the slides following, Reuse Slides,
 * and a video played over its slide in the show. Screenshots
 * of the panes go to e2e/screenshots/deck-features/.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { type Download, expect, type Page, test } from '@playwright/test';

const shot = (name: string) =>
  fileURLToPath(new URL(`../screenshots/deck-features/${name}`, import.meta.url));
const fixturePath = (name: string) =>
  fileURLToPath(new URL(`../../fixtures/pptx/features/${name}`, import.meta.url));
const mediaPath = (name: string) => fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url));

interface OutlineEntry {
  slide: number;
  title: string;
  lines: { level: number; text: string }[];
  has_title: boolean;
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
  state: { designPane: boolean; outline: boolean };
}
type W = {
  __kilnDeck: DeckHandle;
  __kilnDeckStage?: { stats: { frames: number } } | null;
  __kiln?: { client?: { open(bytes: Uint8Array, hint: object): Promise<{ doc: number }> } | null };
  __kilnShowEngine?: { frames: number } | null;
};

const CORE_START = 60_000;

async function waitForStage(page: Page) {
  await page.waitForFunction(() => ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > 0, null, {
    timeout: CORE_START,
  });
  await page.evaluate(() => document.fonts.ready);
}

/** A new presentation. */
async function newDeck(page: Page) {
  await page.goto('/#/deck?blink=0');
  await waitForStage(page);
}

/** Opens a fixture in the web core and shows it in Kiln Deck. */
async function openFixture(page: Page, name: string) {
  await newDeck(page);
  const bytes = Array.from(readFileSync(fixturePath(name)));
  const doc = await page.evaluate(
    async ([b, file]) => {
      const client = (window as unknown as W).__kiln?.client;
      if (!client) throw new Error('window.__kiln.client is missing (development build only)');
      const s = await client.open(Uint8Array.from(b as number[]), {
        name: file as string,
        path: null,
        format: 'Pptx',
        read_only: false,
      });
      return s.doc;
    },
    [bytes, name] as const,
  );
  await page.goto(`/#/deck?doc=${doc}&blink=0`);
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.pptx$/, ''), {
    timeout: 30_000,
  });
  await waitForStage(page);
}

async function core(page: Page, command: unknown) {
  await page.evaluate(async (c) => {
    const d = (window as unknown as W).__kilnDeck;
    await d.client.execute(d.doc, { Deck: c });
    await d.fetch();
  }, command);
}

async function outline(page: Page): Promise<OutlineEntry[]> {
  return page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'OutlineView' })) as {
      Deck: { OutlineView: OutlineEntry[] };
    };
    return r.Deck.OutlineView;
  });
}

async function shapeNames(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
      Deck: { State: { shapes: { name: string }[] } };
    };
    return r.Deck.State.shapes.map((s) => s.name);
  });
}

/** A Title and Content slide after the title slide, with a title and five bullets. */
async function textSlide(page: Page) {
  await core(page, { NewSlide: { layout: null } });
  await core(page, { EditOutline: { edit: { Text: { slide: 1, line: 0, text: 'Firing schedule' } } } });
  expect(await outline(page)).toHaveLength(2);
  // The bullets are typed on the stage.
  await page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
      Deck: { State: { shapes: { id: number; name: string; kind: string }[] } };
    };
    const body = r.Deck.State.shapes.find((s) => !/^Title/.test(s.name) && s.kind === 'placeholder');
    if (!body) throw new Error('no body placeholder');
    await d.client.execute(d.doc, { Deck: { SelectShapes: { ids: [body.id] } } });
    await d.client.execute(d.doc, { Deck: 'EditText' });
    await d.client.execute(d.doc, { Deck: { InsertText: { text: 'Bisque at 950 °C' } } });
    for (const t of ['Glaze at 1240 °C', 'Cool for a day', 'Unload', 'Sort the pots']) {
      await d.client.execute(d.doc, { Deck: 'NewParagraph' });
      await d.client.execute(d.doc, { Deck: { InsertText: { text: t } } });
    }
    await d.client.execute(d.doc, { Deck: 'Escape' });
    await d.client.execute(d.doc, { Deck: 'Escape' });
    await d.fetch();
  });
}

/** The bytes of a download. */
async function bytesOf(download: Download): Promise<Buffer> {
  const path = await download.path();
  if (!path) throw new Error('the download has no file');
  return readFileSync(path);
}

/** The pages of a PDF: its page objects (`/Type /Page`, not `/Pages` or `/PageLabel`). */
function pdfPages(pdf: Buffer): number {
  return [...pdf.toString('latin1').matchAll(/\/Type\s*\/Page(?![A-Za-z])/g)].length;
}

/** Width and height from a PNG's IHDR. */
function pngSize(png: Buffer): [number, number] {
  return [png.readUInt32BE(16), png.readUInt32BE(20)];
}

async function openBackstage(page: Page, tab: 'Export' | 'Print') {
  await page.getByRole('button', { name: 'File' }).click();
  const dialog = page.getByRole('dialog', { name: /File menu/ });
  await dialog.getByRole('button', { name: tab, exact: true }).click();
  return dialog;
}

test.describe('deck-features', () => {
  test('Design Ideas shows pictures of the slide and applies one in one undo step', async ({ page }) => {
    await newDeck(page);
    await textSlide(page);
    await page.getByRole('tab', { name: 'Design', exact: true }).click();
    await page.getByRole('button', { name: 'Design Ideas' }).click();
    const pane = page.getByTestId('deck-design-pane');
    await expect(pane).toBeVisible();
    const ideas = pane.locator('.kiln-designpane__idea');
    await expect(ideas.first()).toBeVisible({ timeout: 20_000 });
    const n = await ideas.count();
    expect(n).toBeGreaterThanOrEqual(4);
    expect(n).toBeLessThanOrEqual(6);
    // Every picture decoded, at the pane's width.
    await expect
      .poll(() =>
        pane.locator('img').evaluateAll((imgs) => imgs.map((i) => (i as HTMLImageElement).naturalWidth)),
      )
      .toEqual(Array(n).fill(expect.any(Number)));
    const widths = await pane
      .locator('img')
      .evaluateAll((imgs) => imgs.map((i) => (i as HTMLImageElement).naturalWidth));
    expect(widths.every((w) => w >= 248)).toBe(true);
    await page.screenshot({ path: shot('design-ideas.png') });
    const before = await shapeNames(page);
    await pane.getByRole('button', { name: 'Title band' }).click();
    await expect.poll(() => shapeNames(page)).toContain('Design Element 1');
    // Another idea replaces the first's decoration.
    await pane.getByRole('button', { name: 'Side panel' }).click();
    await expect
      .poll(async () => (await shapeNames(page)).filter((s) => s.startsWith('Design Element')).length)
      .toBe(1);
    await page.screenshot({ path: shot('design-ideas-applied.png') });
    await page.keyboard.press('Control+z');
    await page.keyboard.press('Control+z');
    await expect.poll(() => shapeNames(page)).toEqual(before);
  });

  test('Export as PNG saves one slide, and every slide as a zip of a folder', async ({ page }) => {
    await openFixture(page, 'placeholders.pptx');
    const dialog = await openBackstage(page, 'Export');
    const pane = dialog.getByTestId('deck-export-pictures');
    await expect(pane).toBeVisible();
    await page.screenshot({ path: shot('export.png') });
    await pane.getByRole('combobox', { name: 'Slides' }).click();
    await page.getByRole('option', { name: 'Just This One' }).click();
    const one = page.waitForEvent('download');
    await pane.getByTestId('deck-export-pictures-go').click();
    const file = await one;
    expect(file.suggestedFilename()).toBe('placeholders.png');
    const png = await bytesOf(file);
    expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    expect(pngSize(png)).toEqual([1920, 1080]);
    await pane.getByRole('combobox', { name: 'Slides' }).click();
    await page.getByRole('option', { name: 'All Slides' }).click();
    await pane.getByRole('combobox', { name: 'Size' }).click();
    await page.getByRole('option', { name: /1280/ }).click();
    const all = page.waitForEvent('download');
    await pane.getByTestId('deck-export-pictures-go').click();
    const zip = await bytesOf(await all);
    expect(zip.subarray(0, 2).toString('latin1')).toBe('PK');
    const names = [...zip.toString('latin1').matchAll(/placeholders\/Slide(\d+)\.png/g)].map((m) =>
      Number(m[1]),
    );
    expect(new Set(names).size).toBe(11);
  });

  test('Export as PDF saves slides, handouts three per page and notes pages', async ({ page }) => {
    await openFixture(page, 'placeholders.pptx');
    const dialog = await openBackstage(page, 'Export');
    const pdfPane = dialog.getByTestId('pdf-export');
    const exportAs = async (what: string | null) => {
      if (what) {
        await pdfPane.getByRole('combobox', { name: 'Publish what' }).click();
        await page.getByRole('option', { name: what, exact: true }).click();
      }
      const download = page.waitForEvent('download');
      await pdfPane.getByTestId('pdf-export-go').click();
      const pdf = await bytesOf(await download);
      expect(pdf.subarray(0, 5).toString('latin1')).toBe('%PDF-');
      return pdf;
    };
    // Eleven slides.
    const slides = await exportAs(null);
    expect(pdfPages(slides)).toBe(11);
    const handouts = await exportAs('3 Slides');
    expect(pdfPages(handouts)).toBe(Math.ceil(11 / 3));
    // Portrait pages of the notes size (7.5 × 10 in = 540 × 720 pt).
    expect(handouts.toString('latin1')).toMatch(/\/MediaBox\s*\[\s*0\s+0\s+540\s+720\s*\]/);
    await page.screenshot({ path: shot('export-pdf-handouts.png') });
    const notes = await exportAs('Notes Pages');
    expect(pdfPages(notes)).toBe(11);
  });

  test('Print previews handouts and notes pages', async ({ page }) => {
    await openFixture(page, 'notes.pptx');
    const dialog = await openBackstage(page, 'Print');
    const sheet = dialog.getByTestId('print-sheet');
    await expect(sheet).toHaveText(/Page 1 of \d+/, { timeout: 30_000 });
    const slides = Number((await sheet.textContent())?.match(/of (\d+)/)?.[1]);
    await dialog.getByRole('combobox', { name: 'Print layout' }).click();
    await page.getByRole('option', { name: '6 Slides Horizontal' }).click();
    await expect(sheet).toHaveText(`Page 1 of ${Math.ceil(slides / 6)}`, { timeout: 15_000 });
    await expect(dialog.getByRole('checkbox', { name: 'Frame Slides' })).toBeChecked();
    await page.screenshot({ path: shot('print-handouts.png') });
    await dialog.getByRole('combobox', { name: 'Print layout' }).click();
    await page.getByRole('option', { name: 'Notes Pages' }).click();
    await expect(sheet).toHaveText(`Page 1 of ${slides}`, { timeout: 15_000 });
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('print-notes.png') });
  });

  test('Outline View: typing, Enter, Tab and Shift+Tab edit the slides', async ({ page }) => {
    await newDeck(page);
    await textSlide(page);
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.getByRole('button', { name: 'Outline View', exact: true }).click();
    const pane = page.getByTestId('deck-outline');
    await expect(pane).toBeVisible();
    await expect(pane.getByRole('textbox', { name: 'Slide 2, line 5' })).toHaveText('Sort the pots');
    await page.screenshot({ path: shot('outline.png') });

    // Typing into the first slide's title shows on the slide.
    const title = pane.getByRole('textbox', { name: 'Slide 1 title' });
    await title.click();
    await page.keyboard.type('Studio year');
    await expect.poll(async () => (await outline(page))[0]?.title).toBe('Studio year');
    await expect
      .poll(() =>
        page.evaluate(async () => {
          const d = (window as unknown as W).__kilnDeck;
          const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
            Deck: { State: { slides: { title: string }[] } };
          };
          return r.Deck.State.slides.map((s) => s.title);
        }),
      )
      .toEqual(['Studio year', 'Firing schedule']);

    // Shift+Tab on a first-level line makes it a new slide with the lines after it.
    const unload = pane.getByRole('textbox', { name: 'Slide 2, line 4' });
    await unload.click();
    await page.keyboard.press('Home');
    await page.keyboard.press('Shift+Tab');
    await expect
      .poll(async () => (await outline(page)).map((e) => e.title))
      .toEqual(['Studio year', 'Firing schedule', 'Unload']);
    await expect
      .poll(async () => (await outline(page))[2]?.lines.map((l) => l.text))
      .toEqual(['Sort the pots']);
    // The caret stays in the line, now the new slide's title.
    await page.keyboard.type('ing');
    await expect.poll(async () => (await outline(page))[2]?.title).toBe('Unloading');

    // Tab on that title makes it part of slide 2 again, its line a level down.
    await page.keyboard.press('Tab');
    await expect.poll(async () => (await outline(page)).length).toBe(2);
    const lines = (await outline(page))[1]?.lines.map((l) => [l.level, l.text]);
    expect(lines).toEqual([
      [0, 'Bisque at 950 °C'],
      [0, 'Glaze at 1240 °C'],
      [0, 'Cool for a day'],
      [0, 'Unloading'],
      [1, 'Sort the pots'],
    ]);

    // Enter at the end of a line starts a new one; what is typed goes into it.
    await pane.getByRole('textbox', { name: 'Slide 2, line 1' }).click();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Check the cones');
    await expect.poll(async () => (await outline(page))[1]?.lines[1]?.text).toBe('Check the cones');
    await page.screenshot({ path: shot('outline-edited.png') });

    // Every structural edit undoes on its own.
    await page.keyboard.press('Control+z');
    await expect.poll(async () => (await outline(page))[1]?.lines.length).toBe(6);
    // Normal view brings the thumbnails back.
    await page.getByRole('button', { name: 'Outline View', exact: true }).click();
    await expect(pane).toBeHidden();
    await expect(page.getByTestId('deck-rail')).toBeVisible();
  });

  test("Reuse Slides inserts another presentation's slide on this one's layouts", async ({ page }) => {
    await newDeck(page);
    // Reuse Slides is a P1 command: the pane is opened through the editor's model here.
    await page.evaluate(() => (window as unknown as W).__kilnDeck.set({ reusePane: true }));
    const pane = page.getByTestId('deck-reuse-pane');
    await pane.getByTestId('deck-reuse-file').setInputFiles(fixturePath('sections.pptx'));
    const slides = pane.locator('.kiln-reusepane__slide');
    await expect(slides.first()).toBeVisible({ timeout: 20_000 });
    const offered = await slides.count();
    expect(offered).toBeGreaterThan(1);
    await page.screenshot({ path: shot('reuse-slides.png') });
    await slides.nth(1).click();
    await expect.poll(async () => (await outline(page)).length).toBe(2);
    await pane.getByTestId('deck-reuse-all').click();
    await expect.poll(async () => (await outline(page)).length).toBe(2 + offered);
  });

  test('a video plays over its slide in the show, without advancing it', async ({ page }) => {
    await newDeck(page);
    const clip = Array.from(readFileSync(mediaPath('kiln-clip.webm')));
    // Insert ▸ Video is a P1 command (hidden in builds of the v0.1 tier): the clip goes to the
    // core as its button sends it, with the poster frame the browser reads from the video.
    await page.evaluate(async (bytes) => {
      const blob = new Blob([new Uint8Array(bytes)], { type: 'video/webm' });
      const v = document.createElement('video');
      v.muted = true;
      v.src = URL.createObjectURL(blob);
      await new Promise((r) => v.addEventListener('loadeddata', r, { once: true }));
      v.currentTime = 0.1;
      await new Promise((r) => v.addEventListener('seeked', r, { once: true }));
      const c = document.createElement('canvas');
      c.width = v.videoWidth;
      c.height = v.videoHeight;
      c.getContext('2d')?.drawImage(v, 0, 0);
      const png = await new Promise<Blob | null>((r) => c.toBlob(r, 'image/png'));
      const poster = Array.from(new Uint8Array(await (png as Blob).arrayBuffer()));
      const d = (window as unknown as W).__kilnDeck;
      await d.client.execute(d.doc, {
        Deck: {
          InsertMedia: {
            kind: 'Video',
            name: 'kiln-clip.webm',
            mime: 'video/webm',
            bytes,
            poster,
            width: c.width,
            height: c.height,
          },
        },
      });
      await d.fetch();
    }, clip);
    await expect.poll(() => shapeNames(page)).toContain('kiln-clip');
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await page.screenshot({ path: shot('video-stage.png') });
    await page.evaluate(() => (window as unknown as W).__kilnDeck.stage?.focus());
    await page.keyboard.press('F5');
    await page.waitForSelector('[data-testid="deck-show"]', { timeout: 15_000 });
    const clipBox = page.getByTestId('deck-show-clip');
    await expect(clipBox).toBeVisible({ timeout: 15_000 });
    // The clip sits on its shape: centred, two thirds of the slide wide (16:9 slide, 16:9 clip).
    const box = await clipBox.boundingBox();
    const view = page.viewportSize();
    expect(box && view).toBeTruthy();
    if (box && view) {
      expect(Math.abs(box.x + box.width / 2 - view.width / 2)).toBeLessThan(3);
      expect(box.width / box.height).toBeCloseTo(16 / 9, 1);
    }
    await clipBox.click();
    const video = clipBox.locator('video');
    await expect
      .poll(() => video.evaluate((v) => (v as HTMLVideoElement).currentTime), { timeout: 10_000 })
      .toBeGreaterThan(0.2);
    // Still on the first slide.
    await expect(page.getByTestId('deck-show')).toHaveAttribute('data-slide', '0');
    await page.screenshot({ path: shot('video-show.png') });
    await page.keyboard.press('Escape');
  });

  test('Insert Equation puts Office Math on the slide, drawn as its linear text', async ({ page }) => {
    await newDeck(page);
    // Insert ▸ Equation is a P1 command: its dialog is opened through the editor's model here.
    await page.evaluate(() => (window as unknown as W).__kilnDeck.openDialog('equation'));
    const dialog = page.getByTestId('deck-equation-dialog');
    await dialog.getByRole('button', { name: /Pythagoras/ }).click();
    await expect(dialog.getByLabel('Equation')).toHaveValue('a^2+b^2=c^2');
    await page.screenshot({ path: shot('equation-dialog.png') });
    await dialog.getByRole('button', { name: 'Insert' }).click();
    await expect(dialog).toBeHidden();
    await expect.poll(async () => (await shapeNames(page)).some((n) => n.startsWith('TextBox'))).toBe(true);
    const text = await page.evaluate(async () => {
      const d = (window as unknown as W).__kilnDeck;
      const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
        Deck: { State: { slides: { title: string }[] } };
      };
      return r.Deck.State.slides[0]?.title ?? '';
    });
    // A slide without a title is named by its first text: the equation's linear form.
    expect(text).toBe('a²+b²=c²');
    await page.keyboard.press('Escape');
    await page.screenshot({ path: shot('equation-slide.png') });
  });
});
