/**
 * Kiln Deck's slide show in the browser: every transition played with the show's clock
 * stopped half way (a screenshot of each in e2e/screenshots/deck-show/), the animation presets
 * half way through, the keys of the show, the pointer tools, touch, the Transitions and
 * Animations ribbons, the Animation Pane (reorder, timing, remove), the Animation Painter,
 * Presenter View in a second window kept in step through the relay, Present in Browser Tab, Set
 * Up Show, the previews and Rehearse Timings. Pixels are read from the show's canvas to check
 * what is on screen, not only which commands ran.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-show/${name}`, import.meta.url));

interface DeckAnim {
  id: number;
  shape: number;
  effect: string;
  option: string | null;
  start: string;
  click: number;
}
interface DeckStateLike {
  slides: { id: number; title: string; hidden: boolean; motion: boolean }[];
  current: number;
  shapes: { id: number; name: string }[];
  selection: number[];
  notes: string;
  motion: {
    transition: {
      effect: string;
      option: string | null;
      duration_ms: number;
      on_click: boolean;
      after_ms: number | null;
    } | null;
    animations: DeckAnim[];
    selected: number[];
    badges: boolean;
    settings: {
      kind: string;
      loop_show: boolean;
      from: number | null;
      to: number | null;
      presenter_view: boolean;
    };
    show: { running: boolean };
  };
}
interface DeckHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  fetch(): Promise<unknown>;
  stage: { focus(): void } | null;
}
interface EngineHandle {
  at: number;
  frames: number;
  last: {
    status: { slide: number; click: number; screen: string; tool: string; running: boolean };
    animating: boolean;
  } | null;
}
type W = {
  __kilnDeck: DeckHandle;
  __kilnDeckStage?: { stats: { frames: number } } | null;
  __kilnShowClock?: { now(): number; set(ms: number): void };
  __kilnShowEngine?: EngineHandle | null;
};

const CORE_START = 60_000;

async function openDeck(page: Page, query = '') {
  await page.goto(`/#/deck?blink=0${query}`);
  await page.waitForFunction(() => ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > 0, null, {
    timeout: CORE_START,
  });
  await page.evaluate(() => document.fonts.ready);
}

async function state(page: Page): Promise<DeckStateLike> {
  return page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as { Deck: { State: DeckStateLike } };
    return r.Deck.State;
  });
}

async function core(page: Page, command: unknown) {
  await page.evaluate(async (c) => {
    const d = (window as unknown as W).__kilnDeck;
    await d.client.execute(d.doc, { Deck: c });
    await d.fetch();
  }, command);
}

/** Types into the selected placeholder of the current slide (title or subtitle) with the keyboard. */
async function typeInto(page: Page, name: RegExp, text: string) {
  const s = await state(page);
  const shape = s.shapes.find((x) => name.test(x.name));
  if (!shape) throw new Error(`no shape matching ${name}`);
  await core(page, { SelectShapes: { ids: [shape.id] } });
  await page.evaluate(() => (window as unknown as W).__kilnDeck.stage?.focus());
  await page.keyboard.type(text);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
}

/**
 * Two slides that look nothing alike: a cream title slide with an ember square, then a deep blue
 * slide with a gold circle. The transition (and its duration) go on slide 2.
 */
/** The first slide as Title and Content (a light slide in every bundled theme). */
async function lightFirstSlide(page: Page) {
  const layouts = await page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
      Deck: { State: { layouts: { id: number; name: string }[] } };
    };
    return r.Deck.State.layouts;
  });
  const content = layouts.find((l) => l.name === 'Title and Content');
  if (content) await core(page, { SetLayout: { layout: content.id } });
  await core(page, {
    SetBackground: { fill: { Solid: { color: 'FBF6EF', brightness: null, transparency: null } }, all: false },
  });
}

async function twoSlides(page: Page) {
  await lightFirstSlide(page);
  await typeInto(page, /^Title/, 'Slide one');
  await core(page, {
    InsertShape: { preset: 'rect', rect: { x: 80, y: 330, width: 320, height: 240 } },
  });
  await core(page, {
    SetFill: { fill: { Solid: { color: 'E05A27', brightness: null, transparency: null } } },
  });
  await core(page, { NewSlide: { layout: null } });
  await core(page, {
    SetBackground: { fill: { Solid: { color: '1F3A6B', brightness: null, transparency: null } }, all: false },
  });
  await core(page, {
    InsertShape: { preset: 'ellipse', rect: { x: 820, y: 300, width: 300, height: 300 } },
  });
  await core(page, {
    SetFill: { fill: { Solid: { color: 'F2B705', brightness: null, transparency: null } } },
  });
}

/** Waits until the show painted a frame at the clock's time `t`. */
async function frameAt(page: Page, t: number) {
  await page.evaluate((ms) => (window as unknown as W).__kilnShowClock?.set(ms), t);
  await page.waitForFunction(
    (ms) => {
      const e = (window as unknown as W).__kilnShowEngine;
      return !!e && e.at === ms;
    },
    t,
    { timeout: 10_000 },
  );
  // One more animation frame for the canvas to reach the screen.
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r(null))));
}

/** The colour of the show's canvas at a point given as fractions of the slide. */
async function pixel(page: Page, fx: number, fy: number): Promise<[number, number, number]> {
  return page.evaluate(
    ([x, y]) => {
      const c = document.querySelector('[data-testid="deck-show-canvas"]') as HTMLCanvasElement | null;
      if (!c) return [0, 0, 0] as [number, number, number];
      const ctx = c.getContext('2d');
      if (!ctx) return [0, 0, 0] as [number, number, number];
      // The slide is letterboxed: find it in the canvas.
      const k = Math.min(c.width / 1280, c.height / 720);
      const ox = Math.round((c.width - 1280 * k) / 2);
      const oy = Math.round((c.height - 720 * k) / 2);
      const d = ctx.getImageData(Math.round(ox + x * 1280 * k), Math.round(oy + y * 720 * k), 1, 1).data;
      return [d[0] ?? 0, d[1] ?? 0, d[2] ?? 0] as [number, number, number];
    },
    [fx, fy] as const,
  );
}

/** A digest of the show's canvas (to tell frames apart). */
async function digest(page: Page): Promise<string> {
  return page.evaluate(() => {
    const c = document.querySelector('[data-testid="deck-show-canvas"]') as HTMLCanvasElement | null;
    const ctx = c?.getContext('2d');
    if (!c || !ctx) return '';
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    let h = 2166136261;
    for (let i = 0; i < d.length; i += 97) h = Math.imul(h ^ (d[i] ?? 0), 16777619);
    return (h >>> 0).toString(16);
  });
}

const near = (a: readonly number[], b: readonly number[], tol = 24) =>
  a.every((v, i) => Math.abs(v - (b[i] ?? 0)) <= tol);
const CREAM_OR_WHITE = (c: readonly number[]) => (c[0] ?? 0) > 200 && (c[1] ?? 0) > 200 && (c[2] ?? 0) > 180;
const BLUE: [number, number, number] = [0x1f, 0x3a, 0x6b];

/** Back in the editor after the show, with its model ready. */
async function backInEditor(page: Page) {
  await expect(page).toHaveURL(/#\/deck\?doc=\d+/);
  await page.waitForFunction(() => {
    const w = window as unknown as W;
    return !!w.__kilnDeck && (w.__kilnDeckStage?.stats.frames ?? 0) > 0;
  });
}

/** Starts the show with F5 from the editor (the keys reach the stage) and waits for slide 1. */
async function startShow(page: Page, key = 'F5') {
  await page.evaluate(() => (window as unknown as W).__kilnDeck.stage?.focus());
  await page.keyboard.press(key);
  await page.waitForSelector('[data-testid="deck-show"]', { timeout: 15_000 });
  await page.waitForFunction(() => ((window as unknown as W).__kilnShowEngine?.frames ?? 0) > 0);
}

/**
 * Plays a transition from slide 1 to slide 2 with the show's clock stopped: a frame at the start,
 * half way (the screenshot) and at the end; half way the screen is neither slide.
 */
async function playTransition(page: Page, id: string) {
  await openDeck(page, '&clock=manual');
  await twoSlides(page);
  await core(page, { SetTransition: { effect: id } });
  await core(page, { TransitionDuration: { ms: 2000 } });
  await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
  await startShow(page);
  await frameAt(page, 1000);
  const start = await digest(page);
  expect(CREAM_OR_WHITE(await pixel(page, 0.9, 0.9))).toBe(true);
  await page.keyboard.press('ArrowRight');
  await frameAt(page, 1001);
  await frameAt(page, 2001);
  const mid = await digest(page);
  await page.screenshot({ path: shot(`transition-${id}.png`) });
  await frameAt(page, 3001);
  const end = await digest(page);
  expect(near(await pixel(page, 0.05, 0.05), BLUE)).toBe(true);
  if (id === 'none' || id === 'cut') {
    expect(mid).toBe(end);
  } else {
    expect(mid).not.toBe(start);
    expect(mid).not.toBe(end);
  }
  await page.keyboard.press('Escape');
  await expect(page).toHaveURL(/#\/deck\?doc=\d+/);
}

test.describe('Deck slide show', () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test('transition None', ({ page }) => playTransition(page, 'none'));
  test('transition Morph', ({ page }) => playTransition(page, 'morph'));
  test('transition Fade', ({ page }) => playTransition(page, 'fade'));
  test('transition Push', ({ page }) => playTransition(page, 'push'));
  test('transition Wipe', ({ page }) => playTransition(page, 'wipe'));
  test('transition Split', ({ page }) => playTransition(page, 'split'));
  test('transition Reveal', ({ page }) => playTransition(page, 'reveal'));
  test('transition Cut', ({ page }) => playTransition(page, 'cut'));
  test('transition Random Bars', ({ page }) => playTransition(page, 'randomBars'));
  test('transition Shape', ({ page }) => playTransition(page, 'shape'));
  test('transition Uncover', ({ page }) => playTransition(page, 'uncover'));
  test('transition Cover', ({ page }) => playTransition(page, 'cover'));
  test('transition Flash', ({ page }) => playTransition(page, 'flash'));

  test('push moves the slides up and fade mixes them half way', async ({ page }) => {
    await openDeck(page, '&clock=manual');
    await twoSlides(page);
    await core(page, { SetTransition: { effect: 'push' } });
    await core(page, { TransitionDuration: { ms: 2000 } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    await frameAt(page, 1000);
    await page.keyboard.press('ArrowRight');
    await frameAt(page, 1001);
    await frameAt(page, 2001);
    // From Bottom: the old slide in the top half, the new one coming up below.
    expect(CREAM_OR_WHITE(await pixel(page, 0.95, 0.2))).toBe(true);
    expect(near(await pixel(page, 0.05, 0.8), BLUE)).toBe(true);
    await page.keyboard.press('Escape');
    await backInEditor(page);
    await core(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    await core(page, { SetTransition: { effect: 'fade' } });
    await core(page, { TransitionDuration: { ms: 2000 } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    await frameAt(page, 5000);
    await page.keyboard.press('Space');
    await frameAt(page, 5001);
    await frameAt(page, 6001);
    const [r, g, b] = await pixel(page, 0.95, 0.95);
    // Half cream, half blue.
    expect(r).toBeGreaterThan(110);
    expect(r).toBeLessThan(170);
    expect(b).toBeGreaterThan(130);
    expect(g).toBeLessThan(200);
  });

  test('animation presets play on their click', async ({ page }) => {
    test.setTimeout(240_000);
    const presets = [
      'appear',
      'fade',
      'flyIn',
      'floatIn',
      'split',
      'wipe',
      'zoom',
      'growTurn',
      'pulse',
      'colorPulse',
      'teeter',
      'spin',
      'growShrink',
      'disappear',
      'fadeOut',
      'flyOut',
      'zoomOut',
    ];
    await openDeck(page, '&clock=manual');
    await lightFirstSlide(page);
    await typeInto(page, /^Title/, 'Animations');
    for (let i = 0; i < presets.length; i++) {
      if (i > 0) await core(page, { NewSlide: { layout: null } });
      await core(page, {
        InsertShape: { preset: 'rect', rect: { x: 440, y: 260, width: 400, height: 240 } },
      });
      await core(page, {
        SetFill: { fill: { Solid: { color: 'E05A27', brightness: null, transparency: null } } },
      });
      await core(page, { AddAnimation: { effect: presets[i] } });
      await core(page, { AnimationDuration: { ms: 2000 } });
    }
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    let t = 1000;
    for (const [i, id] of presets.entries()) {
      await frameAt(page, t);
      const inside = await pixel(page, 0.5, 0.5);
      const entrance = ![
        'pulse',
        'colorPulse',
        'teeter',
        'spin',
        'growShrink',
        'disappear',
        'fadeOut',
        'flyOut',
        'zoomOut',
      ].includes(id);
      // Entrances start hidden; the others start in place.
      expect(near(inside, [0xe0, 0x5a, 0x27], 30), `${id} before`).toBe(!entrance);
      await page.keyboard.press('ArrowRight');
      await frameAt(page, t + 1);
      await frameAt(page, t + 1001);
      await page.screenshot({ path: shot(`animation-${id}.png`) });
      await frameAt(page, t + 2101);
      const after = await pixel(page, 0.5, 0.5);
      const exit = ['disappear', 'fadeOut', 'flyOut', 'zoomOut'].includes(id);
      expect(near(after, [0xe0, 0x5a, 0x27], 30), `${id} after`).toBe(!exit);
      if (i + 1 < presets.length) await page.keyboard.press('ArrowRight');
      t += 5000;
    }
  });

  test('the keys of the show: next, previous, number and Enter, black, white, home, end, Esc', async ({
    page,
  }) => {
    await openDeck(page);
    await typeInto(page, /^Title/, 'One');
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    const show = page.getByTestId('deck-show');
    await expect(show).toHaveAttribute('data-slide', '0');
    for (const key of ['n', 'Space', 'ArrowRight', 'PageDown']) {
      const before = Number(await show.getAttribute('data-slide'));
      await page.keyboard.press(key);
      await expect(show).toHaveAttribute('data-slide', String(Math.min(3, before + 1)));
      if (before + 1 >= 3) break;
    }
    await page.keyboard.press('p');
    await expect(show).toHaveAttribute('data-slide', '2');
    await page.keyboard.press('Backspace');
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.keyboard.press('End');
    await expect(show).toHaveAttribute('data-slide', '3');
    await page.keyboard.press('Home');
    await expect(show).toHaveAttribute('data-slide', '0');
    await page.keyboard.type('3');
    await page.keyboard.press('Enter');
    await expect(show).toHaveAttribute('data-slide', '2');
    await page.keyboard.press('b');
    await expect(show).toHaveAttribute('data-screen', 'Black');
    expect(await pixel(page, 0.5, 0.5)).toEqual([0, 0, 0]);
    await page.keyboard.press('b');
    await expect(show).toHaveAttribute('data-screen', 'Slide');
    await page.keyboard.press('w');
    await expect(show).toHaveAttribute('data-screen', 'White');
    expect(await pixel(page, 0.5, 0.5)).toEqual([255, 255, 255]);
    await page.keyboard.press('w');
    await page.keyboard.press('End');
    await page.keyboard.press('ArrowRight');
    await expect(show).toHaveAttribute('data-screen', 'End');
    await expect(page.getByText('End of slide show, click to exit.')).toBeVisible();
    await page.keyboard.press('ArrowLeft');
    await expect(show).toHaveAttribute('data-slide', '3');
    await page.keyboard.press('Escape');
    await backInEditor(page);
    // The editor is on the slide the show ended on.
    await expect.poll(async () => (await state(page)).current).toBe(3);
  });

  test('pointer tools: pen, highlighter, eraser, erase all, laser, and clicks that advance', async ({
    page,
  }) => {
    await openDeck(page);
    await lightFirstSlide(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    const show = page.getByTestId('deck-show');
    const at = async (fx: number, fy: number) => {
      const box = (await page.getByTestId('deck-show-canvas').boundingBox()) ?? {
        x: 0,
        y: 0,
        width: 1280,
        height: 720,
      };
      return { x: box.x + fx * box.width, y: box.y + fy * box.height };
    };
    await page.keyboard.press('Control+p');
    await expect(show).toHaveAttribute('data-tool', 'Pen');
    const a = await at(0.2, 0.8);
    const b = await at(0.6, 0.8);
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    for (let i = 1; i <= 10; i++) await page.mouse.move(a.x + ((b.x - a.x) * i) / 10, a.y);
    await page.mouse.up();
    // Red ink on the slide, and the pen did not advance.
    await expect.poll(async () => (await pixel(page, 0.4, 0.8))[0]).toBeGreaterThan(200);
    expect((await pixel(page, 0.4, 0.8))[1]).toBeLessThan(80);
    await expect(show).toHaveAttribute('data-slide', '0');
    await page.keyboard.press('e');
    await expect.poll(async () => CREAM_OR_WHITE(await pixel(page, 0.4, 0.8))).toBe(true);
    await page.keyboard.press('Control+i');
    await expect(show).toHaveAttribute('data-tool', 'Highlighter');
    await page.mouse.move(a.x, a.y - 60);
    await page.mouse.down();
    await page.mouse.move(b.x, a.y - 60, { steps: 8 });
    await page.mouse.up();
    // Yellow, translucent over the slide.
    await expect
      .poll(async () => {
        const [r, g, bl] = await pixel(page, 0.4, 0.8 - 60 / 720);
        return r > 200 && g > 200 && bl < 190;
      })
      .toBe(true);
    await page.keyboard.press('Control+e');
    await expect(show).toHaveAttribute('data-tool', 'Eraser');
    await page.mouse.move(a.x + 100, a.y - 70);
    await page.mouse.down();
    await page.mouse.move(a.x + 100, a.y - 50, { steps: 4 });
    await page.mouse.up();
    await expect.poll(async () => CREAM_OR_WHITE(await pixel(page, 0.4, 0.8 - 60 / 720))).toBe(true);
    await page.keyboard.press('Control+l');
    await expect(show).toHaveAttribute('data-tool', 'Laser');
    const c = await at(0.5, 0.5);
    await page.mouse.move(c.x - 20, c.y);
    await page.mouse.move(c.x, c.y, { steps: 4 });
    await expect
      .poll(async () => {
        const [r, g] = await pixel(page, 0.5, 0.5);
        return r > 200 && g < 120;
      })
      .toBe(true);
    await page.keyboard.press('Control+a');
    await expect(show).toHaveAttribute('data-tool', 'Arrow');
    await page.mouse.click(c.x, c.y);
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.screenshot({ path: shot('show-toolbar.png') });
  });

  test('a tap on the left third goes back, elsewhere advances (touch)', async ({ browser }) => {
    const context = await browser.newContext({ hasTouch: true, viewport: { width: 1024, height: 768 } });
    const page = await context.newPage();
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    const show = page.getByTestId('deck-show');
    const box = (await page.getByTestId('deck-show-canvas').boundingBox()) ?? {
      x: 0,
      y: 0,
      width: 1024,
      height: 768,
    };
    await page.touchscreen.tap(box.x + box.width * 0.8, box.y + box.height / 2);
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.touchscreen.tap(box.x + box.width * 0.1, box.y + box.height / 2);
    await expect(show).toHaveAttribute('data-slide', '0');
    await context.close();
  });

  test('the Transitions ribbon: gallery, effect options, duration, apply to all, advance', async ({
    page,
  }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await page.getByRole('tab', { name: 'Transitions' }).click();
    await page.getByRole('option', { name: 'Push' }).first().click();
    await expect.poll(async () => (await state(page)).motion.transition?.effect).toBe('push');
    await page.screenshot({ path: shot('ribbon-transitions.png') });
    await page.getByRole('button', { name: 'Effect Options' }).first().click();
    await page.getByRole('menuitem', { name: /From Left/ }).click();
    await expect.poll(async () => (await state(page)).motion.transition?.option).toBe('fromLeft');
    const duration = page.getByRole('spinbutton', { name: 'Duration' }).first();
    await duration.fill('2.5');
    await duration.press('Enter');
    await expect.poll(async () => (await state(page)).motion.transition?.duration_ms).toBe(2500);
    await page.getByRole('button', { name: 'Apply To All' }).click();
    await expect.poll(async () => (await state(page)).slides.every((s) => s.motion)).toBe(true);
    await page.getByText('On Mouse Click', { exact: true }).click();
    await expect.poll(async () => (await state(page)).motion.transition?.on_click).toBe(false);
    await page.getByText('After', { exact: true }).click();
    await expect.poll(async () => (await state(page)).motion.transition?.after_ms).toBe(0);
    // The stage keeps the keys after a ribbon command.
    await page.keyboard.press('Control+z');
    await expect.poll(async () => (await state(page)).motion.transition?.after_ms ?? null).toBe(null);
    // Preview plays on the stage and goes away.
    await page.getByRole('button', { name: 'Preview' }).first().click();
    await expect(page.getByTestId('deck-preview')).toBeVisible();
    await expect(page.getByTestId('deck-preview')).toBeHidden({ timeout: 8_000 });
  });

  test('the Animations ribbon, the Animation Pane reorder and timing, the painter and the numbers on the stage', async ({
    page,
  }) => {
    await openDeck(page);
    await core(page, { InsertShape: { preset: 'rect', rect: { x: 100, y: 100, width: 200, height: 150 } } });
    const s0 = await state(page);
    const rect = s0.selection[0] ?? 0;
    await page.getByRole('tab', { name: 'Animations' }).click();
    await page.getByRole('option', { name: 'Fly In' }).first().click();
    await expect
      .poll(async () => (await state(page)).motion.animations.map((a) => a.effect))
      .toEqual(['flyIn']);
    await page.getByRole('button', { name: 'Effect Options' }).first().click();
    await page.getByRole('menuitem', { name: /From Left/ }).click();
    await expect.poll(async () => (await state(page)).motion.animations[0]?.option).toBe('fromLeft');
    await page.getByRole('button', { name: 'Add Animation' }).click();
    await page.getByRole('menuitem', { name: 'Spin' }).click();
    await expect
      .poll(async () => (await state(page)).motion.animations.map((a) => a.effect))
      .toEqual(['flyIn', 'spin']);
    // Effect numbers on the stage while the Animations tab shows.
    await expect.poll(async () => (await state(page)).motion.badges ?? false).toBe(true);
    await page.getByRole('button', { name: 'Animation Pane' }).click();
    const panel = page.getByTestId('deck-animation-pane');
    await expect(panel).toBeVisible();
    const pane = panel.getByRole('listbox', { name: 'Effects' });
    await expect(pane.getByRole('option')).toHaveCount(2);
    await page.screenshot({ path: shot('animation-pane.png') });
    // Drag Spin above Fly In.
    await pane.getByRole('option').nth(1).dragTo(pane.getByRole('option').nth(0));
    await expect
      .poll(async () => (await state(page)).motion.animations.map((a) => a.effect))
      .toEqual(['spin', 'flyIn']);
    // Move Later puts it back.
    await expect(pane.getByRole('option').nth(0)).toContainText('Spin');
    await pane.getByRole('option').nth(0).click();
    const spin = (await state(page)).motion.animations[0]?.id;
    await expect.poll(async () => (await state(page)).motion.selected).toEqual([spin]);
    await panel.getByRole('button', { name: 'Move Later' }).click();
    await expect
      .poll(async () => (await state(page)).motion.animations.map((a) => a.effect))
      .toEqual(['flyIn', 'spin']);
    // Start with the previous one, from the pane (once the pane shows the new order).
    await expect(pane.getByRole('option').nth(1)).toContainText('Spin');
    await pane.getByRole('option').nth(1).click();
    await expect.poll(async () => (await state(page)).motion.selected.length).toBe(1);
    await panel.getByRole('combobox', { name: 'Start' }).click();
    await page
      .locator('[role="listbox"]:not([aria-label="Effects"]) [role="option"]', { hasText: 'With Previous' })
      .click();
    await expect.poll(async () => (await state(page)).motion.animations[1]?.start).toBe('WithPrevious');
    await expect.poll(async () => (await state(page)).motion.animations[1]?.click).toBe(1);
    // The Animation Painter gives an oval the same effects: pick up, then click the oval.
    await core(page, {
      InsertShape: { preset: 'ellipse', rect: { x: 700, y: 300, width: 150, height: 150 } },
    });
    const oval = (await state(page)).selection[0] ?? 0;
    await core(page, { SelectShapes: { ids: [rect] } });
    await page.getByRole('button', { name: 'Animation Painter' }).click();
    const at = await page.evaluate(() => {
      const w = window as unknown as W & {
        __kilnDeckStage: { view(): { zoom: number; left: number; top: number } } | null;
      };
      const v = w.__kilnDeckStage?.view() ?? { zoom: 1, left: 0, top: 0 };
      const r = document.querySelector('[data-testid="deck-stage"]')?.getBoundingClientRect();
      return { x: (r?.left ?? 0) + v.left + 775 * v.zoom, y: (r?.top ?? 0) + v.top + 375 * v.zoom };
    });
    await page.mouse.click(at.x, at.y);
    await expect
      .poll(async () =>
        (await state(page)).motion.animations.filter((a) => a.shape === oval).map((a) => a.effect),
      )
      .toEqual(['flyIn', 'spin']);
    // Preview plays the slide's effects on the stage and goes away.
    await page.getByRole('button', { name: 'Preview Animations' }).click();
    await expect(page.getByTestId('deck-preview')).toBeVisible();
    await expect(page.getByTestId('deck-preview')).toBeHidden({ timeout: 10_000 });
    // Remove from the pane.
    await pane.getByRole('option').first().click();
    await panel.getByRole('button', { name: 'Remove' }).click();
    await expect.poll(async () => (await state(page)).motion.animations.length).toBe(3);
  });

  test('presenter view in a second window follows the presenter and back', async ({ page, context }) => {
    await openDeck(page);
    await typeInto(page, /^Title/, 'Welcome');
    await core(page, { SetNotes: { text: 'Say hello first.' } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SetNotes: { text: 'Then the numbers.' } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await page.getByRole('tab', { name: 'Slide Show' }).click();
    await page.getByRole('button', { name: 'Use Presenter View' }).click();
    await expect.poll(async () => (await state(page)).motion.settings.presenter_view).toBe(true);
    const popup = context.waitForEvent('page');
    await page.getByRole('button', { name: 'From Beginning' }).click();
    const audience = await popup;
    await expect(page.getByTestId('deck-presenter')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId('deck-presenter-notes')).toHaveText('Say hello first.');
    await expect(audience.getByTestId('deck-show')).toBeVisible({ timeout: 60_000 });
    await expect(audience.getByTestId('deck-show')).toHaveAttribute('data-slide', '0');
    await page.screenshot({ path: shot('presenter-view.png') });
    // The presenter moves on: the audience follows.
    await page.keyboard.press('ArrowRight');
    await expect(audience.getByTestId('deck-show')).toHaveAttribute('data-slide', '1');
    await expect(page.getByTestId('deck-presenter-notes')).toHaveText('Then the numbers.');
    await expect(page.getByTestId('deck-presenter-count')).toHaveText('Slide 2 of 2');
    // A key in the audience's window moves the presenter too.
    await audience.keyboard.press('ArrowLeft');
    await expect(page.getByTestId('deck-presenter')).toHaveAttribute('data-slide', '0');
    await page.getByTestId('deck-presenter-end').click();
    await expect(page).toHaveURL(/#\/deck\?doc=\d+/);
  });

  test('presenter view: keys pressed while the audience window opens reach it', async ({ page, context }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await core(page, {
      SetShowSettings: {
        settings: {
          kind: 'Speaker',
          loop_show: false,
          no_narration: false,
          no_animation: false,
          from: null,
          to: null,
          manual: false,
          presenter_view: true,
        },
      },
    });
    const popup = context.waitForEvent('page');
    await page.getByRole('tab', { name: 'Slide Show' }).click();
    await page.getByRole('button', { name: 'From Beginning' }).click();
    await expect(page).toHaveURL(/view=presenter&channel=/);
    // The moment the presenter's window answers the audience with the presentation, the presenter
    // presses Next and B: the audience is still opening it.
    await page.evaluate(() => {
      const name = decodeURIComponent(/channel=([^&]+)/.exec(window.location.hash)?.[1] ?? '');
      const ch = new BroadcastChannel(name);
      ch.onmessage = (e: MessageEvent) => {
        if ((e.data as { kind?: string })?.kind !== 'snapshot') return;
        ch.close();
        for (const key of ['ArrowRight', 'b'])
          document.body.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
      };
    });
    const audience = await popup;
    const show = audience.getByTestId('deck-show');
    await expect(show).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId('deck-presenter')).toHaveAttribute('data-slide', '1');
    await expect(show).toHaveAttribute('data-slide', '1');
    await expect(show).toHaveAttribute('data-screen', 'Black');
    // And they stay together.
    await page.keyboard.press('b');
    await page.keyboard.press('ArrowRight');
    await expect(page.getByTestId('deck-presenter')).toHaveAttribute('data-slide', '2');
    await expect(show).toHaveAttribute('data-slide', '2');
    await expect(show).toHaveAttribute('data-screen', 'Slide');
    await page.getByTestId('deck-presenter-end').click();
    await expect(page).toHaveURL(/#\/deck\?doc=\d+/);
  });

  test('see all slides: Tab or the arrows to a slide and Enter go to it', async ({ page }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await startShow(page);
    const show = page.getByTestId('deck-show');
    await page.mouse.move(400, 300);
    await page.getByRole('button', { name: 'See all slides' }).click();
    const grid = page.getByTestId('deck-show-grid');
    await expect(grid).toBeVisible();
    await expect(grid.getByRole('button', { name: 'Slide 1' })).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Tab');
    await expect(grid.getByRole('button', { name: 'Slide 3' })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(grid).toBeHidden();
    await expect(show).toHaveAttribute('data-slide', '2');
    // Space on a slide goes to it too.
    await page.getByRole('button', { name: 'See all slides' }).click();
    await expect(grid.getByRole('button', { name: 'Slide 3' })).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(grid.getByRole('button', { name: 'Slide 2' })).toBeFocused();
    await page.keyboard.press('Space');
    await expect(grid).toBeHidden();
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.keyboard.press('Escape');
    await backInEditor(page);
  });

  test('present in browser tab, from current slide, hidden slides skipped', async ({ page }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    await page.getByRole('tab', { name: 'Slide Show' }).click();
    await page.getByRole('button', { name: 'Hide Slide' }).click();
    await expect.poll(async () => (await state(page)).slides[1]?.hidden).toBe(true);
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await page.getByRole('button', { name: 'Present in Browser Tab' }).click();
    await expect(page).toHaveURL(/#\/present\?doc=\d+&tab=1/);
    const show = page.getByTestId('deck-show');
    await expect(show).toHaveAttribute('data-slide', '0');
    await page.keyboard.press('ArrowRight');
    await expect(show).toHaveAttribute('data-slide', '2');
    await page.keyboard.press('Escape');
    await backInEditor(page);
    await core(page, { SelectSlide: { index: 2, extend: false, toggle: false } });
    await page.evaluate(() => (window as unknown as W).__kilnDeck.stage?.focus());
    await page.keyboard.press('Shift+F5');
    await expect(page.getByTestId('deck-show')).toHaveAttribute('data-slide', '2');
    await page.keyboard.press('Escape');
  });

  test('set up show: kiosk loops a range of slides and the settings save', async ({ page }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await core(page, { NewSlide: { layout: null } });
    await page.getByRole('tab', { name: 'Slide Show' }).click();
    await page.getByRole('button', { name: 'Set Up Slide Show' }).click();
    const dlg = page.getByTestId('deck-setup-show');
    await expect(dlg).toBeVisible();
    await expect(dlg).toHaveAttribute('data-kiln-shortcuts', 'off');
    await dlg.getByLabel('Browsed by an individual (window)').check();
    await dlg.getByLabel('Loop continuously until ‘Esc’').check();
    await dlg.getByRole('spinbutton', { name: 'From slide' }).fill('2');
    await dlg.getByRole('spinbutton', { name: 'To slide' }).fill('3');
    await page.getByRole('button', { name: 'OK' }).click();
    await expect
      .poll(async () => (await state(page)).motion.settings)
      .toMatchObject({
        kind: 'Window',
        loop_show: true,
        from: 2,
        to: 3,
      });
    // Focus is back on the slide: typing selects nothing odd, the keys reach the stage.
    await page.keyboard.press('F5');
    // Browsed by an individual: in the tab.
    await expect(page).toHaveURL(/#\/present\?doc=\d+&tab=1/);
    const show = page.getByTestId('deck-show');
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.keyboard.press('ArrowRight');
    await expect(show).toHaveAttribute('data-slide', '2');
    await page.keyboard.press('ArrowRight');
    // Loops back to slide 2 of the range.
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.keyboard.press('Escape');
  });

  test('rehearse timings keeps the time of each slide', async ({ page }) => {
    await openDeck(page);
    await core(page, { NewSlide: { layout: null } });
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    // Rehearse Timings is a P1 command, hidden in builds of the v0.1 tier: start it as its button does.
    await page.evaluate(() =>
      (window as unknown as { __kilnDeck: { present: (r: unknown) => void } }).__kilnDeck.present({
        from: null,
        mode: 'Rehearse',
        tab: false,
        presenter: false,
      }),
    );
    await expect(page.getByTestId('deck-rehearse')).toBeVisible();
    await page.waitForTimeout(1200);
    await page.keyboard.press('ArrowRight');
    await page.waitForTimeout(700);
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('deck-keep-timings')).toBeVisible();
    await page.getByRole('button', { name: 'Yes' }).click();
    await backInEditor(page);
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await expect.poll(async () => (await state(page)).motion.transition?.after_ms ?? 0).toBeGreaterThan(1000);
  });
});
