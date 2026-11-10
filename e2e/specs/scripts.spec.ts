/**
 * Chinese, Japanese, Korean, Arabic and emoji, typed into Kiln Grid, Kiln Write and Kiln Deck,
 * drawn on the canvas with real glyphs: the web core fetches the fonts that cover them (the
 * slices of Noto Sans CJK SC that have the text's characters, Noto Naskh Arabic, Noto Emoji) when
 * the text needs them. Each canvas is compared with its committed screenshot, so a glyph drawn as
 * a box (tofu) fails; the core also says which characters still have no font.
 */
import { expect, type Page, test } from '@playwright/test';
import { state as deckState, newDeck, onSlide } from './deck-helpers.ts';
import { inCoreWorker } from './pwa-helpers.ts';

const LINES = ['日本語 中文 한국어 かな', 'مرحبا بالعالم', 'Emoji 😀 🎉 ✓'];
/** A few pixels of antialiasing apart; a box drawn for a glyph is hundreds. */
const SHOT = { maxDiffPixels: 40 };

/** The part of `el` from (x, y), w × h (CSS pixels), for a comparison of the text alone. */
async function clipOf(page: Page, testId: string, x: number, y: number, w: number, h: number) {
  const box = await page.getByTestId(testId).boundingBox();
  if (!box) throw new Error(`no ${testId}`);
  return { x: box.x + x, y: box.y + y, width: w, height: h };
}

/** Waits until the fallback fonts have reached the core and the canvas has drawn again. */
async function fontsDrawn(page: Page): Promise<void> {
  await page.waitForTimeout(1500);
  await page.evaluate(() => document.fonts?.ready);
  await page.waitForTimeout(1000);
}

test.describe('scripts on the canvas', () => {
  // `KILN_BLOCK_FALLBACK_FONTS=1` withholds the fallback fonts, to see the comparison fail on tofu.
  test.beforeEach(async ({ page }) => {
    if (process.env.KILN_BLOCK_FALLBACK_FONTS === '1')
      await page.route(/Noto(SansCJK|Emoji|Naskh)[^?]*\.(ttf|otf)$/, (r) => r.abort());
  });

  test('Kiln Grid draws CJK, Arabic and emoji', async ({ page }) => {
    await page.goto('/#/grid');
    const bar = page.getByRole('textbox', { name: 'Formula Bar' });
    await expect(bar).toBeVisible({ timeout: 60_000 });
    const nameBox = page
      .getByRole('combobox', { name: 'Name Box' })
      .or(page.getByRole('textbox', { name: 'Name Box' }));
    // The fallback fonts first (a row fits its text with the fonts it has when it is entered).
    await nameBox.first().fill('H20');
    await nameBox.first().press('Enter');
    await page.keyboard.insertText(LINES.join(' '));
    await page.keyboard.press('Enter');
    await fontsDrawn(page);
    for (const [i, line] of LINES.entries()) {
      await nameBox.first().fill(`B${i + 2}`);
      await nameBox.first().press('Enter');
      await page.keyboard.insertText(line);
      await page.keyboard.press('Enter');
    }
    await expect(page.getByRole('gridcell', { name: /^B4 Emoji/ })).toBeAttached({ timeout: 15_000 });
    await fontsDrawn(page);
    // Cells B2:D4, where the three lines are.
    await expect(page).toHaveScreenshot('grid-scripts.png', {
      ...SHOT,
      clip: await clipOf(page, 'grid-editor', 112, 90, 260, 84),
    });
  });

  test('Kiln Write draws CJK, Arabic and emoji', async ({ page }) => {
    await page.goto('/#/write');
    const proxy = page.getByTestId('kiln-input-proxy');
    await expect(proxy).toBeAttached({ timeout: 60_000 });
    await proxy.focus();
    for (const line of LINES) {
      await page.keyboard.insertText(line);
      await page.keyboard.press('Enter');
    }
    await fontsDrawn(page);
    const canvas = await page.locator('.kiln-appframe__canvas').boundingBox();
    if (!canvas) throw new Error('no canvas');
    // The three lines at the top of the page.
    await expect(page).toHaveScreenshot('write-scripts.png', {
      ...SHOT,
      clip: { x: canvas.x + 400, y: canvas.y + 155, width: 260, height: 105 },
    });
  });

  test('Kiln Deck draws CJK, Arabic and emoji', async ({ page }) => {
    await newDeck(page);
    const at = await onSlide(page, 640, 250);
    await page.mouse.click(at.x, at.y);
    await expect.poll(async () => (await deckState(page)).editing).toBe(true);
    await page.keyboard.insertText(LINES.join(' '));
    await page.keyboard.press('Escape');
    await fontsDrawn(page);
    await expect(page.getByTestId('deck-stage')).toHaveScreenshot('deck-scripts.png', SHOT);
  });

  /** The Grid core Worker's font diagnostics (`kilnFonts` in worker.ts). */
  async function fontState(page: Page): Promise<{ slices: string[]; waiting: number; uncovered: string }> {
    const s = await inCoreWorker(page, 'kilnFonts', () =>
      (globalThis as unknown as { kilnFonts: () => unknown }).kilnFonts(),
    );
    if (!s) throw new Error('no core Worker with font diagnostics');
    return s as { slices: string[]; waiting: number; uncovered: string };
  }

  /** Types `text` into the next cell of column B, one cell per item. */
  async function enter(page: Page, items: string[], from: number): Promise<void> {
    const nameBox = page
      .getByRole('combobox', { name: 'Name Box' })
      .or(page.getByRole('textbox', { name: 'Name Box' }));
    for (const [i, text] of items.entries()) {
      const ref = `B${from + i}`;
      await nameBox.first().fill(ref);
      await nameBox.first().press('Enter');
      await expect(nameBox.first()).toHaveValue(ref);
      await page.keyboard.insertText(text);
      await page.keyboard.press('Enter');
      // Entered before the next: a long text keeps the core busy for a moment.
      await expect(page.getByRole('gridcell', { name: new RegExp(`^${ref} \\S`) })).toBeAttached({
        timeout: 30_000,
      });
      // And laid out: the core Worker answers, with no font slice still on its way.
      await expect
        .poll(
          () =>
            fontState(page).then(
              (s) => s.waiting,
              () => -1,
            ),
          { timeout: 60_000 },
        )
        .toBe(0);
    }
  }

  const HANGUL = Array.from({ length: 11_172 }, (_, i) => String.fromCodePoint(0xac00 + i)).join('');
  /** The characters review 5 found as boxes, a Cantonese sample, flags and emoji sequences. */
  const FOUND_MISSING = ['啰镕堃 𠮟 똠뷁쌰햏', '佢哋喺邊度？我哋冇嘢做，嘅確係噉。咁啱嘞，乜嘢都唔使驚。'];
  const EMOJI = [
    '🇯🇵 🇬🇧 ✅ ⭐ ⭐\u{FE0F} 1\u{FE0F}\u{20E3} #\u{FE0F}\u{20E3} ❤\u{FE0F} 👨\u{200D}👩\u{200D}👧 🏳\u{FE0F}\u{200D}🌈',
  ];

  test('Kiln Grid draws every Hangul syllable, the characters once missing, flags and emoji sequences', async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await page.goto('/#/grid');
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toBeVisible({ timeout: 60_000 });
    // First the fonts (a row fits its text with the fonts it has when it is entered): every
    // Hangul syllable, the samples, the emoji, below where they are compared and in view.
    await enter(page, [HANGUL, ...FOUND_MISSING, ...EMOJI], 8);
    await fontsDrawn(page);
    await expect.poll(async () => (await fontState(page)).waiting, { timeout: 60_000 }).toBe(0);
    // Laid out again with every slice it needed: nothing is left without a font.
    await enter(page, [...FOUND_MISSING, ...EMOJI], 2);
    await fontsDrawn(page);
    const state = await fontState(page);
    console.log(
      `SLICES for Hangul, the samples and emoji: ${state.slices.length} (${state.slices.join(' ')})`,
    );
    expect(state.uncovered).toBe('');
    expect(state.waiting).toBe(0);
    // Back to the top, where B2:B4 hold the samples.
    const nameBox = page
      .getByRole('combobox', { name: 'Name Box' })
      .or(page.getByRole('textbox', { name: 'Name Box' }));
    await nameBox.first().fill('A1');
    await nameBox.first().press('Enter');
    // The last sample row is entered and on the canvas.
    await expect(page.getByRole('gridcell', { name: /^B4 🇯🇵/ })).toBeAttached({ timeout: 15_000 });
    await fontsDrawn(page);
    await expect(page).toHaveScreenshot('grid-cjk-samples.png', {
      ...SHOT,
      clip: await clipOf(page, 'grid-editor', 112, 90, 420, 110),
    });
  });

  test('a document fetches only the slices its text needs', async ({ page }) => {
    test.setTimeout(180_000);
    const fetched: string[] = [];
    let bytes = 0;
    page.on('response', (r) => {
      const name = /(NotoSansCJKsc-\d+|NotoEmoji-Regular|slices)[^/?]*\.(otf|ttf|json)/.exec(r.url())?.[1];
      if (!name) return;
      fetched.push(name);
      void r.body().then(
        (b) => {
          bytes += b.length;
        },
        () => undefined,
      );
    });
    await page.goto('/#/grid');
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toBeVisible({ timeout: 60_000 });
    const runs: [string, string[]][] = [
      ['English', ['Quarterly revenue by region, in thousands']],
      ['Chinese', ['今年第一季度的销售额比去年同期增长了百分之十二，主要来自华东和华南地区的新客户。']],
      ['Korean', ['올해 1분기 매출은 작년 같은 기간보다 12% 늘었으며, 주로 새로운 고객에게서 나왔습니다.']],
    ];
    let row = 2;
    for (const [what, text] of runs) {
      const before = fetched.length;
      const b0 = bytes;
      await enter(page, text, row);
      row += text.length;
      await fontsDrawn(page);
      await expect.poll(async () => (await fontState(page)).waiting, { timeout: 60_000 }).toBe(0);
      const got = fetched.slice(before);
      console.log(`FETCH ${what}: ${got.length} files (${got.join(' ')}), ${bytes - b0} bytes`);
      if (what === 'English') expect(got).toEqual([]);
      else expect(got.length).toBeGreaterThan(0);
    }
    expect((await fontState(page)).uncovered).toBe('');
  });
});
