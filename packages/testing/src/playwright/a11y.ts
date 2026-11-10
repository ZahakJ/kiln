/**
 * Accessibility checks with axe-core (0 serious or critical violations on Home, every
 * app, Backstage and the dialogs). WCAG 2.2 A and AA rules plus axe's best practices.
 */
import AxeBuilder from '@axe-core/playwright';
import type { Page } from '@playwright/test';

export type Impact = 'minor' | 'moderate' | 'serious' | 'critical';

export interface A11yViolation {
  id: string;
  impact: Impact | null;
  help: string;
  helpUrl: string;
  /** CSS selectors of the offending nodes. */
  targets: string[];
}

export interface AxeOptions {
  /** Only check inside these selectors. */
  include?: string[];
  /** Skip these selectors (third-party widgets, the document canvas). */
  exclude?: string[];
  /** Rule ids to turn off, each with a reason in the calling test. */
  disableRules?: string[];
}

const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'];

/**
 * Waits (at most a second) for the page's finite CSS animations and transitions to end: a dialog
 * still fading in, or a theme still changing colour, has text at part opacity that axe measures
 * as low contrast.
 */
async function settleAnimations(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const finite = document
      .getAnimations()
      .filter(
        (a) => a.playState === 'running' && a.effect?.getTiming().iterations !== Number.POSITIVE_INFINITY,
      );
    const done = Promise.all(finite.map((a) => a.finished.catch(() => undefined)));
    await Promise.race([done, new Promise((r) => setTimeout(r, 1000))]);
  });
}

/** Runs axe on the page and returns every violation. */
export async function axeScan(page: Page, options: AxeOptions = {}): Promise<A11yViolation[]> {
  await settleAnimations(page);
  let builder = new AxeBuilder({ page }).withTags(TAGS);
  for (const sel of options.include ?? []) builder = builder.include(sel);
  for (const sel of options.exclude ?? []) builder = builder.exclude(sel);
  if (options.disableRules?.length) builder = builder.disableRules(options.disableRules);
  const results = await builder.analyze();
  return results.violations.map((v) => ({
    id: v.id,
    impact: (v.impact ?? null) as Impact | null,
    help: v.help,
    helpUrl: v.helpUrl,
    targets: v.nodes.map((n) => n.target.join(' ')),
  }));
}

/** The violations that fail a gate (serious and critical). */
export function blockingViolations(violations: readonly A11yViolation[]): A11yViolation[] {
  return violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
}

/** One line per violation, for assertion messages. */
export function formatViolations(violations: readonly A11yViolation[]): string {
  return violations
    .map((v) => `${v.impact ?? 'unknown'} ${v.id}: ${v.help} (${v.targets.slice(0, 3).join(', ')})`)
    .join('\n');
}

/** Throws when the page has serious or critical violations. */
export async function expectNoSeriousA11yViolations(page: Page, options: AxeOptions = {}): Promise<void> {
  const blocking = blockingViolations(await axeScan(page, options));
  if (blocking.length > 0) {
    throw new Error(
      `${blocking.length} serious or critical accessibility violation(s):\n${formatViolations(blocking)}`,
    );
  }
}
