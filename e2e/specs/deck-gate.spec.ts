/**
 * The Kiln Deck integration gate: the Office file loop over real presentations (open
 * through File ▸ Open, type into a title, nudge a shape, add a slide on a layout with a title,
 * an oval with text and a fill, give it a transition, save with Ctrl+S, open the saved file again
 * with everything kept), LibreOffice opening every saved file and exporting it as PDF with the
 * same slides and text, and the screenshots of the visual review (the chrome in light and dark,
 * desktop and phone, the slide sorter, the show, Presenter View, the dialogs and the twelve
 * themes) checked against the reviewed baselines in `e2e/screenshots/baseline/deck/`.
 */
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import { decodePng } from '../../packages/testing/src/png.ts';
import {
  type DeckWindow,
  deck,
  dragOnSlide,
  newDeck,
  onSlide,
  openBytes,
  query,
  repoFile,
  ribbonTab,
  state,
  suite,
  twoTonePng,
  waitForStage,
} from './deck-helpers.ts';

const out = (p: string) => repoFile(`e2e/screenshots/deck-gate/${p}`);
const baseline = (name: string) => repoFile(`e2e/screenshots/baseline/deck/${name}`);

// ── files ───────────────────────────────────────────────────────────────────────────────────

/** Every entry of a zip package, inflated. */
function unzip(zip: Buffer): Map<string, Buffer> {
  const files = new Map<string, Buffer>();
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return files;
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const name = zip.subarray(at + 46, at + 46 + nameLen).toString('utf8');
    const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const data = zip.subarray(start, start + size);
    files.set(name, method === 0 ? Buffer.from(data) : inflateRawSync(data));
    at += 46 + nameLen + extra + comment;
  }
  return files;
}

/** The slide part of a saved package whose text includes `text`. */
function slideWith(zip: Buffer, text: string): string {
  for (const [name, bytes] of unzip(zip)) {
    if (!/^ppt\/slides\/slide\d+\.xml$/.test(name)) continue;
    const xml = bytes.toString('utf8');
    if (xml.includes(text)) return xml;
  }
  return '';
}

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
async function openFile(page: Page, name: string, buffer: Buffer): Promise<void> {
  await page.goto('/#/home?blink=0');
  await expect(
    page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
  ).toBeVisible({ timeout: 45_000 });
  await page.evaluate(async () => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
    const root = await navigator.storage.getDirectory();
    await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
  });
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
  await chooser.setFiles({ name, mimeType: 'application/octet-stream', buffer });
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.[a-z]+$/, ''), {
    timeout: 45_000,
  });
  await waitForStage(page);
}

/** The bytes of a saved file in the origin's private file system, once the package is complete. */
async function savedBytes(page: Page, name: string): Promise<Buffer> {
  let bytes: number[] | null = null;
  await expect
    .poll(
      async () => {
        bytes = await page.evaluate(async (n) => {
          try {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('documents');
            const file = await (await dir.getFileHandle(n)).getFile();
            return Array.from(new Uint8Array(await file.arrayBuffer()));
          } catch {
            return null;
          }
        }, name);
        return bytes !== null && Buffer.from(bytes).lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06])) > 0;
      },
      { message: `${name} is saved`, timeout: 30_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

/** The LibreOffice the file loop checks with (`SOFFICE`, else /usr/bin/soffice). */
const SOFFICE = process.env.SOFFICE ?? '/usr/bin/soffice';

/** Skips the test, saying why, when LibreOffice is missing and allowed to be; fails otherwise. */
function needLibreOffice(): void {
  if (existsSync(SOFFICE)) return;
  if (process.env.KILN_E2E_NO_LIBREOFFICE === '1') {
    console.log(`file loop: skipped, LibreOffice is not installed at ${SOFFICE} (KILN_E2E_NO_LIBREOFFICE=1)`);
    test.skip(true, `LibreOffice is not installed at ${SOFFICE} (KILN_E2E_NO_LIBREOFFICE=1)`);
    return;
  }
  throw new Error(
    `the file loop needs LibreOffice at ${SOFFICE} (set SOFFICE, or KILN_E2E_NO_LIBREOFFICE=1 to skip it)`,
  );
}

/** LibreOffice opens a saved presentation and exports it as PDF (`tools/oracle-soffice.sh`). */
function libreOfficePdf(file: string, dir: string): string {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  execFileSync(repoFile('tools/oracle-soffice.sh'), [file, 'pdf', dir], {
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 180_000,
  });
  const pdf = readdirSync(dir).find((f) => f.endsWith('.pdf'));
  if (!pdf) throw new Error(`LibreOffice wrote no PDF for ${file}`);
  return `${dir}/${pdf}`;
}

/**
 * The pages of a PDF, each as its text (poppler's `pdftotext`) in reading order and in the order
 * the file draws it (`-raw`).
 */
function pdfPages(pdf: string): { text: string; raw: string }[] {
  const read = (mode: string[]) => {
    const text = execFileSync('pdftotext', [...mode, '-enc', 'UTF-8', pdf, '-'], {
      encoding: 'utf8',
      timeout: 60_000,
    });
    const pages = text.split('\f');
    if (pages.at(-1)?.trim() === '') pages.pop();
    return pages;
  };
  const raw = read(['-raw']);
  return read([]).map((text, i) => ({ text, raw: raw[i] ?? '' }));
}

/** Text as compared: compatibility forms (ligatures), runs of whitespace as one space, any case (All Caps). */
// Bidirectional marks are invisible, and PDF text keeps them where the document does not.
const squeeze = (s: string) =>
  s
    .normalize('NFKC')
    .replace(/[\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();

/** One shape's text and one slide's (`DeckQuery::SlideText`). */
interface ShapeText {
  name: string;
  kind: string;
  lines: string[];
}
interface SlideText {
  slide: number;
  shapes: number;
  texts: ShapeText[];
  notes: string[];
  hidden: boolean;
}

/** A slide's text as compared between two sessions (node ids are each session's own). */
const comparable = (s: SlideText | undefined) =>
  s ? { shapes: s.shapes, texts: s.texts, notes: s.notes, hidden: s.hidden } : null;

/** One slide of Outline View (`DeckQuery::OutlineView`). */
interface OutlineEntry {
  title: string;
  lines: { level: number; text: string }[];
  has_title: boolean;
  hidden: boolean;
}

/** Whether a shape is a slide's title placeholder (`Title 1`, not `Subtitle 2`). */
const isTitle = (s: { kind: string; name: string }) =>
  s.kind === 'placeholder' && /title/i.test(s.name) && !/subtitle/i.test(s.name);

/** Text as compared when a line is wrapped inside a word (`flowchart_direct_` / `access_storage`). */
const unspaced = (s: string) => squeeze(s).replace(/ /g, '');

/**
 * Whether a page shows a line: its characters in order with only whitespace between them (a line
 * wrapped in its box, even inside a word, is still one), in the page's text as poppler reads it
 * in reading order or in the order LibreOffice wrote it.
 */
const shows = (page: { text: string; raw: string }, line: string) =>
  [page.text, page.raw].some(
    (t) => squeeze(t).includes(squeeze(line)) || unspaced(t).includes(unspaced(line)),
  );

/** The current slide's transition (the parts of `DeckState.motion` the loop reads). */
const transitionOf = async (page: Page) =>
  ((await state(page)).motion as { transition: { effect: string } | null }).transition?.effect ?? null;

/** The selected shapes' box, in points. */
const boxOf = async (page: Page) =>
  (await state(page)).selection_box as { x: number; y: number; width: number; height: number } | null;

/** Selects the slide at `index`. */
const goTo = (page: Page, index: number) =>
  deck(page, { SelectSlide: { index, extend: false, toggle: false } });

/** Gives the stage the keys. */
const focusStage = (page: Page) =>
  page.evaluate(() => (window as unknown as DeckWindow).__kilnDeck.stage?.focus());

/** The fixtures of the loop: the tracked feature decks, and the external corpus when fetched. */
const LOOP: readonly string[] = [
  ...[
    'text-formatting',
    'placeholders',
    'tables',
    'charts',
    'pictures',
    'shape-styles',
    'preset-shapes',
    'backgrounds',
    'theme-colors',
    'transitions',
    'animations',
    'sections',
    'notes',
    'many-slides',
    'groups',
    'connectors',
    'hyperlinks',
    'aspect-4x3',
  ].map((n) => `fixtures/pptx/features/${n}.pptx`),
  ...(existsSync(repoFile('fixtures/external/pptx'))
    ? readdirSync(repoFile('fixtures/external/pptx'))
        .filter((f) => f.endsWith('.pptx'))
        .sort()
        .map((f) => `fixtures/external/pptx/${f}`)
    : []),
];

/**
 * Shapes whose text runs off the slide in the file itself, by deck and slide: LibreOffice's PDF
 * leaves out what is off its page, so their lines are listed in the report, not compared.
 */
const OFF_THE_PAGE: Record<string, Record<number, { shape: string; why: string }>> = {
  'poi-with_japanese.pptx': {
    1: {
      shape: 'Rectangle 3',
      why: 'a box of no height half an inch from the top with its text centred on it: most of the text is above the slide',
    },
  },
  // A slide under two inches wide: the loop's nudge takes the box past its right edge, and the
  // oval's 18 pt text does not fit the page.
  'poi-smartart-simple.pptx': {
    1: { shape: 'TextBox 4', why: 'the nudge moves it past the edge of a slide two inches wide' },
    2: { shape: 'Oval 85', why: 'the new oval’s 18 pt text on a slide two inches wide' },
  },
};

/** The text typed at the end of a title. */
const TYPED = ' · Kiln';
/** The new slide's title and its oval's text. */
const GATE_TITLE = 'Gate slide';
const SHAPE_TEXT = 'Kiln oval';

test.describe('Kiln Deck gate: the Office file loop', () => {
  test.describe.configure({ timeout: 240_000 });

  for (const file of LOOP) {
    const name = file.split('/').pop() as string;
    test(`office file loop: ${name} is edited, gets a slide with a shape and a transition, saves, reopens, and LibreOffice reads it`, async ({
      page,
    }) => {
      needLibreOffice();
      await openFile(page, name, readFileSync(repoFile(file)));
      const before = await state(page);
      const outline0 = await query<OutlineEntry[]>(page, 'OutlineView');
      const text0 = await query<SlideText[]>(page, 'SlideText');
      // What the deck must say after the edits: the original's text, with the typed title.
      const expected = text0.map(comparable);

      // Text: the first slide with a title gets text typed at the end of it (F2, then keys).
      const titled = outline0.findIndex((e) => e.has_title && e.title.trim() !== '');
      let title = '';
      if (titled >= 0) {
        await goTo(page, titled);
        const shape = (await state(page)).shapes.find((s) => isTitle(s));
        if (shape) {
          await deck(page, { SelectShapes: { ids: [shape.id] } });
          await focusStage(page);
          await page.keyboard.press('F2');
          await expect.poll(async () => (await state(page)).editing).toBe(true);
          await page.keyboard.type(TYPED);
          await page.keyboard.press('Escape');
          await page.keyboard.press('Escape');
          title = `${outline0[titled]?.title.trim()}${TYPED}`;
          await expect
            .poll(async () => (await query<OutlineEntry[]>(page, 'OutlineView'))[titled]?.title.trim())
            .toBe(title);
          const typed = expected[titled]?.texts.find((t) => t.name === shape.name);
          if (typed) typed.lines = [...typed.lines.slice(0, -1), `${typed.lines.at(-1) ?? ''}${TYPED}`];
        }
      }

      // Shapes: the first shape of the deck that is not a placeholder moves right with the keys.
      let nudged: { slide: number; name: string; x: number } | null = null;
      for (let i = 0; i < before.slides.length && !nudged; i++) {
        await goTo(page, i);
        const shape = (await state(page)).shapes.find((s) =>
          ['shape', 'text box', 'picture'].includes(s.kind),
        );
        if (!shape) continue;
        await deck(page, { SelectShapes: { ids: [shape.id] } });
        const x0 = (await boxOf(page))?.x ?? 0;
        await focusStage(page);
        for (let k = 0; k < 3; k++) await page.keyboard.press('ArrowRight');
        await expect.poll(async () => (await boxOf(page))?.x ?? 0).toBeGreaterThan(x0);
        nudged = { slide: i, name: shape.name, x: (await boxOf(page))?.x ?? 0 };
        await page.keyboard.press('Escape');
      }

      // A new slide after the last one, on a layout picked from Home ▸ New Slide ▾.
      await goTo(page, before.slides.length - 1);
      const names = (await state(page)).layouts as { id: number; name: string }[];
      const layout =
        ['Title Only', 'Title and Content'].find((n) => names.some((l) => l.name === n)) ??
        names.find((l) => l.name !== 'Blank')?.name ??
        names[0]?.name ??
        '';
      expect(layout, `${name} has layouts`).not.toBe('');
      await ribbonTab(page, 'Home');
      await page.getByRole('button', { name: 'More options for New Slide' }).click();
      await page.getByRole('menuitem', { name: layout, exact: true }).first().click();
      await expect.poll(async () => (await state(page)).slides.length).toBe(before.slides.length + 1);
      const gate = before.slides.length;
      expect((await state(page)).current).toBe(gate);
      expect((await state(page)).slides[gate]?.layout).toBe(layout);
      const titleShape = (await state(page)).shapes.find((s) => isTitle(s));
      const gateTitle = !!titleShape && (await query<OutlineEntry[]>(page, 'OutlineView'))[gate]?.has_title;
      if (titleShape && gateTitle) {
        await deck(page, { SelectShapes: { ids: [titleShape.id] } });
        await focusStage(page);
        await page.keyboard.type(GATE_TITLE);
        await page.keyboard.press('Escape');
        await page.keyboard.press('Escape');
        await expect
          .poll(async () => (await query<OutlineEntry[]>(page, 'OutlineView'))[gate]?.title)
          .toBe(GATE_TITLE);
      }
      // Insert ▸ Shapes ▸ Oval, drawn on the slide, typed into and filled.
      const st = await state(page);
      await ribbonTab(page, 'Insert');
      await page.getByRole('button', { name: 'More options for Shapes' }).first().click();
      await page.getByRole('menuitem', { name: 'Oval', exact: true }).click();
      const [w, h] = [st.slide_width as number, st.slide_height as number];
      await dragOnSlide(page, [w * 0.6, h * 0.62], [w * 0.85, h * 0.9]);
      await expect.poll(async () => (await state(page)).shapes.length).toBe(st.shapes.length + 1);
      const oval = (await state(page)).shapes.find((s) => !st.shapes.some((o) => o.id === s.id))?.name ?? '';
      expect(oval, 'the oval is on the slide').not.toBe('');
      await focusStage(page);
      await page.keyboard.type(SHAPE_TEXT);
      await page.keyboard.press('Escape');
      await deck(page, {
        SetFill: { fill: { Solid: { color: 'E05A27', brightness: null, transparency: null } } },
      });
      await page.keyboard.press('Escape');
      // Transitions ▸ Push on the new slide.
      await ribbonTab(page, 'Transitions');
      await page.getByRole('option', { name: 'Push' }).first().click();
      await expect.poll(() => transitionOf(page)).toBe('push');
      // The new slide's text: its title (when its layout has one) and the oval's.
      const gateText = (await query<SlideText[]>(page, 'SlideText'))[before.slides.length];
      expect(gateText?.texts.find((t) => t.name === oval)?.lines).toEqual([SHAPE_TEXT]);
      expected.push(comparable(gateText));

      // Save (Ctrl+S; the first save of an opened file asks for the format: the same one).
      await ribbonTab(page, 'Home');
      await focusStage(page);
      await page.keyboard.press('Control+s');
      const dialog = page.getByRole('dialog', { name: /File menu/ });
      if (await dialog.isVisible().catch(() => false))
        await dialog.getByRole('button', { name: /PowerPoint Presentation/ }).click();
      const saved = await savedBytes(page, name);
      const dir = out(`file-loop/${name.replace(/\.pptx$/, '')}`);
      mkdirSync(dir, { recursive: true });
      writeFileSync(`${dir}/${name}`, saved);
      const gateXml = slideWith(saved, SHAPE_TEXT);
      expect(gateXml, 'the new slide is saved with its oval').toContain('prst="ellipse"');
      expect(gateXml).toContain('E05A27');
      expect(gateXml).toMatch(/<p:transition\b[\s\S]*<p:push\b/);

      // Kiln opens it again with everything kept.
      await openFile(page, name, saved);
      const back = await state(page);
      expect(back.slides.length).toBe(before.slides.length + 1);
      const outline = await query<OutlineEntry[]>(page, 'OutlineView');
      if (title) expect(outline[titled]?.title.trim()).toBe(title);
      expect(back.slides[gate]?.layout).toBe(layout);
      if (gateTitle) expect(outline[gate]?.title).toBe(GATE_TITLE);
      await goTo(page, gate);
      expect(await transitionOf(page)).toBe('push');
      // The oval keeps its text and fill.
      const ovalAgain = (await state(page)).shapes.find((s) => s.name === oval);
      expect(ovalAgain, `${oval} is on the new slide`).toBeDefined();
      await deck(page, { SelectShapes: { ids: [ovalAgain?.id ?? 0] } });
      const ovalFormat = (await state(page)).format as { fill: string | null; fill_kind: string };
      expect([ovalFormat.fill_kind, ovalFormat.fill?.toUpperCase()]).toEqual(['solid', 'E05A27']);
      await deck(page, 'Escape');
      // Every slide says what it said, with the edits: every shape's text (text boxes, tables,
      // groups and placeholders), its notes and how many shapes it has.
      const text1 = await query<SlideText[]>(page, 'SlideText');
      const reopened = text1.map(comparable);
      const changed = expected
        .map((e, i) => (JSON.stringify(e) === JSON.stringify(reopened[i]) ? null : i + 1))
        .filter((i): i is number => i !== null);
      for (const i of changed) expect(reopened[i - 1], `slide ${i} after reopening`).toEqual(expected[i - 1]);
      await page.getByTestId('deck-editor').screenshot({ path: `${dir}/kiln-reopened.png` });
      if (nudged) {
        await goTo(page, nudged.slide);
        // Node ids are the session's: the shape is found again by its name.
        const again = (await state(page)).shapes.find((s) => s.name === nudged?.name);
        expect(again, `${nudged.name} is on slide ${nudged.slide + 1}`).toBeDefined();
        await deck(page, { SelectShapes: { ids: [again?.id ?? 0] } });
        expect(Math.abs(((await boxOf(page))?.x ?? 0) - nudged.x)).toBeLessThan(0.01);
      }

      // LibreOffice opens the saved file: one page per slide shown, with the text the deck had
      // and the edits (every line of every shape that has text).
      const pdf = libreOfficePdf(`${dir}/${name}`, `${dir}/libreoffice`);
      const pages = pdfPages(pdf);
      const shown = expected.flatMap((e, i) => (e && !e.hidden ? [{ e, slide: i + 1 }] : []));
      // What LibreOffice's PDF gives of the original: a line it does not give there (bidirectional
      // text in pieces, a line split across runs) says nothing about the save.
      writeFileSync(`${dir}/original-${name}`, readFileSync(repoFile(file)));
      const original = pdfPages(libreOfficePdf(`${dir}/original-${name}`, `${dir}/libreoffice-original`));
      const report: {
        file: string;
        slides: { opened: number; saved: number; shown: number };
        libreoffice: { pages: number };
        compared: number;
        differences: string[];
        off_the_page: string[];
        unread_in_the_original: string[];
        edits: Record<string, unknown>;
      } = {
        file: name,
        slides: { opened: before.slides.length, saved: back.slides.length, shown: shown.length },
        libreoffice: { pages: pages.length },
        compared: 0,
        differences: [],
        off_the_page: [],
        unread_in_the_original: [],
        edits: { title: title || null, layout, gate_title: gateTitle ? GATE_TITLE : null, nudged: !!nudged },
      };
      shown.forEach(({ e, slide }, i) => {
        for (const t of e.texts) {
          for (const text of t.lines) {
            if (!squeeze(text)) continue;
            report.compared += 1;
            if (shows(pages[i] ?? { text: '', raw: '' }, text)) continue;
            const off = OFF_THE_PAGE[name]?.[slide];
            const where = `slide ${slide}, ${t.name}: LibreOffice does not show "${text}"`;
            if (off?.shape === t.name) report.off_the_page.push(`${where} (${off.why})`);
            else if (slide <= before.slides.length && !shows(original[i] ?? { text: '', raw: '' }, text))
              report.unread_in_the_original.push(where);
            else report.differences.push(where);
          }
        }
      });
      if (
        !shows(pages[shown.length - 1] ?? { text: '', raw: '' }, SHAPE_TEXT) &&
        !OFF_THE_PAGE[name]?.[shown.length]
      )
        report.differences.push('the new oval’s text is not on the last page');
      execFileSync(
        'pdftoppm',
        [
          '-r',
          '48',
          '-png',
          '-f',
          String(pages.length),
          '-l',
          String(pages.length),
          pdf,
          `${dir}/libreoffice/last`,
        ],
        {
          timeout: 60_000,
        },
      );
      writeFileSync(`${dir}/report.json`, `${JSON.stringify(report, null, 2)}\n`);
      console.log(
        `file loop ${name}: ${back.slides.length} slides, LibreOffice ${pages.length} pages; ${report.compared} texts compared, ${report.differences.length} differences`,
      );
      expect(pages.length, 'LibreOffice shows every slide that is not hidden').toBe(shown.length);
      expect(report.differences, 'texts LibreOffice shows differently').toEqual([]);
    });
  }
});

// ── the visual review ───────────────────────────────────────────────────────────────────────

/** A screenshot without the pointer's tooltip or a focused button's. */
async function snap(page: Page, name: string): Promise<void> {
  await page.mouse.move(2, 600);
  await page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (a?.closest('.kiln-ribbon, .kiln-statusbar')) a.blur();
  });
  await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished)));
  await page.waitForTimeout(250);
  mkdirSync(out(''), { recursive: true });
  await page.screenshot({ path: out(name) });
}

/** Side of the square regions a screenshot is compared in, px. */
const REGION = 32;

/**
 * How a screenshot differs from another: pixelmatch (threshold 0.1, anti-aliased pixels not
 * counted) over the whole image, and the worst of its 32 px regions.
 */
function screenshotDiff(a: Buffer, b: Buffer): { total: number; worst: number; at: string } {
  const ia = decodePng(a);
  const ib = decodePng(b);
  if (ia.width !== ib.width || ia.height !== ib.height)
    return { total: 1, worst: 1, at: `size ${ia.width}×${ia.height} against ${ib.width}×${ib.height}` };
  const { width: w, height: h } = ia;
  const diff = new Uint8Array(w * h * 4);
  pixelmatch(ia.data, ib.data, diff, w, h, { threshold: 0.1, diffMask: true, includeAA: false });
  let differing = 0;
  let worst = 0;
  let at = '';
  for (let y0 = 0; y0 < h; y0 += REGION) {
    for (let x0 = 0; x0 < w; x0 += REGION) {
      let n = 0;
      const y1 = Math.min(h, y0 + REGION);
      const x1 = Math.min(w, x0 + REGION);
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const i = (y * w + x) * 4;
          // Differing pixels are drawn red; anti-aliasing yellow (not counted).
          if (diff[i + 3] !== 0 && diff[i] === 255 && diff[i + 1] === 0) n += 1;
        }
      }
      differing += n;
      const frac = n / ((y1 - y0) * (x1 - x0));
      if (frac > worst) {
        worst = frac;
        at = `${x0},${y0}`;
      }
    }
  }
  return { total: differing / (w * h), worst, at };
}

/**
 * At most 0.1 % of the pixels may differ, and at most 4 % of any 32 px region: what font
 * hinting and antialiasing change between runs, spread thin. A different ribbon tab, a moved
 * control or a changed colour fills whole regions and fails.
 */
const MAX_TOTAL = 0.001;
const MAX_REGION = 0.04;

function matchesBaseline(shot: Buffer, base: Buffer): { ok: boolean; why: string } {
  const d = screenshotDiff(shot, base);
  const why = `${(d.total * 100).toFixed(3)} % of the pixels differ, ${(d.worst * 100).toFixed(1)} % of the region at ${d.at}`;
  return { ok: d.total <= MAX_TOTAL && d.worst <= MAX_REGION, why };
}

/**
 * The screenshot against its reviewed baseline ({@link matchesBaseline}).
 * `KILN_UPDATE_BASELINES=1` writes the baseline from the screenshot instead.
 */
function expectBaseline(name: string): void {
  const shot = readFileSync(out(name));
  const path = baseline(name);
  if (process.env.KILN_UPDATE_BASELINES === '1' || !existsSync(path)) {
    mkdirSync(baseline(''), { recursive: true });
    writeFileSync(path, shot);
    if (process.env.KILN_UPDATE_BASELINES !== '1')
      throw new Error(`${name}: no baseline yet (written; review it)`);
    return;
  }
  const { ok, why } = matchesBaseline(shot, readFileSync(path));
  console.log(`baseline ${name}: ${why}`);
  expect(ok, `${name} against its baseline: ${why}`).toBe(true);
}

/** The layout of the current master named `name`. */
async function layoutId(page: Page, name: string): Promise<number> {
  const l = ((await state(page)).layouts as { id: number; name: string }[]).find((x) => x.name === name);
  if (!l) throw new Error(`no layout ${name}`);
  return l.id;
}

/** Types into the current slide's placeholder whose name matches, with the keyboard. */
async function typeInto(page: Page, name: RegExp, text: string): Promise<void> {
  const shape = (await state(page)).shapes.find((s) => name.test(s.name));
  if (!shape) throw new Error(`no shape matching ${name}`);
  await deck(page, { SelectShapes: { ids: [shape.id] } });
  await focusStage(page);
  await page.keyboard.type(text);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
}

/**
 * A new presentation of the slides the themes are reviewed on: a title slide, bullets, a
 * picture, a table and a chart, with speaker notes; the first slide current.
 */
async function reviewDeck(page: Page): Promise<void> {
  await newDeck(page);
  await typeInto(page, /^Title/, 'Firing schedule');
  await typeInto(page, /^Subtitle/, 'Studio report, autumn term');
  await deck(page, { SetNotes: { text: 'Open with the kiln numbers.' } });
  await deck(page, { NewSlide: { layout: await layoutId(page, 'Title and Content') } });
  await typeInto(page, /^Title/, 'What we fired');
  const body = (await state(page)).shapes.find((s) => /^(Content|Text|Body)/.test(s.name));
  if (!body) throw new Error('no body placeholder');
  await deck(page, { SelectShapes: { ids: [body.id] } });
  await deck(page, 'EditText');
  await deck(page, { InsertText: { text: 'Bisque at 1000 °C' } });
  await deck(page, 'NewParagraph');
  await deck(page, { InsertText: { text: 'Glaze at 1240 °C' } });
  await deck(page, 'NewParagraph');
  await deck(page, 'Demote');
  await deck(page, { InsertText: { text: 'Cone 6, oxidation' } });
  await deck(page, 'NewParagraph');
  await deck(page, 'Promote');
  await deck(page, { InsertText: { text: 'Raku on open day' } });
  await deck(page, 'Escape');
  await deck(page, 'Escape');
  await deck(page, { NewSlide: { layout: await layoutId(page, 'Title Only') } });
  await typeInto(page, /^Title/, 'The new shelves');
  await deck(page, {
    InsertPicture: {
      name: 'shelves.png',
      bytes: await twoTonePng(page, 800, 450),
      rect: null,
    },
  });
  await deck(page, { NewSlide: { layout: await layoutId(page, 'Title Only') } });
  await typeInto(page, /^Title/, 'Costs');
  // The table starts with the first cell's text being edited; Tab moves to the next cell.
  await deck(page, { InsertTable: { rows: 4, cols: 3 } });
  const cells = [
    'Item',
    'Term',
    'Cost',
    'Clay',
    'Autumn',
    '£420',
    'Glaze',
    'Autumn',
    '£310',
    'Gas',
    'Autumn',
    '£1,180',
  ];
  for (const [i, text] of cells.entries()) {
    if (i > 0) await deck(page, { Tab: { shift: false } });
    await deck(page, { InsertText: { text } });
  }
  await deck(page, 'Escape');
  await deck(page, 'Escape');
  await deck(page, { NewSlide: { layout: await layoutId(page, 'Title Only') } });
  await typeInto(page, /^Title/, 'Firings per month');
  await suite(page, { ChartInsert: { kind: 'ColumnClustered' } });
  await deck(page, 'Escape');
  await goTo(page, 0);
  await deck(page, 'Escape');
}

/** The deck's themes, as the Design gallery names them. */
const THEMES = [
  'Kiln',
  'Tenmoku',
  'Cobalt',
  'Jade',
  'Saffron',
  'Celadon',
  'Shino',
  'Raku',
  'Porcelain',
  'Slate',
  'Terracotta',
  'Ash',
] as const;

/**
 * How a part of the current slide (slide px) is drawn on the stage: the median luminance of
 * its pixels, and the share of them that are light and neutral (grey or white text, not a
 * coloured fill; a chart's labels are a lighter grey than its title).
 */
async function lightOnDark(
  page: Page,
  r: { x: number; y: number; width: number; height: number },
): Promise<{ background: number; light: number }> {
  await page.waitForTimeout(500);
  const a = await onSlide(page, r.x, r.y);
  const b = await onSlide(page, r.x + r.width, r.y + r.height);
  const png = decodePng(
    await page.screenshot({ clip: { x: a.x + 2, y: a.y + 2, width: b.x - a.x - 4, height: b.y - a.y - 4 } }),
  );
  const lum: number[] = [];
  let light = 0;
  for (let i = 0; i < png.data.length; i += 4) {
    const [red, green, blue] = [png.data[i] ?? 0, png.data[i + 1] ?? 0, png.data[i + 2] ?? 0];
    const l = (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
    lum.push(l);
    if (l > 0.45 && Math.max(red, green, blue) - Math.min(red, green, blue) < 48) light += 1;
  }
  lum.sort((x, y) => x - y);
  return { background: lum[Math.floor(lum.length / 2)] ?? 1, light: light / Math.max(lum.length, 1) };
}

/** Shows a view of the status bar's view buttons. */
async function view(page: Page, name: 'Normal' | 'Slide Sorter') {
  await page.locator('.kiln-statusbar').getByRole('radio', { name }).click();
  await expect(page.getByTestId('deck-editor')).toHaveAttribute(
    'data-view',
    name === 'Normal' ? 'SlideNormal' : 'SlideSorter',
  );
}

/** Waits until the slide sorter has a thumbnail for every slide. */
async function sorterReady(page: Page, count: number) {
  const sorter = page.getByTestId('deck-sorter');
  await expect(sorter.locator('canvas')).toHaveCount(count, { timeout: 30_000 });
  await expect(sorter.locator('canvas[data-version="-1"]')).toHaveCount(0, { timeout: 60_000 });
  await page.waitForTimeout(400);
}

test.describe('Kiln Deck gate: screenshots', () => {
  test.describe.configure({ timeout: 240_000 });

  test('screenshots: the Deck chrome in light and dark, desktop and phone, the sorter, against the baselines', async ({
    page,
    browser,
    baseURL,
  }) => {
    await reviewDeck(page);
    await goTo(page, 1);
    await deck(page, 'Escape');
    await page.waitForTimeout(600);
    await snap(page, 'chrome-light-desktop.png');
    expectBaseline('chrome-light-desktop.png');
    for (const t of ['Insert', 'Design', 'Transitions', 'Animations', 'Slide Show', 'Review', 'View']) {
      await ribbonTab(page, t);
      await page.waitForTimeout(300);
      await snap(page, `${t.toLowerCase().replace(' ', '-')}-tab-light-desktop.png`);
    }
    expectBaseline('design-tab-light-desktop.png');
    // The check tells one ribbon tab from another: the View tab is not the Design baseline.
    const other = matchesBaseline(
      readFileSync(out('view-tab-light-desktop.png')),
      readFileSync(baseline('design-tab-light-desktop.png')),
    );
    expect(other.ok, `the View tab passes as the Design tab: ${other.why}`).toBe(false);
    await ribbonTab(page, 'Home');
    await view(page, 'Slide Sorter');
    await sorterReady(page, 5);
    await snap(page, 'sorter-light-desktop.png');
    expectBaseline('sorter-light-desktop.png');
    await view(page, 'Normal');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(800);
    await snap(page, 'chrome-dark-desktop.png');
    expectBaseline('chrome-dark-desktop.png');
    const { expectNoSeriousA11yViolations } = await import('../../packages/testing/src/playwright/a11y.ts');
    await expectNoSeriousA11yViolations(page);
    await page.emulateMedia({ colorScheme: 'light' });
    const bytes = Buffer.from(await query<number[]>(page, 'Snapshot'));
    for (const scheme of ['light', 'dark'] as const) {
      const ctx = await browser.newContext({
        baseURL,
        viewport: { width: 412, height: 915 },
        colorScheme: scheme,
      });
      const phone = await ctx.newPage();
      try {
        await newDeck(phone);
        await openBytes(phone, bytes, 'Firing schedule.pptx');
        await phone.waitForTimeout(800);
        await snap(phone, `chrome-${scheme}-phone.png`);
        expectBaseline(`chrome-${scheme}-phone.png`);
        expect(await phone.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
      } finally {
        await ctx.close();
      }
    }
  });

  test('screenshots: the twelve themes on title, bullets, picture, table and chart slides', async ({
    page,
  }) => {
    await reviewDeck(page);
    await view(page, 'Slide Sorter');
    for (const name of THEMES) {
      await deck(page, { ApplyTheme: { name } });
      await expect.poll(async () => (await state(page)).theme).toBe(name);
      await sorterReady(page, 5);
      await page.waitForTimeout(600);
      await page.getByTestId('deck-sorter').screenshot({ path: out(`theme-${name.toLowerCase()}.png`) });
      // The deck as saved, for full-size renders of its slides (`kiln-cli render`).
      mkdirSync(out('themes'), { recursive: true });
      writeFileSync(
        out(`themes/${name.toLowerCase()}.pptx`),
        Buffer.from(await query<number[]>(page, 'Snapshot')),
      );
    }
    expectBaseline('theme-raku.png');
    // Raku's slides are dark and their text light: the title slide's, and the chart's axis
    // labels and legend, which follow the slide's colours.
    await deck(page, { ApplyTheme: { name: 'Raku' } });
    await expect.poll(async () => (await state(page)).theme).toBe('Raku');
    await view(page, 'Normal');
    const st = await state(page);
    const [w, h] = [st.slide_width as number, st.slide_height as number];
    await goTo(page, 0);
    const title = await lightOnDark(page, { x: 0, y: 0, width: w, height: h });
    expect(title.background, 'the title slide is dark').toBeLessThan(0.3);
    expect(title.light, 'the title slide has light text').toBeGreaterThan(0.004);
    await goTo(page, 4);
    const chart = (await state(page)).shapes.find((s) => s.kind === 'chart');
    await deck(page, { SelectShapes: { ids: [chart?.id ?? 0] } });
    const box = await boxOf(page);
    if (!box) throw new Error('no chart on the chart slide');
    await deck(page, 'Escape');
    // The lowest fifth of the chart: the category labels and the legend, below the columns.
    const [bx, by, bw, bh] = [box.x / 0.75, box.y / 0.75, box.width / 0.75, box.height / 0.75];
    const labels = await lightOnDark(page, { x: bx, y: by + bh * 0.8, width: bw, height: bh * 0.2 });
    expect(labels.background, 'the chart sits on the dark slide').toBeLessThan(0.3);
    expect(labels.light, 'the chart’s labels are light').toBeGreaterThan(0.004);
  });

  test('screenshots: the show and Presenter View', async ({ page, context }) => {
    await reviewDeck(page);
    await deck(page, { SetTransition: { effect: 'fade' } });
    await ribbonTab(page, 'Slide Show');
    await page.getByRole('button', { name: 'From Beginning' }).click();
    const show = page.getByTestId('deck-show');
    await expect(show).toBeVisible({ timeout: 30_000 });
    await page.waitForTimeout(1500);
    await snap(page, 'show-light-desktop.png');
    await page.keyboard.press('ArrowRight');
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.waitForTimeout(1500);
    await snap(page, 'show-bullets.png');
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('deck-editor')).toBeVisible({ timeout: 30_000 });
    await waitForStage(page);
    await ribbonTab(page, 'Slide Show');
    await page.getByRole('button', { name: 'Use Presenter View' }).click();
    const popup = context.waitForEvent('page');
    await page.getByRole('button', { name: 'From Beginning' }).click();
    const audience = await popup;
    await expect(page.getByTestId('deck-presenter')).toBeVisible({ timeout: 30_000 });
    await expect(audience.getByTestId('deck-show')).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId('deck-presenter-notes')).toHaveText('Open with the kiln numbers.');
    await page.waitForTimeout(1500);
    await snap(page, 'presenter-view.png');
    await page.getByTestId('deck-presenter-end').click();
    await expect(page).toHaveURL(/#\/deck\?doc=\d+/);
  });

  test('screenshots: the dialogs and panes of the Deck P0 rows, light and dark', async ({ page }) => {
    await reviewDeck(page);
    const open = async (id: string, arg?: number) => {
      await page.evaluate(
        ([d, a]) =>
          (
            (window as unknown as DeckWindow).__kilnDeck as unknown as {
              openDialog(id: string, arg?: number): void;
            }
          ).openDialog(d as string, a as number | undefined),
        [id, arg] as const,
      );
      await expect(page.getByRole('dialog').last()).toBeVisible();
    };
    const close = async () => {
      await page.keyboard.press('Escape');
      await expect(page.getByRole('dialog')).toHaveCount(0);
    };
    for (const scheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await goTo(page, 4);
      await open('insertChart');
      await snap(page, `dialog-insert-chart-${scheme}.png`);
      await close();
      const chart = (await state(page)).shapes.find((s) => s.kind === 'chart');
      if (!chart) throw new Error('no chart');
      await deck(page, { SelectShapes: { ids: [chart.id] } });
      await open('chartData');
      await snap(page, `dialog-chart-data-${scheme}.png`);
      await close();
      await goTo(page, 2);
      await open('icons');
      await snap(page, `dialog-icons-${scheme}.png`);
      await close();
      await open('codeBlock');
      await snap(page, `dialog-code-block-${scheme}.png`);
      await close();
      await open('symbol');
      await snap(page, `dialog-symbol-${scheme}.png`);
      await close();
      const picture = (await state(page)).shapes.find((s) => s.kind === 'picture');
      if (!picture) throw new Error('no picture');
      await deck(page, { SelectShapes: { ids: [picture.id] } });
      await open('formatShape');
      await snap(page, `dialog-format-shape-${scheme}.png`);
      await close();
      await deck(page, 'Escape');
    }
    await page.emulateMedia({ colorScheme: 'light' });
    // Panes: comments, spelling; Slide Master view; the notes page; rulers and guides.
    await goTo(page, 1);
    await page.getByTestId('deck-comments-toggle').click();
    await expect(page.getByTestId('deck-comments-pane')).toBeVisible();
    await snap(page, 'pane-comments.png');
    await page.getByTestId('deck-comments-toggle').click();
    await ribbonTab(page, 'Review');
    await page.getByRole('button', { name: 'Spelling', exact: true }).click();
    await page.waitForTimeout(500);
    await snap(page, 'pane-spelling.png');
    await page.getByRole('button', { name: 'Close Spelling' }).click();
    // View ▸ Slide Master brings its tab to the front.
    await ribbonTab(page, 'View');
    await page.getByRole('button', { name: 'Slide Master', exact: true }).click();
    await expect(page.getByTestId('deck-master-rail')).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Slide Master', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await page.waitForTimeout(500);
    await snap(page, 'slide-master.png');
    await page.getByRole('button', { name: 'Close Master View' }).click();
    await expect(page.getByTestId('deck-master-rail')).toBeHidden();
    await ribbonTab(page, 'View');
    await page.getByRole('button', { name: 'Notes Page', exact: true }).click();
    await page.waitForTimeout(800);
    await snap(page, 'notes-page.png');
    await page.getByRole('button', { name: 'Normal', exact: true }).first().click();
    for (const box of ['Ruler', 'Guides']) {
      await page.locator('.kiln-ribbon').getByText(box, { exact: true }).click();
      await expect(page.getByRole('checkbox', { name: box })).toBeChecked();
    }
    await page.waitForTimeout(500);
    await snap(page, 'rulers-guides.png');
  });
});

// ── budgets ─────────────────────────────────────────────────────────────────────────────────

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.map': 'application/json',
};

/** Serves a built site (the app routes by hash: no fallback needed). */
function serveStatic(root: string): Promise<Server> {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
    const file = normalize(join(root, path === '/' ? 'index.html' : path));
    try {
      if (!file.startsWith(root) || !statSync(file).isFile()) throw new Error('not found');
      res.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream' });
      res.end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

/** A limit of `bench/budgets.toml` (the row with this id and part) and the CI tolerance. */
function budget(id: string, part: string): { limit: number; ci: number } {
  const toml = readFileSync(repoFile('bench/budgets.toml'), 'utf8');
  const row = toml
    .split('[[budget]]')
    .find((b) => b.includes(`id = "${id}"`) && b.includes(`part = "${part}"`));
  const limit = Number(row?.match(/^limit = ([\d.]+)$/m)?.[1]);
  const ci = Number(toml.match(/^ci_tolerance = ([\d.]+)$/m)?.[1]);
  if (!Number.isFinite(limit) || !Number.isFinite(ci)) throw new Error(`no budget ${id} (${part})`);
  return { limit, ci };
}

const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.ceil(s.length * p) - 1)] ?? Number.POSITIVE_INFINITY;
};

/**
 * B6 for Deck on a production build (`VITE_KILN_RELEASE=1`, the release web core): the frames of
 * scrolling the slide sorter of the 200-slide presentation, and the time it takes to open.
 */
test.describe('deck budgets', { tag: '@budget' }, () => {
  test.describe.configure({ mode: 'serial' });
  let outDir = '';
  let server: Server | undefined;
  let base = '';

  test.beforeAll(async () => {
    test.setTimeout(300_000);
    const deck200 = repoFile('fixtures/large/deck-200.pptx');
    if (!existsSync(deck200)) {
      execFileSync(
        'python3',
        ['-I', repoFile('tools/make-fixtures.py'), '--synthetic-large', '--only', 'pptx'],
        {
          stdio: 'pipe',
          timeout: 240_000,
        },
      );
    }
    outDir = mkdtempSync(join(tmpdir(), 'kiln-deck-b6-'));
    execFileSync(
      'pnpm',
      ['--filter', '@kiln/web', 'exec', 'vite', 'build', '--outDir', outDir, '--emptyOutDir'],
      {
        cwd: repoFile(''),
        env: { ...process.env, VITE_KILN_RELEASE: '1' },
        stdio: 'pipe',
      },
    );
    server = await serveStatic(outDir);
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  test.afterAll(async () => {
    await new Promise<void>((resolve) => (server ? server.close(() => resolve()) : resolve()));
    if (outDir) rmSync(outDir, { recursive: true, force: true });
  });

  test('deck B6: the slide sorter of the 200-slide presentation scrolls at 60 fps', async ({ page }) => {
    test.setTimeout(300_000);
    await page.goto(`${base}/#/home?blink=0`);
    await expect(
      page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
    ).toBeVisible({ timeout: 60_000 });
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      delete (window as any).showOpenFilePicker;
    });
    const t0 = Date.now();
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles(repoFile('fixtures/large/deck-200.pptx'));
    await expect(page.getByTestId('deck-stage')).toBeVisible({ timeout: 120_000 });
    await expect(page.locator('.kiln-statusbar').getByText('Slide 1 of 200')).toBeVisible({
      timeout: 120_000,
    });
    const openMs = Date.now() - t0;
    console.log(`B10 web: deck-200.pptx open to the editor ${openMs} ms`);
    test.info().annotations.push({
      type: 'B10',
      description: `deck-200.pptx open to the editor in the web core: ${openMs} ms`,
    });
    await view(page, 'Slide Sorter');
    await sorterReady(page, 200);
    // Scroll at 2,000 px/s for 4 s, one step per animation frame, down and back up.
    const result = await page.evaluate(async () => {
      const sorter = document.querySelector('[data-testid="deck-sorter"]') as HTMLElement;
      const intervals: number[] = [];
      let prev = performance.now();
      const start = prev;
      let dir = 1;
      await new Promise<void>((resolve) => {
        const step = (t: number) => {
          intervals.push(t - prev);
          sorter.scrollTop += (dir * 2000 * Math.min(100, t - prev)) / 1000;
          if (sorter.scrollTop + sorter.clientHeight >= sorter.scrollHeight - 1) dir = -1;
          prev = t;
          if (t - start < 4000) requestAnimationFrame(step);
          else resolve();
        };
        requestAnimationFrame(step);
      });
      return { intervals: intervals.slice(1), height: sorter.scrollHeight, top: sorter.scrollTop };
    });
    const p50 = pct(result.intervals, 0.5);
    const p95 = pct(result.intervals, 0.95);
    const worst = Math.max(...result.intervals);
    const summary = `frame interval p50=${p50.toFixed(1)} ms p95=${p95.toFixed(1)} ms max=${worst.toFixed(1)} ms over ${result.intervals.length} frames; sorter ${result.height} px tall`;
    console.log(`B6 deck sorter scroll on deck-200 (production build): ${summary}`);
    test.info().annotations.push({ type: 'B6', description: summary });
    expect(result.height).toBeGreaterThan(5000);
    const interval = budget('B6', 'Deck sorter scroll frame interval');
    const tolerance = process.env.CI ? interval.ci : 1;
    expect(p95, `B6 frame interval p95 ${p95.toFixed(1)} ms`).toBeLessThanOrEqual(interval.limit * tolerance);
  });
});
