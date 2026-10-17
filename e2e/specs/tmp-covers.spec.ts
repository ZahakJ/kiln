import { fileURLToPath } from 'node:url';
import { test } from '@playwright/test';

const shot = (n: string) => fileURLToPath(new URL(`../screenshots/tmp/${n}`, import.meta.url));
type W = { __kilnWrite?: { controller?: { getStats(): { tiles: number } } }; __kiln?: { registry: { execute(id: string, a?: unknown): Promise<unknown> } } };

test('covers', async ({ page }) => {
  await page.goto('/#/write');
  await page.waitForFunction(() => ((window as unknown as W).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0, null, { timeout: 45_000 });
  await page.getByTestId('kiln-input-proxy').focus();
  await page.keyboard.type('Twice a year we load the big catenary kiln together.');
  const run = (id: string, a?: unknown) => page.evaluate(([i, x]) => (window as unknown as W).__kiln?.registry.execute(i as string, x), [id, a] as const);
  await run('write.file.properties', { props: { title: 'The Autumn Firing', subject: 'Loading plan and schedule', author: 'Studio co-op' } });
  await run('write.view.zoom', { percent: 60 }).catch(() => undefined);
  for (const d of ['Kiln', 'Banded', 'Ledger', 'Classic', 'Slab', 'Quiet']) {
    await run('write.insert.cover_page', { design: d });
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Home');
    await page.waitForTimeout(700);
    await page.screenshot({ path: shot(`cover-${d}.png`) });
  }
});
