import { fileURLToPath } from 'node:url';
import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';
import {
  openPlayground,
  openText,
  paragraphs,
  selectedText,
  settle,
  stats,
  toClient,
} from './surface-helpers';

/**
 * The canvas surface end to end (WP-54): the playground route paints a scratch document laid out
 * by the core; typing goes through the input proxy, IME composition through real CDP IME events,
 * selection through hit testing in the core, and the accessibility mirror exposes the text.
 * Needs the wasm build: `cargo xtask wasm` (or `--release`) before `pnpm e2e`.
 */

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/surface/${name}`, import.meta.url));

const LOREM =
  'Twice a year we load the big catenary kiln together. Stoneware takes the back of the chamber, where the flame path is longest; porcelain and the test tiles sit up front so we can watch them through the spy hole.';

/** Top of the first line: page top (32) + margin (96). */
const FIRST_LINE = 128;
/** Left of the text column: side pad (64) + margin (96). */
const TEXT_X = 160;

test.describe('surface', () => {
  test.beforeEach(async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await openPlayground(page);
  });

  test('types text through the input proxy', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await openText(page, 'Hello');
    const proxy = page.getByTestId('kiln-input-proxy');
    await expect(proxy).toBeFocused();
    await expect(proxy).toHaveAttribute('spellcheck', 'false');
    await expect(proxy).toHaveAttribute('autocapitalize', 'off');
    await page.keyboard.press('Control+End');
    await page.keyboard.type(' world');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Kiln Write');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln Write']);
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Control+Backspace');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln ']);
    // Consecutive deletions are one undo step.
    await page.keyboard.press('Control+z');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln Write']);
    await page.keyboard.insertText(' – ✓');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln Write – ✓']);
    // The proxy never keeps text: the document is the core's.
    await expect(proxy).toHaveText('');
    expect(errors).toEqual([]);
    await settle(page);
    await page.screenshot({ path: shot('typing-light-desktop.png') });
  });

  test('composes text with an IME (CDP) and commits it', async ({ page }) => {
    await openText(page, 'Kiln ');
    await page.keyboard.press('Control+End');
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 });
    await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
    // The composition is in the document (and painted underlined) before it is committed.
    await expect.poll(() => paragraphs(page)).toEqual(['Kiln にほん']);
    await settle(page);
    await page.screenshot({
      path: shot('ime-composing-light-desktop.png'),
      clip: { x: 0, y: 0, width: 720, height: 260 },
    });
    await cdp.send('Input.insertText', { text: '日本' });
    await expect.poll(() => paragraphs(page)).toEqual(['Kiln 日本']);
    // Committed as one typing step.
    await page.keyboard.type('!');
    await expect.poll(() => paragraphs(page)).toEqual(['Kiln 日本!']);
    await page.keyboard.press('Control+z');
    await expect.poll(() => paragraphs(page)).toEqual(['Kiln ']);
    await expect(page.getByTestId('kiln-input-proxy')).toHaveText('');
  });

  test('selects with a mouse drag, double click and Shift+arrows', async ({ page }) => {
    await openText(page, `${LOREM}\nSecond paragraph of the firing plan.`);
    const lineY = FIRST_LINE + 9;
    // Drag across the first words.
    const from = await toClient(page, TEXT_X + 1, lineY);
    const to = await toClient(page, TEXT_X + 160, lineY);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move((from.x + to.x) / 2, to.y, { steps: 4 });
    await page.mouse.move(to.x, to.y, { steps: 4 });
    await page.mouse.up();
    await expect.poll(() => selectedText(page)).toMatch(/^Twice a year we/);
    const dragged = await selectedText(page);
    await page.keyboard.press('Shift+ArrowRight');
    await expect.poll(() => selectedText(page)).toBe(LOREM.slice(0, dragged.length + 1));
    await page.keyboard.press('Shift+End');
    await expect.poll(async () => (await selectedText(page)).length).toBeGreaterThan(60);
    // Double click selects the word and its trailing space, as in Word.
    const word = await toClient(page, TEXT_X + 70, lineY);
    await page.mouse.dblclick(word.x, word.y);
    await expect.poll(() => selectedText(page)).toBe('year ');
    // Drag down into the second paragraph with the button held: the selection follows.
    const start = await toClient(page, TEXT_X + 1, lineY);
    await page.mouse.click(start.x, start.y);
    await page.mouse.down();
    const below = await toClient(page, TEXT_X + 40, FIRST_LINE + 90);
    await page.mouse.move(below.x, below.y, { steps: 6 });
    await page.mouse.up();
    await expect.poll(() => selectedText(page)).toContain(`${LOREM}\nSecond`);
    await settle(page);
    await page.screenshot({
      path: shot('selection-light-desktop.png'),
      clip: { x: 0, y: 0, width: 1000, height: 340 },
    });
    // Shift+Ctrl+Home extends to the start of the document; typing replaces the selection.
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Shift+Home');
    await page.keyboard.type('Replaced.');
    await expect.poll(() => paragraphs(page)).toEqual(['Replaced.']);
  });

  test('scrolls a 500-paragraph document at 60 fps', async ({ page }) => {
    const paras = Array.from({ length: 500 }, (_, i) => `${i + 1}. ${LOREM}`);
    await openText(page, paras.join('\n'), 'long.txt');
    const before = await stats(page);
    const result = await page.evaluate(async () => {
      const scroller = document.querySelector('[data-testid="kiln-surface-scroller"]') as HTMLElement;
      const frames: number[] = [];
      const long: number[] = [];
      const measures = new PerformanceObserver((list) => {
        for (const e of list.getEntries()) if (e.name === 'kiln:frame') frames.push(e.duration);
      });
      measures.observe({ type: 'measure', buffered: false });
      let loaf: PerformanceObserver | null = null;
      try {
        loaf = new PerformanceObserver((list) => {
          for (const e of list.getEntries()) long.push(e.duration);
        });
        loaf.observe({ type: 'long-animation-frame', buffered: false });
      } catch {
        loaf = null;
      }
      const intervals: number[] = [];
      await new Promise<void>((done) => {
        let last = performance.now();
        let n = 0;
        const step = (t: number) => {
          intervals.push(t - last);
          last = t;
          scroller.scrollTop += 36;
          n += 1;
          if (n < 300) requestAnimationFrame(step);
          else done();
        };
        requestAnimationFrame(step);
      });
      await new Promise((r) => setTimeout(r, 100));
      measures.disconnect();
      loaf?.disconnect();
      const sorted = [...frames].sort((a, b) => a - b);
      const p95 = sorted[Math.floor(sorted.length * 0.95)] ?? Number.POSITIVE_INFINITY;
      const intervalsSorted = intervals.slice(5).sort((a, b) => a - b);
      return {
        frames: frames.length,
        p95,
        max: sorted.at(-1) ?? 0,
        intervalP50: intervalsSorted[Math.floor(intervalsSorted.length * 0.5)] ?? 0,
        dropped: intervals.slice(5).filter((i) => i > 25).length,
        longFrames: long.length,
        scrolled: scroller.scrollTop,
      };
    });
    test.info().annotations.push({ type: 'scroll', description: JSON.stringify(result) });
    console.log('scroll benchmark', JSON.stringify(result));
    expect(result.frames).toBeGreaterThan(200);
    // B6: the surface's work per animation frame (composite, overlay, prefetched tile steps),
    // measured with PerformanceObserver, p95 ≤ 16.7 ms; CI runners get the 1.5× tolerance of
    // PLAN.md §3.9 (shared 4-vCPU runners with software rendering).
    expect(result.p95).toBeLessThanOrEqual(process.env.CI ? 16.7 * 1.5 : 16.7);
    // …and the page keeps the display's cadence while scrolling (median frame interval).
    expect(result.intervalP50).toBeLessThanOrEqual(17.5);
    expect(result.scrolled).toBeGreaterThan(9000);
    const after = await stats(page);
    expect(after.tilesRendered).toBeGreaterThan(before.tilesRendered + 5);
    await settle(page);
    await page.screenshot({ path: shot('scrolled-light-desktop.png') });
  });

  test('mirrors the visible text for assistive technology and passes axe', async ({ page }) => {
    await openText(page, `${LOREM}\nSecond paragraph.\nThird paragraph.`);
    const mirror = page.getByTestId('kiln-a11y-mirror');
    await expect(mirror).toHaveAttribute('role', 'document');
    await expect(mirror.locator('p')).toHaveText([LOREM, 'Second paragraph.', 'Third paragraph.']);
    // Off-screen, not display: none.
    expect(await mirror.evaluate((el) => getComputedStyle(el).display)).not.toBe('none');
    const proxy = page.getByTestId('kiln-input-proxy');
    await page.keyboard.press('Control+End');
    await expect
      .poll(async () => {
        const id = await proxy.getAttribute('aria-activedescendant');
        return id ? page.locator(`#${id}`).textContent() : null;
      })
      .toBe('Third paragraph.');
    const results = await new AxeBuilder({ page }).analyze();
    const serious = results.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
    expect(serious, JSON.stringify(serious, null, 2)).toEqual([]);
  });

  test('looks right in dark mode on a phone', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await openText(page, `${LOREM}\n${LOREM}\n${LOREM}`);
    const s = await stats(page);
    // The page is zoomed to fit the phone's width.
    expect(s.zoom).toBeLessThan(0.5);
    await page.screenshot({ path: shot('surface-dark-phone.png') });
  });
});
