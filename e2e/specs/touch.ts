/**
 * Touch gestures for the phone and tablet specs: a finger's drag, a long press, a double
 * tap and a two-finger pinch, sent as touch events through the Chrome DevTools Protocol (the
 * Pixel 7 and iPad projects run on Chromium; Playwright's own touchscreen only taps).
 */
import type { CDPSession, Page } from '@playwright/test';

type Point = { x: number; y: number };

const sessions = new WeakMap<Page, CDPSession>();

async function cdp(page: Page): Promise<CDPSession> {
  let s = sessions.get(page);
  if (!s) {
    s = await page.context().newCDPSession(page);
    sessions.set(page, s);
  }
  return s;
}

async function touch(page: Page, type: 'touchStart' | 'touchMove' | 'touchEnd', points: readonly Point[]) {
  const s = await cdp(page);
  await s.send('Input.dispatchTouchEvent', {
    type,
    touchPoints: points.map((p, i) => ({
      x: Math.round(p.x),
      y: Math.round(p.y),
      id: i,
      radiusX: 4,
      radiusY: 4,
      force: 1,
    })),
  });
}

/** One finger from `from` to `to` in `steps` moves, then lifted. */
export async function drag(page: Page, from: Point, to: Point, steps = 8, holdMs = 0): Promise<void> {
  await touch(page, 'touchStart', [from]);
  if (holdMs) await page.waitForTimeout(holdMs);
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    await touch(page, 'touchMove', [{ x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t }]);
    await page.waitForTimeout(16);
  }
  await touch(page, 'touchEnd', []);
}

/** A finger held still for `ms` (a long press). */
export async function longPress(page: Page, at: Point, ms = 700): Promise<void> {
  await touch(page, 'touchStart', [at]);
  await page.waitForTimeout(ms);
  await touch(page, 'touchEnd', []);
}

/** Two quick taps in the same place. */
export async function doubleTap(page: Page, at: Point): Promise<void> {
  await touch(page, 'touchStart', [at]);
  await touch(page, 'touchEnd', []);
  await page.waitForTimeout(90);
  await touch(page, 'touchStart', [at]);
  await touch(page, 'touchEnd', []);
}

/** Two fingers about `center`, `from` px apart, moved to `to` px apart (apart: zoom in). */
export async function pinch(page: Page, center: Point, from: number, to: number, steps = 10): Promise<void> {
  const at = (d: number): Point[] => [
    { x: center.x - d / 2, y: center.y },
    { x: center.x + d / 2, y: center.y },
  ];
  // One finger, then the second, as fingers land.
  await touch(page, 'touchStart', at(from).slice(0, 1));
  await touch(page, 'touchStart', at(from));
  for (let i = 1; i <= steps; i++) {
    await touch(page, 'touchMove', at(from + ((to - from) * i) / steps));
    await page.waitForTimeout(16);
  }
  await touch(page, 'touchEnd', []);
}

/** The open menus that reach outside the window (sideways or up and down), with how far. */
export async function menusOffScreen(page: Page): Promise<string[]> {
  return page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>('[role="menu"]')]
      .filter((m) => m.getBoundingClientRect().width > 0)
      .flatMap((m) => {
        const r = m.getBoundingClientRect();
        const out = [];
        if (r.left < -0.5) out.push(`left ${Math.round(r.left)}`);
        if (r.right > window.innerWidth + 0.5)
          out.push(`right ${Math.round(r.right - window.innerWidth)} past`);
        if (r.top < -0.5) out.push(`top ${Math.round(r.top)}`);
        if (r.bottom > window.innerHeight + 0.5)
          out.push(`bottom ${Math.round(r.bottom - window.innerHeight)} past`);
        return out.map((o) => `${m.getAttribute('aria-label') ?? 'menu'}: ${o}`);
      }),
  );
}
