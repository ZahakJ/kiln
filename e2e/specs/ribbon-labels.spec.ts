/**
 * no ribbon label is cut off or spills over its group in English, German or Arabic. Every
 * tab of Kiln Write, Grid and Deck is opened in the Classic ribbon of a desktop window, and each
 * group name and button label is measured by its text: inside its group, inside its button,
 * clear of the button's dropdown arrow, and not clipped by the ribbon.
 */
import { expect, test } from '@playwright/test';
import { loaded } from './i18n-helpers.ts';

/**
 * The labels of the ribbon tab on show that are cut off, each with why. A label is measured by its
 * ink, not only its box: the glyphs' real extent (Arabic tails and descenders, German umlauts) is
 * taken from the font with `measureText` and placed on the label's baseline, then checked against
 * every box that clips it, its group, its button, its dropdown arrow and the buttons beside it.
 */
async function cutLabels(page: import('@playwright/test').Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    const panel = document.querySelector('.kiln-ribbon__groups');
    if (!panel) return ['no ribbon'];
    type Box = { left: number; right: number; top: number; bottom: number };
    const canvas = document.createElement('canvas').getContext('2d');
    if (!canvas) return ['no canvas'];
    /** The label's text box (its line boxes) and its ink box (the glyphs as drawn). */
    const boxesOf = (el: HTMLElement): { text: Box; ink: Box } | null => {
      const range = document.createRange();
      range.selectNodeContents(el);
      const rects = [...range.getClientRects()].filter((r) => r.width > 0);
      if (rects.length === 0) return null;
      const text = {
        left: Math.min(...rects.map((r) => r.left)),
        right: Math.max(...rects.map((r) => r.right)),
        top: Math.min(...rects.map((r) => r.top)),
        bottom: Math.max(...rects.map((r) => r.bottom)),
      };
      // The baseline of the last line: a zero-size inline box sits on it.
      const probe = document.createElement('span');
      probe.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline';
      el.append(probe);
      const lastBaseline = probe.getBoundingClientRect().bottom;
      probe.remove();
      const tops = [...new Set(rects.map((r) => Math.round(r.top)))].sort((a, b) => a - b);
      const lineGap = tops.length > 1 ? (tops.at(-1) ?? 0) - (tops[0] ?? 0) : 0;
      const cs = getComputedStyle(el);
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      canvas.direction = cs.direction === 'rtl' ? 'rtl' : 'ltr';
      canvas.textAlign = 'left';
      let ascent = 0;
      let descent = 0;
      let overLeft = 0;
      let overRight = 0;
      for (const word of (el.textContent ?? '').trim().split(/\s+/)) {
        const m = canvas.measureText(word);
        ascent = Math.max(ascent, m.actualBoundingBoxAscent);
        descent = Math.max(descent, m.actualBoundingBoxDescent);
        overLeft = Math.max(overLeft, m.actualBoundingBoxLeft);
        overRight = Math.max(overRight, m.actualBoundingBoxRight - m.width);
      }
      return {
        text,
        ink: {
          left: text.left - overLeft,
          right: text.right + overRight,
          top: lastBaseline - lineGap - ascent,
          bottom: lastBaseline + descent,
        },
      };
    };
    const SLACK = 0.5;
    const insideX = (a: Box, b: Box) => a.left >= b.left - SLACK && a.right <= b.right + SLACK;
    const insideY = (a: Box, b: Box) => a.top >= b.top - SLACK && a.bottom <= b.bottom + SLACK;
    const overlaps = (a: Box, b: Box) =>
      a.left < b.right - 1 && a.right > b.left + 1 && a.top < b.bottom - 1 && a.bottom > b.top + 1;
    const labels = panel.querySelectorAll<HTMLElement>(
      '.kiln-ribbon__group-label, .kiln-btn__label, .kiln-ribbon__number-label',
    );
    for (const el of labels) {
      if (!el.offsetParent) continue;
      const text = (el.textContent ?? '').trim();
      const boxes = text ? boxesOf(el) : null;
      if (!boxes) continue;
      const { ink } = boxes;
      const why: string[] = [];
      const group = el.closest('.kiln-ribbon__group');
      if (group && !insideX(boxes.text, group.getBoundingClientRect())) why.push('spills over its group');
      const button = el.closest('button, .kiln-btn');
      if (button && button !== el && !insideX(boxes.text, button.getBoundingClientRect()))
        why.push('wider than its button');
      const arrow = button?.querySelector('.kiln-btn__icon-after');
      if (arrow) {
        const a = arrow.getBoundingClientRect();
        if (a.width && overlaps(ink, a)) why.push('under its arrow');
      }
      // The buttons beside it (the same group) keep clear of its ink.
      if (button && group) {
        for (const other of group.querySelectorAll('button, .kiln-btn, .kiln-split')) {
          if (other === button || other.contains(button) || button.contains(other)) continue;
          if (!(other as HTMLElement).offsetParent) continue;
          // A split button's own arrow half: the label stays clear of the arrow drawn on it.
          const sameSplit = other.closest('.kiln-split') === button.closest('.kiln-split');
          const mark = sameSplit ? other.querySelector('svg') : other;
          if (mark && overlaps(ink, mark.getBoundingClientRect())) {
            why.push(
              `runs into ${(other.textContent ?? '').trim() || other.getAttribute('aria-label') || 'a neighbour'}`,
            );
            break;
          }
        }
      }
      // Every box that clips it (the label itself included) holds its whole ink.
      for (let p: HTMLElement | null = el; p && p !== panel.parentElement; p = p.parentElement) {
        const cs = getComputedStyle(p);
        const r = p.getBoundingClientRect();
        if (cs.overflowX !== 'visible' && !insideX(ink, r)) {
          why.push(p === el ? 'cut at its side' : 'clipped');
          break;
        }
        if (cs.overflowY !== 'visible' && !insideY(ink, r)) {
          why.push(p === el ? 'glyphs cut at the top or bottom' : 'clipped');
          break;
        }
      }
      if (el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflowX !== 'visible')
        why.push('ends in an ellipsis');
      if (why.length > 0) out.push(`${text}: ${why.join(', ')}`);
    }
    // A combo box's placeholder (the font size box: Size, Grad, حجم) is shown whole.
    for (const input of panel.querySelectorAll<HTMLInputElement>('input[placeholder]')) {
      if (!input.offsetParent || input.value || !input.placeholder) continue;
      const cs = getComputedStyle(input);
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const room = input.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
      if (canvas.measureText(input.placeholder).width > room + SLACK)
        out.push(`${input.placeholder}: placeholder cut short`);
    }
    return out;
  });
}

test.describe('i18n: ribbon labels are whole', () => {
  for (const lang of ['en', 'de', 'ar'] as const) {
    for (const app of ['write', 'grid', 'deck'] as const) {
      test(`${lang}: every tab of Kiln ${app} shows its group names and labels whole`, async ({ page }) => {
        await page.addInitScript(
          (l) => localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: l } })),
          lang,
        );
        await page.goto(`/#/${app}`);
        await loaded(page, {});
        const tabs = page.locator('.kiln-ribbon [role="tab"]');
        const found: string[] = [];
        const count = await tabs.count();
        expect(count).toBeGreaterThan(4);
        for (let i = 0; i < count; i++) {
          const tab = tabs.nth(i);
          if (!(await tab.isVisible())) continue;
          await tab.click();
          await page.waitForTimeout(120);
          for (const cut of await cutLabels(page))
            found.push(`${(await tab.textContent())?.trim()} ▸ ${cut}`);
        }
        expect(found, found.join('\n')).toEqual([]);
      });
    }
  }
});
