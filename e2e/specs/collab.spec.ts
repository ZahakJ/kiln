/**
 * Collaboration in the browser: two people, each in their own browser context, share a
 * Kiln Write document through a collaboration server the test starts. Concurrent typing
 * converges (also text typed at the end of a paragraph the other splits at the same moment), each
 * sees the other's caret and name, edits made offline merge when the connection returns (and
 * leaving the page with edits not yet sent asks first), and Version History shows a version
 * read-only (typing changes nothing, saving writes the document), compares and restores it.
 * Screenshots go to e2e/screenshots/collab/.
 */
import { fileURLToPath } from 'node:url';
import { type Browser, type BrowserContext, expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import {
  type CollabServer,
  collabState,
  join,
  savedDocx,
  share,
  startServer,
  zipEntry,
} from './collab-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/collab/${name}`, import.meta.url));

interface WriteHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  controller: { getStats(): { tiles: number } } | null;
}
type KilnWindow = {
  __kilnWrite?: WriteHandle;
  __kiln?: { registry: { execute(id: string, args?: unknown): Promise<{ status: string }> } };
};

async function waitForWrite(page: Page): Promise<void> {
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
}

/** The body's paragraphs as the core has them. */
async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { id: number; text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

/** Puts the caret at an offset of paragraph `index`. */
async function caretAt(page: Page, index: number, offset: number): Promise<void> {
  await page.evaluate(
    async ([i, o]) => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
      const block = r.Paragraphs[i as number]?.id;
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: o }, extend: false } } });
    },
    [index, offset] as const,
  );
  await page.getByTestId('kiln-input-proxy').focus();
}

/** Switches the colour scheme and lets the chrome's transitions finish. */
async function scheme(page: Page, colorScheme: 'light' | 'dark'): Promise<void> {
  await page.emulateMedia({ colorScheme });
  await page.waitForTimeout(400);
}

async function run(page: Page, id: string): Promise<void> {
  await page.evaluate(async (i) => {
    await (window as unknown as KilnWindow).__kiln?.registry.execute(i);
  }, id);
}

/** Both people see `want`. */
async function converged(a: Page, b: Page, want: string[]): Promise<void> {
  await expect.poll(() => paragraphs(a), { timeout: 20_000 }).toEqual(want);
  await expect.poll(() => paragraphs(b), { timeout: 20_000 }).toEqual(want);
}

/** Ana opens a new document, types "Hello world" and shares it; Ben opens the link. */
async function shareWrite(
  browser: Browser,
  server: CollabServer,
): Promise<{ a: Page; b: Page; contexts: BrowserContext[] }> {
  const ca = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const cb = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const a = await ca.newPage();
  const b = await cb.newPage();
  await a.goto('/#/write?blink=0');
  await waitForWrite(a);
  await a.getByTestId('kiln-input-proxy').focus();
  await a.keyboard.type('Hello world');
  await expect.poll(() => paragraphs(a)).toEqual(['Hello world']);
  const link = await share(a, server, 'Ana Martín');
  await join(b, link, 'Ben Okafor');
  await waitForWrite(b);
  await expect.poll(() => paragraphs(b), { timeout: 30_000 }).toEqual(['Hello world']);
  return { a, b, contexts: [ca, cb] };
}

test.describe('collab', () => {
  test.describe.configure({ mode: 'serial' });
  // The first run builds the server.
  test.setTimeout(240_000);
  let server: CollabServer;
  test.beforeAll(async () => {
    test.setTimeout(900_000);
    server = await startServer();
  });
  test.afterAll(async () => {
    await server?.stop();
  });

  test('concurrent typing converges, also at the end of a paragraph the other splits', async ({
    browser,
  }) => {
    const { a, b, contexts } = await shareWrite(browser, server);
    try {
      // Both type at once in different places.
      await caretAt(a, 0, 0);
      await caretAt(b, 0, 11);
      await Promise.all([a.keyboard.type('Dear all. '), b.keyboard.type(' again')]);
      await converged(a, b, ['Dear all. Hello world again']);
      // Ana types at the end of the paragraph while Ben splits it before "world".
      await contexts[1]?.setOffline(true);
      await expect.poll(async () => (await collabState(b, '__kilnWrite')).status).toBe('Offline');
      await caretAt(b, 0, 16);
      await b.keyboard.press('Enter');
      await caretAt(a, 0, 27);
      await a.keyboard.type('!');
      await expect.poll(() => paragraphs(b)).toEqual(['Dear all. Hello ', 'world again']);
      await contexts[1]?.setOffline(false);
      await converged(a, b, ['Dear all. Hello ', 'world again!']);
    } finally {
      for (const c of contexts) await c.close();
    }
  });

  test('cursors and names are visible to the other person', async ({ browser }) => {
    const { a, b, contexts } = await shareWrite(browser, server);
    try {
      await caretAt(b, 0, 5);
      await b.keyboard.type(',');
      await converged(a, b, ['Hello, world']);
      // Ben's name flag at his caret, his avatar in the title bar.
      await expect(a.getByTestId('remote-flags').getByText('Ben Okafor')).toBeVisible({ timeout: 20_000 });
      await expect(
        a.getByTestId('presence').getByRole('button', { name: /Ben Okafor, editing/ }),
      ).toBeVisible();
      await expect(
        b.getByTestId('presence').getByRole('button', { name: /Ana Martín, editing/ }),
      ).toBeVisible();
      const flag = await a.getByTestId('remote-flags').getByText('Ben Okafor').boundingBox();
      expect(flag?.width ?? 0).toBeGreaterThan(20);
      await a.screenshot({ path: shot('remote-caret-light-desktop.png') });
      await a
        .getByTestId('presence')
        .getByRole('button', { name: /\(you\)/ })
        .click();
      await expect(a.getByTestId('share-dialog')).toBeVisible();
      await expect(a.getByTestId('share-status')).toContainText('1 other person is here');
      await a.getByTestId('share-dialog').screenshot({ path: shot('share-dialog-light-desktop.png') });
      // The dialog is what this step checks; behind its backdrop the name flags are dimmed on purpose.
      await expectNoSeriousA11yViolations(a, { include: ['[data-testid="share-dialog"]'] });
      await scheme(a, 'dark');
      await a.getByTestId('share-dialog').screenshot({ path: shot('share-dialog-dark-desktop.png') });
      await a.getByRole('button', { name: 'Done' }).click();
      await a.screenshot({ path: shot('remote-caret-dark-desktop.png') });
      // On a phone the title bar keeps the avatars.
      await scheme(a, 'light');
      await a.setViewportSize({ width: 412, height: 915 });
      await expect(
        a.getByTestId('presence').getByRole('button', { name: /Ben Okafor, editing/ }),
      ).toBeVisible();
      await a.screenshot({ path: shot('presence-light-phone.png') });
    } finally {
      for (const c of contexts) await c.close();
    }
  });

  test('edits made offline merge when the connection returns', async ({ browser }) => {
    const { a, b, contexts } = await shareWrite(browser, server);
    try {
      await contexts[1]?.setOffline(true);
      await expect.poll(async () => (await collabState(b, '__kilnWrite')).status).toBe('Offline');
      await expect(b.getByTestId('presence').getByRole('img', { name: /Offline/ })).toBeVisible();
      await caretAt(b, 0, 0);
      await b.keyboard.type('Offline. ');
      await caretAt(a, 0, 11);
      await a.keyboard.type(' online');
      await expect.poll(() => paragraphs(a)).toEqual(['Hello world online']);
      await expect.poll(async () => (await collabState(b, '__kilnWrite')).queued).toBeGreaterThan(0);
      // Edits not yet sent live only in the page: leaving it asks first.
      let asked = '';
      b.once('dialog', (d) => {
        asked = d.type();
        void d.dismiss();
      });
      await b.close({ runBeforeUnload: true });
      await expect.poll(() => asked).toBe('beforeunload');
      expect(b.isClosed()).toBe(false);
      await contexts[1]?.setOffline(false);
      await converged(a, b, ['Offline. Hello world online']);
      await expect.poll(async () => (await collabState(b, '__kilnWrite')).status).toBe('Online');
    } finally {
      for (const c of contexts) await c.close();
    }
  });

  test('version history: a version is shown read-only, compared with the document and restored', async ({
    browser,
  }) => {
    const { a, b, contexts } = await shareWrite(browser, server);
    try {
      await caretAt(b, 0, 5);
      await b.keyboard.type(' big');
      await converged(a, b, ['Hello big world']);
      await run(a, 'suite.file.version_history');
      const pane = a.getByTestId('version-history');
      await expect(pane).toBeVisible();
      const version = pane.getByRole('button', { name: /Shared.*Ana Martín/ });
      await expect(version).toBeVisible();
      // The version in place of the document, read-only.
      await version.click();
      await expect(a.getByTestId('version-bar')).toBeVisible();
      await expect.poll(() => paragraphs(a)).toEqual(['Hello world']);
      // Compared with the document: "big" was added since.
      await pane.getByTestId('version-compare').click();
      const diff = pane.getByTestId('version-diff');
      await expect(diff.locator('.kiln-versions__para ins')).toHaveText(/big/);
      await a.screenshot({ path: shot('version-history-light-desktop.png') });
      await expectNoSeriousA11yViolations(a);
      await scheme(a, 'dark');
      await a.screenshot({ path: shot('version-history-dark-desktop.png') });
      await scheme(a, 'light');
      // Typing into the version changes nothing.
      await a.getByTestId('kiln-input-proxy').focus();
      await a.keyboard.type('xyz');
      await a.waitForTimeout(300);
      expect(await paragraphs(a)).toEqual(['Hello world']);
      // Saving writes the document, not the version on show, and shows the document again.
      await a.evaluate(async () => {
        // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
        delete (window as any).showSaveFilePicker;
        const root = await navigator.storage.getDirectory();
        await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
      });
      await a.keyboard.press('Control+s');
      const menu = a.getByRole('dialog', { name: /File menu/ });
      await menu.waitFor({ timeout: 5_000 }).catch(() => undefined);
      if (await menu.isVisible())
        await menu
          .getByRole('button', { name: /Word Document/ })
          .first()
          .click();
      const saved = await savedDocx(a);
      const body = zipEntry(saved, 'word/document.xml')?.toString('utf8') ?? '';
      expect(body).toContain('big');
      expect(body).not.toContain('xyz');
      await expect(a.getByTestId('version-bar')).toBeHidden();
      await expect.poll(() => paragraphs(a)).toEqual(['Hello big world']);
      // Restored for everyone.
      await version.click();
      await pane.getByTestId('version-restore').click();
      await converged(a, b, ['Hello world']);
      await expect(pane.getByRole('button', { name: /Restored from/ })).toBeVisible();
    } finally {
      for (const c of contexts) await c.close();
    }
  });
});
