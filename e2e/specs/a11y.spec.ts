/**
 * Accessibility end to end:
 *
 * - **axe** (WCAG 2.2 A/AA and best practices, 0 serious or critical) on Home, every app frame in
 *   light, dark and high contrast, every ribbon tab (contextual ones in their context), every
 *   Backstage page, the Options pages, and every dialog and pane: these are found by running
 *   every command of the app's registry on a blank document and on documents with a table, a
 *   picture, a chart or a shape selected, and scanning whatever each one opens.
 * - **Keyboard reach**: every catalog command of the build (the tier rule of the chrome) is reached
 *   from the keyboard alone, through the ribbon (Key Tips and arrows, menus and submenus), the
 *   title bar, the status bar, Backstage, the context menu, its shortcut, the command palette
 *   (Alt+Q) or the document's own keys, with a visible focus indicator at every stop.
 * - **Key Tips**: unique and prefix-free on every tab and menu level, Office's letters where
 *   Office has them; the audit table in docs/SHORTCUTS.md is the record (KILN_UPDATE_KEYTIPS=1
 *   rewrites it).
 * - The **Accessible text view**, live **announcements**, **reduced motion**, F6 **focus order**,
 *   **high contrast** and Windows **contrast themes** (forced colors).
 */
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import AxeBuilder from '@axe-core/playwright';
import { expect, type Page, test } from '@playwright/test';
import { RIBBONS } from '../../packages/commands/src/index.ts';
import {
  commonKeymap,
  deckKeymap,
  gridKeymap,
  writeKeymap,
} from '../../packages/commands/src/keymaps/index.ts';
import type { KeyTipsAuditItem, KeyTipsAuditTab } from '../../packages/commands/src/shortcutsDoc.ts';
import {
  type AxeOptions,
  axeScan,
  blockingViolations,
  formatViolations,
} from '../../packages/testing/src/playwright/a11y.ts';
import { type CollabServer, startServer, TOKEN } from './collab-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => {
  mkdirSync(repo('e2e/screenshots/a11y'), { recursive: true });
  return repo(`e2e/screenshots/a11y/${name}`);
};

type App = 'write' | 'grid' | 'deck' | 'notes';
/** The apps of v0.1. */
const APPS: readonly App[] = ['write', 'grid', 'deck'];
const EDITORS: readonly Exclude<App, 'notes'>[] = ['write', 'grid', 'deck'];

interface Registry {
  list(o: { visibleOnly?: boolean }): { id: string; title: string; pending: boolean; palette?: boolean }[];
  execute(id: string, args?: unknown): Promise<{ status: string }>;
  getState(id: string): { enabled: boolean; pending: boolean; visible: boolean; exists: boolean };
}
interface Client {
  open(bytes: Uint8Array, hint: object): Promise<{ doc: number }>;
  execute(doc: number, c: unknown): Promise<unknown>;
  query(doc: number, q: unknown): Promise<Record<string, unknown>>;
  a11y(doc: number): Promise<{ nodes: { id: number; role: string; name: string | null }[] }>;
}
type W = {
  __kiln?: {
    app: string;
    registry: Registry;
    client: Client | null;
    executed: { id: string; status: string }[];
    resolved: { key: string; command: string }[];
    log(): { id: string; outcome: string }[];
    keymap: {
      resolve(
        e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean },
        ctx: Record<string, unknown>,
      ): { command: string } | undefined;
      bindings?: unknown;
    };
  };
  __kilnWrite?: { doc: number };
  __kilnGrid?: { doc: number };
  __kilnDeck?: { doc: number };
};

// ── Opening documents ────────────────────────────────────────────────────────────────────────

/** Opens an app on a new document and waits for the core's answer. */
async function openApp(page: Page, app: App, query = '', fresh = true): Promise<void> {
  // A fresh page: a command may have left the window anywhere (a closed document, a view).
  if (fresh && page.url() !== 'about:blank') await page.goto('about:blank');
  await page.goto(`/#/${app}?blink=0${query}`);
  await expect(page.getByRole('tablist', { name: 'Ribbon tabs' })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: 60_000 });
  if (app !== 'notes')
    await page.waitForFunction((a) => !!(window as unknown as W)[`__kiln${a}` as '__kilnWrite'], cap(app), {
      timeout: 60_000,
    });
  await page.evaluate(() => document.fonts.ready);
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Opens a fixture in its app (through that app's core) and shows it. */
async function openFixture(page: Page, app: Exclude<App, 'notes'>, path: string): Promise<void> {
  await openApp(page, app);
  const name = path.split('/').pop() ?? path;
  const doc = await page.evaluate(
    async ([b, file]) => {
      const client = (window as unknown as W).__kiln?.client;
      if (!client) throw new Error('no client');
      return (await client.open(Uint8Array.from(b as number[]), { name: file, path: null, read_only: false }))
        .doc;
    },
    [Array.from(readFileSync(repo(path))), name] as const,
  );
  await openApp(page, app, `&doc=${doc}`, false);
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.[a-z]+$/, ''));
}

async function docOf(page: Page, app: Exclude<App, 'notes'>): Promise<number> {
  return page.evaluate((a) => (window as unknown as W)[`__kiln${a}` as '__kilnWrite']?.doc ?? -1, cap(app));
}

async function core(page: Page, app: Exclude<App, 'notes'>, command: unknown): Promise<void> {
  const doc = await docOf(page, app);
  await page.evaluate(
    async ([d, c]) => {
      await (window as unknown as W).__kiln?.client?.execute(d as number, c);
    },
    [doc, command] as const,
  );
  await page.waitForTimeout(150);
}

async function treeNodes(page: Page, app: Exclude<App, 'notes'>, query: 'A11y' | 'A11yDocument' = 'A11y') {
  const doc = await docOf(page, app);
  return page.evaluate(
    async ([d, q]) => {
      const r = (await (window as unknown as W).__kiln?.client?.query(d as number, q)) as {
        A11y: {
          nodes: { id: number; role: string; name: string | null; text: string | null; children: number[] }[];
        };
      };
      return r.A11y.nodes;
    },
    [doc, query] as const,
  );
}

/** Document states that bring out the contextual commands, tabs, dialogs and panes. */
interface Context {
  name: string;
  setup: (page: Page) => Promise<void>;
  /** Dialogs and panes explored in this context (the keyboard reach test). */
  openers?: readonly string[];
}

/** A document with a misspelling, comments and tracked changes: the review panes have work. */
async function reviewed(page: Page, app: Exclude<App, 'notes'>, fixture: string): Promise<void> {
  await openFixture(page, app, fixture);
}

const CONTEXTS: Readonly<Record<Exclude<App, 'notes'>, Context[]>> = {
  write: [
    { name: 'blank', setup: (page) => openApp(page, 'write') },
    {
      name: 'table',
      setup: async (page) => {
        await openFixture(page, 'write', 'fixtures/docx/features/tables.docx');
        const nodes = await treeNodes(page, 'write', 'A11yDocument');
        const cell = nodes.find((n) => n.role === 'Cell' || n.role === 'ColumnHeader');
        const para = cell ? nodes.find((n) => n.id === cell.children[0]) : undefined;
        if (para)
          await core(page, 'write', {
            Write: { SelectTo: { pos: { block: para.id, offset: 0 }, extend: false } },
          });
      },
    },
    {
      name: 'picture',
      setup: async (page) => {
        await openFixture(page, 'write', 'fixtures/docx/features/images-inline.docx');
        const nodes = await treeNodes(page, 'write', 'A11yDocument');
        const pic = nodes.find((n) => n.role === 'Image');
        if (pic) await core(page, 'write', { Write: { SelectObject: { id: pic.id } } });
      },
    },
    {
      name: 'shape',
      setup: async (page) => {
        await openFixture(page, 'write', 'fixtures/docx/features/text-boxes.docx');
        const nodes = await treeNodes(page, 'write', 'A11yDocument');
        const box = nodes.find((n) => n.role === 'TextBox');
        if (box) await core(page, 'write', { Write: { SelectObject: { id: box.id } } });
      },
    },
    {
      name: 'comments',
      setup: (page) => reviewed(page, 'write', 'fixtures/docx/features/comments.docx'),
      openers: ['ui.write.comments_pane'],
    },
    {
      name: 'tracked changes',
      setup: (page) => reviewed(page, 'write', 'fixtures/docx/features/tracked-changes.docx'),
      openers: ['write.review.reviewing_pane'],
    },
    {
      name: 'misspelt',
      setup: async (page) => {
        await openApp(page, 'write');
        await page.getByTestId('kiln-input-proxy').focus();
        await page.keyboard.type('Teh kiln hass a fier. ');
        await page.waitForTimeout(400);
      },
      openers: ['suite.review.spelling', 'suite.arrange.selection_pane'],
    },
    {
      name: 'chart',
      setup: async (page) => {
        await openApp(page, 'write');
        await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('suite.chart.insert'));
        await page.waitForTimeout(300);
      },
    },
  ],
  grid: [
    { name: 'blank', setup: (page) => openApp(page, 'grid') },
    {
      name: 'table',
      setup: async (page) => {
        await openFixture(page, 'grid', 'fixtures/xlsx/features/tables.xlsx');
        await core(page, 'grid', { Grid: { SelectCell: { row: 1, col: 1, extend: false, add: false } } });
      },
      // The dialogs that need data: they act on the table around the active cell.
      openers: [
        'grid.data.sort',
        'grid.data.subtotal',
        'grid.data.remove_duplicates',
        'grid.insert.pivot_table',
        'grid.insert.sparkline_line',
        'grid.insert.table',
        'ui.grid.resize_table',
        'grid.data.goal_seek',
        'grid.formulas.define_name',
        'grid.formulas.name_manager',
        'grid.review.protect_sheet',
        'grid.review.protect_workbook',
        'ui.grid.cf_rule',
        'ui.grid.insert_cells',
        'ui.grid.delete_cells',
        'ui.grid.go_to_special',
        'suite.insert.link',
        'grid.review.new_note',
        'suite.edit.replace',
        'grid.styles.format_as_table',
        'grid.cells.format_cells',
        'ui.grid.custom_lists',
      ],
    },
    {
      name: 'comments',
      setup: (page) => reviewed(page, 'grid', 'fixtures/xlsx/features/comments.xlsx'),
      openers: [
        'suite.review.new_comment',
        'suite.review.spelling',
        'grid.data.sort',
        'grid.data.subtotal',
        'grid.data.remove_duplicates',
        'grid.formulas.define_name',
        'grid.formulas.name_manager',
        'grid.insert.pivot_table',
        'grid.insert.sparkline_line',
        'grid.data.goal_seek',
        'grid.review.protect_sheet',
        'grid.review.protect_workbook',
        'ui.grid.tab_color',
        'ui.grid.custom_lists',
        'grid.insert.table',
        'ui.grid.cf_rule',
        'grid.formulas.watch_window',
        'grid.formulas.error_checking',
      ],
    },
    {
      name: 'chart',
      setup: async (page) => {
        await openFixture(page, 'grid', 'fixtures/xlsx/features/charts.xlsx');
        await core(page, 'grid', { Grid: { SelectDrawing: { index: 0 } } });
      },
    },
    {
      name: 'pivot',
      setup: async (page) => {
        await openFixture(page, 'grid', 'fixtures/xlsx/features/pivot-tables.xlsx');
        // The Report sheet's PivotTable is A3:F9.
        await core(page, 'grid', { Grid: { ActivateSheet: { index: 1 } } });
        await core(page, 'grid', { Grid: { SelectCell: { row: 4, col: 1, extend: false, add: false } } });
      },
    },
  ],
  deck: [
    { name: 'blank', setup: (page) => openApp(page, 'deck') },
    {
      name: 'notes',
      setup: (page) => reviewed(page, 'deck', 'fixtures/pptx/features/notes.pptx'),
      openers: [
        'suite.review.new_comment',
        'suite.review.spelling',
        'deck.animations.animation_pane',
        'deck.drawing.design_ideas',
        'ui.deck.rename_section',
        'suite.edit.find',
        'suite.edit.replace',
      ],
    },
    ...(['pictures', 'tables', 'charts', 'preset-shapes'] as const).map((f) => ({
      name: f,
      setup: async (page: Page) => {
        await openFixture(page, 'deck', `fixtures/pptx/features/${f}.pptx`);
        const nodes = await treeNodes(page, 'deck');
        const want =
          f === 'pictures' ? 'Image' : f === 'tables' ? 'Table' : f === 'charts' ? 'Image' : 'TextBox';
        const shape =
          nodes.find((n, i) => i > 0 && n.role === want) ?? nodes.find((n) => n.role === 'TextBox');
        if (shape) await core(page, 'deck', { Deck: { SelectShapes: { ids: [shape.id] } } });
      },
    })),
  ],
};

// ── axe ──────────────────────────────────────────────────────────────────────────────────────

/** Throws with the list when the page (or `include`) has serious or critical violations. */
async function expectAxe(page: Page, what: string, options: AxeOptions = {}): Promise<void> {
  const blocking = blockingViolations(await axeScan(page, options));
  expect(blocking, `${what}:\n${formatViolations(blocking)}`).toEqual([]);
}

const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'];

/** Serious and critical findings inside `selector`, with axe's reason for each node. */
/**
 * The high-contrast theme holds WCAG's enhanced contrast (1.4.6, 7:1; 4.5:1 for large text):
 * axe's `color-contrast-enhanced` findings inside `selector` (the whole page when absent).
 */
async function enhancedContrast(page: Page, selector?: string): Promise<string[]> {
  let b = new AxeBuilder({ page }).withRules(['color-contrast-enhanced']);
  if (selector) b = b.include(selector);
  const r = await b.analyze();
  return r.violations.flatMap((v) =>
    v.nodes
      .slice(0, 4)
      .map((n) => `${v.id} ${n.target.join(' ')}: ${n.any.map((a) => a.message).join('; ')}`),
  );
}

async function axeFindings(page: Page, selector: string): Promise<string[]> {
  await page.evaluate(async () => {
    const running = document.getAnimations().filter((a) => a.playState === 'running');
    await Promise.race([
      Promise.all(running.map((a) => a.finished.catch(() => undefined))),
      new Promise((r) => setTimeout(r, 1000)),
    ]);
  });
  const r = await new AxeBuilder({ page }).include(selector).withTags(TAGS).analyze();
  const out: string[] = [];
  for (const v of r.violations.filter((x) => x.impact === 'serious' || x.impact === 'critical'))
    for (const n of v.nodes.slice(0, 4)) {
      // The element as it was, inside its label when it has one (what the finding is about).
      const where = await page
        .locator(n.target.join(' '))
        .first()
        .evaluate((e) => (e.closest('label') ?? e).outerHTML.slice(0, 240))
        .catch(() => n.html.slice(0, 240));
      out.push(
        `${v.impact} ${v.id} ${n.target.join(' ')} ${where}: ${n.any.map((a) => a.message).join('; ')}`,
      );
    }
  return out;
}

/** What a command opened: a dialog, a pane, Backstage, a menu or nothing. */
async function surfaces(page: Page): Promise<{ kind: string; name: string; selector: string }[]> {
  return page.evaluate(() => {
    const out: { kind: string; name: string; selector: string }[] = [];
    const nameOf = (el: Element) =>
      el.getAttribute('aria-label') ??
      (el.getAttribute('aria-labelledby')
        ? document.getElementById(el.getAttribute('aria-labelledby') ?? '')?.textContent
        : null) ??
      el.querySelector('h1, h2, h3')?.textContent ??
      '';
    let n = 0;
    const mark = (el: Element, kind: string) => {
      const id = `kiln-a11y-surface-${n++}`;
      el.setAttribute('data-a11y-surface', id);
      out.push({ kind, name: (nameOf(el) ?? '').trim(), selector: `[data-a11y-surface="${id}"]` });
    };
    for (const el of document.querySelectorAll('[role="dialog"]:not(.kiln-popover), [role="alertdialog"]'))
      mark(el, el.classList.contains('kiln-backstage') ? 'backstage' : 'dialog');
    for (const el of document.querySelectorAll(
      '.kiln-appframe__body aside, .kiln-appframe__body [role="complementary"], .kiln-taskpane',
    ))
      if (!el.parentElement?.closest('aside, .kiln-taskpane')) mark(el, 'pane');
    for (const el of document.querySelectorAll('.kiln-popover[role="dialog"]')) mark(el, 'popover');
    return out;
  });
}

/** Closes whatever is open (Esc, the panes' Close buttons); reloads the app when that fails. */
async function closeAll(page: Page, reopen: () => Promise<void>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    const open = await page
      .locator('[role="dialog"], [role="alertdialog"], [role="menu"], [data-testid="keytips"]')
      .count();
    if (open === 0) break;
    await page.keyboard.press('Escape');
    await page.waitForTimeout(120);
  }
  await page.evaluate(() => {
    for (const b of document.querySelectorAll<HTMLButtonElement>(
      '.kiln-appframe__body aside button[aria-label^="Close"], .kiln-taskpane button[aria-label^="Close"]',
    ))
      b.click();
  });
  const stuck = await page.locator('[role="dialog"], [role="alertdialog"]').count();
  const frame = await page
    .getByRole('tablist', { name: 'Ribbon tabs' })
    .isVisible()
    .catch(() => false);
  const title = await page
    .getByTestId('document-title')
    .textContent()
    .catch(() => null);
  if (stuck > 0 || !frame || title === 'Opening…') await reopen();
}

/** Commands not run while looking for dialogs: they leave the window or take it over. */
const LEAVES =
  /^(suite\.file\.(new|open|close|save|save_as|save_copy|print|exit|reload)$|deck\.show\.|suite\.window\.|ui\.view\.text_view$|suite\.file\.share$|suite\.collab\.|ui\.theme\.)/;

// ── Keyboard reach ───────────────────────────────────────────────────────────────────────────

interface Stop {
  /** A marker that is the same each time the same element has the keys. */
  key: string;
  command: string | null;
  label: string;
  role: string;
  /** `aria-haspopup`, for controls that open a menu or popover. */
  popup: string | null;
  /** Shown and on screen. */
  visible: boolean;
  /** A focus indicator is drawn (an outline or a focus shadow on it or around it). */
  ring: boolean;
  /** Where it is: ribbon, menu, dialog, backstage, palette, titlebar, statusbar, document. */
  region: string;
  textEntry: boolean;
}

/** What has the keys now, and whether it shows that it has. */
async function stop(page: Page): Promise<Stop | null> {
  return page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el || el === document.body || el === document.documentElement) return null;
    const w = window as unknown as { __a11yVisit?: number };
    if (!el.dataset.a11yVisit) {
      w.__a11yVisit = (w.__a11yVisit ?? 0) + 1;
      el.dataset.a11yVisit = String(w.__a11yVisit);
    }
    const r = el.getBoundingClientRect();
    const visible =
      r.width > 0 &&
      r.height > 0 &&
      r.bottom > 0 &&
      r.right > 0 &&
      r.top < innerHeight &&
      r.left < innerWidth;
    const drawn = (cs: CSSStyleDeclaration) =>
      (cs.outlineStyle !== 'none' &&
        Number.parseFloat(cs.outlineWidth) >= 1 &&
        cs.outlineColor !== 'rgba(0, 0, 0, 0)') ||
      (cs.boxShadow !== 'none' && cs.boxShadow !== '');
    let ring = false;
    let e: HTMLElement | null = el;
    for (let d = 0; e && d < 4 && !ring; d++, e = e.parentElement) {
      const cs = getComputedStyle(e);
      // An ancestor's shadow is a container's (menus and dialogs have one) unless the ancestor is
      // the field around a text box (its ring is drawn round the whole field).
      const container = e.matches(
        '[role="menu"], [role="dialog"], [role="listbox"], [role="toolbar"], [role="tabpanel"], [role="group"]',
      );
      if (
        d === 0
          ? drawn(cs)
          : (cs.outlineStyle !== 'none' && Number.parseFloat(cs.outlineWidth) >= 1) ||
            (d <= 2 && !container && cs.boxShadow !== 'none')
      )
        ring = true;
      for (const pseudo of ['::before', '::after']) {
        const p = getComputedStyle(e, pseudo);
        if (p.content !== 'none' && p.content !== '' && drawn(p)) ring = true;
      }
    }
    // The document's input takes the keys under the caret, which the canvas draws (the active
    // cell's frame in Grid, the selection's handles in Deck): that is its focus indicator.
    // So is the slide show, which is the whole screen.
    if (
      el.matches(
        '[data-testid="kiln-input-proxy"], [data-kiln-grid-proxy], [data-kiln-deck-proxy], [data-testid="deck-show"]',
      )
    )
      ring = true;
    // A highlighted menu or list row is its own indicator (a filled row, Office's look).
    if (
      !ring &&
      (el.matches('[data-highlighted], [aria-selected="true"]') || el.closest('[data-highlighted]'))
    ) {
      const bg = getComputedStyle(el).backgroundColor;
      ring = bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent';
    }
    const region = el.closest('[role="menu"]')
      ? 'menu'
      : el.closest('.kiln-backstage')
        ? 'backstage'
        : el.closest('.kiln-palette, [cmdk-root]')
          ? 'palette'
          : el.closest('[role="dialog"], [role="alertdialog"]')
            ? 'dialog'
            : el.closest('.kiln-ribbon')
              ? 'ribbon'
              : el.closest('.kiln-titlebar')
                ? 'titlebar'
                : el.closest('.kiln-statusbar')
                  ? 'statusbar'
                  : 'document';
    const tag = el.tagName.toLowerCase();
    const type = (el as HTMLInputElement).type ?? '';
    return {
      key: el.dataset.a11yVisit,
      command: el.closest('[data-command]')?.getAttribute('data-command') ?? null,
      label:
        (el.getAttribute('aria-label') ?? el.textContent ?? '').trim().slice(0, 60) +
        (visible
          ? ''
          : ` @${Math.round(r.x)},${Math.round(r.y)} ${Math.round(r.width)}x${Math.round(r.height)}`),
      role: el.getAttribute('role') ?? tag,
      popup: el.getAttribute('aria-haspopup'),
      visible,
      ring,
      region,
      textEntry:
        tag === 'textarea' || (tag === 'input' && !['button', 'checkbox', 'radio', 'range'].includes(type)),
    };
  });
}

interface Reach {
  /** Command id → how the keyboard got there ("ribbon Home ▸ Bold"). */
  reached: Map<string, string>;
  /** Stops with no visible focus indicator. */
  unseen: string[];
}

function note(reach: Reach, s: Stop, path: string): void {
  if (!s.visible || !s.ring)
    reach.unseen.push(`${path} [${s.role} "${s.label}"]${s.visible ? '' : ' off screen'}`);
  if (s.command && !reach.reached.has(s.command)) reach.reached.set(s.command, path);
}

async function key(page: Page, k: string): Promise<void> {
  await page.keyboard.press(k);
  await page.waitForTimeout(40);
}

/** Walks an open menu with the arrows, submenus with → and ←. */
async function walkMenu(page: Page, reach: Reach, path: string, depth = 0): Promise<void> {
  await expect(page.locator('[role="menu"]').last()).toBeVisible();
  await page.waitForTimeout(60);
  let first: string | null = null;
  for (let i = 0; i < 80; i++) {
    const s = await stop(page);
    if (s?.region !== 'menu') break;
    // The menu itself takes the keys first; the arrows go to its items.
    if (s.role === 'menu') {
      await key(page, 'ArrowDown');
      if (i > 2) break;
      continue;
    }
    if (s.key === first) break;
    first ??= s.key;
    note(reach, s, `${path} ▸ ${s.label}`);
    if (s.popup === 'menu' && depth < 3) {
      await key(page, 'ArrowRight');
      const inner = await stop(page);
      if (inner && inner.key !== s.key) {
        await walkMenu(page, reach, `${path} ▸ ${s.label}`, depth + 1);
        await key(page, 'ArrowLeft');
      }
    }
    await key(page, 'ArrowDown');
  }
}

/** Walks a popover (an expanded gallery, a colour picker) with Tab until the keys leave or wrap. */
async function walkPopover(page: Page, reach: Reach, path: string): Promise<void> {
  await page.waitForTimeout(120);
  const seen = new Set<string>();
  for (let i = 0; i < 60; i++) {
    const s = await stop(page);
    if (
      !s ||
      seen.has(s.key) ||
      !(await page.evaluate(() => !!document.activeElement?.closest('.kiln-popover')))
    )
      break;
    seen.add(s.key);
    note(reach, s, `${path} ▸ ${s.label}`);
    await key(page, 'Tab');
  }
}

/** F6 until the keys are in `region` (Office's F6 cycle). */
async function f6To(page: Page, region: string): Promise<boolean> {
  for (let i = 0; i < 7; i++) {
    await key(page, 'F6');
    if ((await stop(page))?.region === region) return true;
  }
  return false;
}

/** Every tab of the ribbon: its controls with ← →, their menus and popovers. */
async function walkRibbon(page: Page, reach: Reach, only?: (tab: string) => boolean): Promise<string[]> {
  expect(await f6To(page, 'ribbon'), 'F6 reaches the ribbon').toBe(true);
  // The keys are on the selected tab; Home first.
  await key(page, 'Home');
  const tabs: string[] = [];
  const count = await page.getByRole('tablist', { name: 'Ribbon tabs' }).getByRole('tab').count();
  for (let t = 0; t < count; t++) {
    const tab = await stop(page);
    if (tab?.role !== 'tab') break;
    tabs.push(tab.label);
    note(reach, tab, `ribbon ${tab.label}`);
    if (!only || only(tab.label)) {
      await key(page, 'ArrowDown');
      let first: string | null = null;
      for (let i = 0; i < 400; i++) {
        const s = await stop(page);
        if (!s || s.region !== 'ribbon' || s.role === 'tab' || s.key === first) break;
        first ??= s.key;
        const path = `ribbon ${tab.label} ▸ ${s.label || s.command}`;
        note(reach, s, path);
        if ((s.popup === 'menu' || s.popup === 'true') && s.role !== 'combobox') {
          await key(page, 'Enter');
          if ((await stop(page))?.region === 'menu') {
            await walkMenu(page, reach, path);
            await key(page, 'Escape');
          }
        } else if (s.popup === 'dialog' && s.role !== 'combobox') {
          await key(page, 'Enter');
          await walkPopover(page, reach, path);
          await key(page, 'Escape');
        }
        // A text box keeps the arrows until its caret is at the end (a spin box's End would set
        // its maximum, so it is left alone: its value has no caret to move).
        if (s.textEntry && s.role === 'combobox') await key(page, 'End');
        await key(page, 'ArrowRight');
      }
      await key(page, 'Escape');
      // Back on the tab (Escape from a command returns there).
      const back = await stop(page);
      if (back?.role !== 'tab') await f6To(page, 'ribbon');
    }
    await key(page, 'ArrowRight');
  }
  return tabs;
}

/** Tab through a region (title bar, status bar) until the keys leave it. */
async function walkRegion(page: Page, reach: Reach, region: string): Promise<void> {
  if (!(await f6To(page, region))) return;
  const seen = new Set<string>();
  for (let i = 0; i < 40; i++) {
    const s = await stop(page);
    if (!s || s.region !== region || seen.has(s.key)) break;
    seen.add(s.key);
    note(reach, s, `${region} ▸ ${s.label}`);
    await key(page, 'Tab');
  }
}

/** File (Alt, F): every page, its controls with Tab. */
async function walkBackstage(page: Page, reach: Reach): Promise<void> {
  await f6To(page, 'document');
  await key(page, 'Alt');
  await key(page, 'f');
  await expect(page.locator('.kiln-backstage')).toBeVisible();
  const ids = await page
    .locator('.kiln-backstage__nav-item[data-backstage]')
    .evaluateAll((els) => els.map((e) => e.getAttribute('data-backstage') ?? ''));
  for (const id of ids) {
    if (['save', 'close', 'options'].includes(id)) {
      // Actions: reaching them is enough (Enter would close the window or open Options).
      const s = await page.evaluate((i) => {
        const b = document.querySelector<HTMLElement>(`.kiln-backstage__nav-item[data-backstage="${i}"]`);
        return b?.getAttribute('data-command') ?? null;
      }, id);
      for (let i = 0; i < 40; i++) {
        const now = await stop(page);
        if (now?.command === s && now?.region === 'backstage') {
          note(reach, now, `File ▸ ${now.label}`);
          break;
        }
        await key(page, 'Tab');
      }
      continue;
    }
    for (let i = 0; i < 40; i++) {
      const on = await page.evaluate((x) => document.activeElement?.getAttribute('data-backstage') === x, id);
      if (on) break;
      await key(page, 'Tab');
    }
    const nav = await stop(page);
    if (nav) note(reach, nav, `File ▸ ${nav.label}`);
    await key(page, 'Enter');
    await page.waitForTimeout(150);
    const seen = new Set<string>();
    for (let i = 0; i < 80; i++) {
      await key(page, 'Tab');
      const s = await stop(page);
      if (!s || seen.has(s.key) || s.region !== 'backstage') break;
      if (await page.evaluate(() => !!document.activeElement?.closest('.kiln-backstage__rail'))) break;
      seen.add(s.key);
      note(reach, s, `File ▸ ${id} ▸ ${s.label}`);
      if (s.popup === 'menu') {
        await key(page, 'Enter');
        if ((await stop(page))?.region === 'menu') {
          await walkMenu(page, reach, `File ▸ ${id} ▸ ${s.label}`);
          await key(page, 'Escape');
        }
      }
    }
    // Print and Export: their default button (Print, Export to PDF) with Enter.
    if (id === 'print' || id === 'export') {
      for (let i = 0; i < 60; i++) {
        if (
          await page.evaluate(() => !!document.activeElement?.matches('.kiln-backstage .kiln-btn--primary'))
        ) {
          await viaRunning(page, reach, `File ▸ ${id} ▸ Enter on its default button`, () =>
            key(page, 'Enter'),
          );
          break;
        }
        await key(page, 'Tab');
      }
      if (
        !(await page
          .locator('.kiln-backstage')
          .isVisible()
          .catch(() => false))
      ) {
        await key(page, 'Alt');
        await key(page, 'f');
      }
    }
  }
  await key(page, 'Escape');
}

/** The context menu of the document (Shift+F10). */
async function walkContextMenu(page: Page, reach: Reach, where: string): Promise<void> {
  if (!(await f6To(page, 'document'))) return;
  const at = await stop(page);
  if (at && !at.visible)
    reach.unseen.push(`context menu (${where}): F6 put the keys on a hidden ${at.role} "${at.label}"`);
  await key(page, 'Shift+F10');
  if ((await stop(page))?.region !== 'menu') return;
  await walkMenu(page, reach, `context menu (${where})`);
  await key(page, 'Escape');
}

const KEYMAPS = { write: writeKeymap, grid: gridKeymap, deck: deckKeymap, notes: writeKeymap } as const;

/** The catalog: every core command, its tier and apps. */
const CATALOG = (
  JSON.parse(readFileSync(repo('packages/protocol/src/generated/catalog.json'), 'utf8')) as {
    commands: { id: string; tier: string; apps: string[] }[];
  }
).commands;

/** Keymap key (`Mod+Shift+ArrowUp`) → the KeyboardEvent fields a Windows or Linux keyboard sends. */
function eventInit(k: string) {
  const parts = k.endsWith('++') ? [...k.slice(0, -2).split('+'), '+'] : k.split('+');
  const main = parts.pop() ?? '';
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  const named: Record<string, string> = { Esc: 'Escape', Space: ' ' };
  return {
    key: named[main] ?? (main.length === 1 ? main.toLowerCase() : main),
    ctrlKey: mods.has('mod') || mods.has('ctrl'),
    shiftKey: mods.has('shift'),
    altKey: mods.has('alt'),
    metaKey: false,
  };
}

/** Shortcuts: a command whose key resolves to it in this window is reached by that key. */
async function viaShortcuts(page: Page, app: App, reach: Reach, ids: readonly string[]): Promise<void> {
  const bindings = [...commonKeymap, ...KEYMAPS[app]].filter(
    (b) => ids.includes(b.command) && !reach.reached.has(b.command),
  );
  for (const b of bindings) {
    const got = await page.evaluate(
      ([init, scope]) => {
        const k = (window as unknown as W).__kiln;
        return (
          k?.keymap.resolve(init as never, {
            ...(k as unknown as { registry: { context: object } }).registry.context,
            scopes: scope ? [scope] : [],
          })?.command ?? null
        );
      },
      [eventInit(b.key), b.scope ?? null] as const,
    );
    if (got === b.command && !reach.reached.has(b.command))
      reach.reached.set(b.command, `shortcut ${b.key}${b.scope ? ` (${b.scope})` : ''}`);
  }
}

/** The command palette (Alt+Q): its title typed, the arrows to its row. */
async function viaPalette(page: Page, reach: Reach, ids: readonly string[]): Promise<void> {
  const titles = await page.evaluate(
    (list) => {
      const r = (window as unknown as W).__kiln?.registry;
      const all = r?.list({ visibleOnly: true }) ?? [];
      return list
        .map((id) => all.find((c) => c.id === id))
        .filter((c) => !!c && c.palette !== false && r?.getState(c.id).enabled)
        .map((c) => ({ id: c?.id ?? '', title: c?.title ?? '' }));
    },
    [...ids],
  );
  for (const { id, title } of titles) {
    if (reach.reached.has(id)) continue;
    await f6To(page, 'document');
    await key(page, 'Alt+q');
    const input = page.getByRole('combobox').or(page.locator('[cmdk-input]')).first();
    if (!(await input.isVisible().catch(() => false))) continue;
    await page.keyboard.type(title);
    await page.waitForTimeout(120);
    for (let i = 0; i < 60; i++) {
      const on = await page.evaluate(
        () =>
          document.querySelector('[cmdk-item][aria-selected="true"]')?.getAttribute('data-command') ?? null,
      );
      if (on === id) {
        const lit = await page.evaluate(() => {
          const el = document.querySelector<HTMLElement>('[cmdk-item][aria-selected="true"]');
          if (!el) return false;
          const bg = getComputedStyle(el).backgroundColor;
          return bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent';
        });
        if (!lit) reach.unseen.push(`palette ▸ ${title} (row not highlighted)`);
        reach.reached.set(id, `palette (Alt+Q) “${title}”`);
        break;
      }
      await key(page, 'ArrowDown');
    }
    await key(page, 'Escape');
  }
}

// ── Key Tips ─────────────────────────────────────────────────────────────────────────────────

/** Letters the ribbon definitions give each command of a tab (Office's), by tab id. */
function declaredTips(
  app: Exclude<App, 'notes'>,
): Map<string, { command: string | null; label?: string; tip: string }[]> {
  const out = new Map<string, { command: string | null; label?: string; tip: string }[]>();
  const spec = RIBBONS[app] as unknown as
    | { tabs: { id: string; groups: { items: unknown[] }[] }[] }
    | undefined;
  for (const tab of spec?.tabs ?? []) {
    const list: { command: string | null; label?: string; tip: string }[] = [];
    const walk = (it: unknown) => {
      const x = it as {
        kind?: string;
        items?: unknown[];
        command?: string;
        label?: string;
        keyTip?: string;
        tier?: string;
      };
      if (x.kind === 'row') x.items?.forEach(walk);
      // Items of a tier this build hides are not drawn.
      else if (x.keyTip && (x.tier ?? 'P0') === 'P0')
        list.push({ command: x.command ?? null, label: x.label, tip: x.keyTip.toUpperCase() });
    };
    for (const g of tab.groups) {
      g.items.forEach(walk);
      const l = (g as { launcher?: { command: string; keyTip?: string } }).launcher;
      if (l?.keyTip) list.push({ command: l.command, tip: l.keyTip.toUpperCase() });
    }
    out.set(tab.id, list);
  }
  return out;
}

/** Letters the ribbon definitions give that are Kiln's, not Office's (commands Office has elsewhere). */
const KILN_LETTERS: ReadonlySet<string> = new Set(['suite.chart.insert', 'deck.slides.reset_slide']);

const prefixFree = (tips: readonly string[]) =>
  tips.every((a, i) => tips.every((b, j) => i === j || !b.startsWith(a)));

interface TipTarget {
  tip: string;
  label: string;
  command: string | null;
  menu: boolean;
}

/** The Key Tip targets of a level now on screen (what the overlay badges). */
async function tipTargets(page: Page, level: string): Promise<TipTarget[]> {
  return page.evaluate((lv) => {
    const out: TipTarget[] = [];
    for (const el of document.querySelectorAll<HTMLElement>(
      `[data-keytip][data-keytip-level="${CSS.escape(lv)}"]`,
    )) {
      if (el.closest('[hidden], [aria-hidden="true"], [inert]') || !el.isConnected) continue;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const popup = el.getAttribute('aria-haspopup');
      out.push({
        tip: el.dataset.keytip ?? '',
        label: (
          el.getAttribute('aria-label') ??
          el.querySelector('.kiln-btn__label')?.textContent ??
          el.querySelector('[aria-label]')?.getAttribute('aria-label') ??
          el.textContent ??
          ''
        ).trim(),
        command:
          el.closest('[data-command]')?.getAttribute('data-command') ??
          el.querySelector('[data-command]')?.getAttribute('data-command') ??
          null,
        menu: popup === 'menu' || popup === 'true',
      });
    }
    return out;
  }, level);
}

async function badges(page: Page): Promise<string[]> {
  return page
    .locator('[data-keytip-badge]')
    .evaluateAll((els) => els.map((e) => e.getAttribute('data-keytip-badge') ?? ''));
}

async function typeTip(page: Page, tip: string): Promise<void> {
  for (const ch of tip) await key(page, ch.toLowerCase());
  await page.waitForTimeout(80);
}

async function dialogsPassAxe(page: Page, app: Exclude<App, 'notes'>): Promise<void> {
  const seen = new Map<string, string>();
  const failures: string[] = [];
  // Commands run in an earlier state (the blank document first) are not run again.
  const ran = new Set<string>();
  for (const context of CONTEXTS[app]) {
    await context.setup(page);
    const reopen = () => context.setup(page);
    const ids = await page.evaluate(
      () =>
        (window as unknown as W).__kiln?.registry
          .list({ visibleOnly: true })
          .filter((c) => !c.pending)
          .map((c) => c.id) ?? [],
    );
    // The contextual tabs of this state.
    const tabs = page.getByRole('tablist', { name: 'Ribbon tabs' }).getByRole('tab');
    for (let i = 0; i < (await tabs.count()); i++) {
      const label = (await tabs.nth(i).textContent()) ?? '';
      if (seen.has(`tab:${label}`)) continue;
      seen.set(`tab:${label}`, context.name);
      await tabs.nth(i).click();
      const v = blockingViolations(await axeScan(page, { include: ['.kiln-ribbon'] }));
      if (v.length) failures.push(`${app}/${context.name} tab ${label}:\n${formatViolations(v)}`);
    }
    for (const id of ids) {
      if (LEAVES.test(id)) continue;
      const state = await page.evaluate((i) => (window as unknown as W).__kiln?.registry.getState(i), id);
      if (!state?.enabled || ran.has(id)) continue;
      ran.add(id);
      const url = page.url();
      await page
        .evaluate((i) => (window as unknown as W).__kiln?.registry.execute(i).catch(() => undefined), id)
        .catch(() => undefined);
      await page.waitForTimeout(300);
      if (page.url() !== url) {
        failures.push(`${app}/${context.name} ${id} left the window for ${page.url()}`);
        await reopen();
        continue;
      }
      for (const s of await surfaces(page).catch(() => [])) {
        const key = `${s.kind}:${s.name}`;
        if (seen.has(key)) continue;
        seen.set(key, `${context.name}/${id}`);
        // In the light theme, then in high contrast.
        const v = await axeFindings(page, s.selector);
        await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'hc'));
        v.push(...(await axeFindings(page, s.selector)).map((f) => `high contrast: ${f}`));
        v.push(...(await enhancedContrast(page, s.selector)).map((f) => `high contrast: ${f}`));
        await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'));
        if (v.length) {
          failures.push(`${app}/${context.name} ${id} → ${key}:\n${v.join('\n')}`);
          await page.screenshot({
            path: repo(`target/scratch/fail-${app}-${failures.length}.png`),
          });
        }
      }
      await closeAll(page, reopen);
    }
  }
  const found = [...seen.keys()];
  writeFileSync(shot(`${app}-surfaces.json`), JSON.stringify(Object.fromEntries(seen), null, 1));
  expect(failures, failures.join('\n\n')).toEqual([]);
  expect(found.filter((k) => k.startsWith('dialog:')).length).toBeGreaterThan(20);
  expect(found.some((k) => k.startsWith('pane:'))).toBe(true);
}

async function keyboardReach(page: Page, app: Exclude<App, 'notes'>): Promise<void> {
  // The walk, the shortcuts and the ribbon pressed, then the scenarios: one long test per app.
  test.setTimeout(2_700_000);
  const reach: Reach = { reached: new Map(), unseen: [] };
  const mine = CATALOG.filter((c) => c.apps.includes(cap(app)));
  const shipped = mine.filter((c) => c.tier === 'P0').map((c) => c.id);
  for (const [i, context] of (process.env.KILN_A11Y_SCENARIOS === '1' ? [] : CONTEXTS[app]).entries()) {
    // A step that leaves the window (a command that navigates) costs the rest of that step only.
    const safe = async (step: () => Promise<void>) => {
      try {
        await step();
      } catch (e) {
        if (!String(e).includes('Execution context was destroyed') && !String(e).includes('navigat')) throw e;
        await context.setup(page);
      }
    };
    await context.setup(page);
    await safe(() =>
      walkRibbon(
        page,
        reach,
        i === 0 ? undefined : (t) => !['Home', 'Insert', 'View', 'Review', 'Help'].includes(t),
      ).then(() => undefined),
    );
    if (i === 0) {
      await safe(() => walkRegion(page, reach, 'titlebar'));
      await safe(() => walkRegion(page, reach, 'statusbar'));
    }
    // The File menu, on a blank document and on one opened from a file (recent files listed).
    if (i <= 1) await safe(() => walkBackstage(page, reach));
    await safe(() => walkContextMenu(page, reach, context.name));
    await safe(() => viaShortcuts(page, app, reach, shipped));
    await safe(() =>
      viaPalette(
        page,
        reach,
        // A command the window sends through its own command (STEP_OF) is reached with that one.
        [...shipped, ...shipped.flatMap((id) => (STEP_OF[id] ? [STEP_OF[id]] : []))].filter(
          (id) => !reach.reached.has(id),
        ),
      ),
    );
    await safe(() =>
      viaSurfaces(
        page,
        reach,
        i === 0 ? SURFACE_OPENERS[app] : (context.openers ?? CONTEXT_OPENERS[app]),
        () => context.setup(page),
      ),
    );
  }
  const only = process.env.KILN_A11Y_SCENARIOS === '1';
  await viaDocumentKeys(page, app, reach);
  await pressShortcuts(page, app, reach);
  await pressRibbon(page, app, reach);
  const failedScenarios = await viaScenarios(page, app, reach);
  // Document Recovery lists the documents of the pages before (each fresh page above left one):
  // F6 to the pane, Tab to Delete, Enter.
  if (
    await page
      .locator('aside[aria-label="Document Recovery"]')
      .isVisible()
      .catch(() => false)
  ) {
    for (let f = 0; f < 6; f++) {
      await key(page, 'F6');
      if (
        await page.evaluate(() => !!document.activeElement?.closest('aside[aria-label="Document Recovery"]'))
      )
        break;
    }
    for (let t = 0; t < 30; t++) {
      const label = await page.evaluate(() => document.activeElement?.getAttribute('aria-label') ?? '');
      if (label.startsWith('Delete ')) {
        const s = await stop(page);
        if (s) note(reach, s, `pane Document Recovery ▸ ${label}`);
        await viaRunning(page, reach, `pane Document Recovery ▸ ${label}`, () => key(page, 'Enter'));
        break;
      }
      await key(page, 'Tab');
    }
  }
  for (const [id, why] of PLUMBING)
    if (!reach.reached.has(id)) reach.reached.set(id, `not a user command: ${why}`);
  for (const [id, by] of Object.entries(STEP_OF))
    if (!reach.reached.has(id) && reach.reached.has(by))
      reach.reached.set(id, `a step of ${by}: ${reach.reached.get(by)}`);
  for (const [id, keys] of Object.entries(POINTER))
    if (!reach.reached.has(id) && reach.reached.has(keys))
      reach.reached.set(id, `pointer gesture; its keyboard command ${keys}: ${reach.reached.get(keys)}`);
  const missing = shipped.filter((id) => !reach.reached.has(id));
  writeFileSync(
    shot(`${app}-keyboard-reach.json`),
    JSON.stringify(
      {
        shipped: shipped.length,
        excluded: mine.length - shipped.length,
        reached: Object.fromEntries(reach.reached),
        missing,
        elsewhere: Object.fromEntries(
          missing.flatMap((id) => {
            const why = REACHED_ELSEWHERE[app].get(id);
            return why ? [[id, why]] : [];
          }),
        ),
        unseen: reach.unseen,
        failedScenarios,
      },
      null,
      1,
    ),
  );
  if (only) return;
  expect(failedScenarios, failedScenarios.join('\n')).toEqual([]);
  expect(reach.unseen, reach.unseen.join('\n')).toEqual([]);
  // Every command of the app is reached here, or (a shared one) in the app that reaches it.
  const unexpected = missing.filter((id) => !REACHED_ELSEWHERE[app].has(id));
  expect(unexpected, unexpected.join('\n')).toEqual([]);
}

/** Ids of everything sent to the core or run by the registry so far. */
async function ranIds(page: Page): Promise<string[]> {
  await recordCore(page);
  const [ids, sent] = await page.evaluate(() => {
    const k = (window as unknown as W).__kiln;
    return [
      [...(k?.log().map((e) => e.id) ?? []), ...(k?.executed.map((e) => e.id) ?? [])],
      (window as unknown as { __a11ySent?: string[] }).__a11ySent ?? [],
    ] as const;
  });
  const named = sent.map((s) => {
    const [app, variant, sub] = s.split(':');
    const map = VARIANT_IDS[app ?? ''] ?? {};
    return map[variant ?? ''] ?? map[sub ?? ''] ?? `${app}.${variant}`;
  });
  return [...ids, ...named];
}

/** Records the commands a keyboard action ran (core log and registry) as reached by `path`. */
async function viaRunning(
  page: Page,
  reach: Reach,
  path: string,
  action: () => Promise<void>,
): Promise<void> {
  const before = new Map<string, number>();
  for (const id of await ranIds(page)) before.set(id, (before.get(id) ?? 0) + 1);
  await action();
  await page.waitForTimeout(150);
  for (const id of await ranIds(page)) {
    const left = before.get(id) ?? 0;
    if (left > 0) before.set(id, left - 1);
    else if (!reach.reached.has(id)) reach.reached.set(id, path);
  }
}

/** Opens a command with the palette (Alt+Q, its title, Enter). False when it is not listed there. */
async function openByPalette(page: Page, id: string): Promise<boolean> {
  const title = await page.evaluate(
    (i) =>
      (window as unknown as W).__kiln?.registry
        .list({ visibleOnly: true })
        .find((c) => c.id === i && c.palette !== false)?.title ?? null,
    id,
  );
  if (!title) return false;
  // Alt+Q works from anywhere in the window; with no focus at all, F6 to the document first.
  if (await page.evaluate(() => !document.activeElement || document.activeElement === document.body))
    await f6To(page, 'document');
  await key(page, 'Alt+q');
  await page.keyboard.type(title);
  await page.waitForTimeout(120);
  for (let i = 0; i < 60; i++) {
    const on = await page.evaluate(
      () => document.querySelector('[cmdk-item][aria-selected="true"]')?.getAttribute('data-command') ?? null,
    );
    if (on === id) {
      await key(page, 'Enter');
      await page.waitForTimeout(300);
      return true;
    }
    await key(page, 'ArrowDown');
  }
  await key(page, 'Escape');
  return false;
}

/**
 * The dialogs and panes of an app. Each opener is itself reached from the keyboard above; once a
 * dialog is open, only keys: Tab through every control (each must show its focus; empty text
 * fields get a value), then Enter on the default button. A pane: each of its buttons with Tab and
 * Enter in turn. The commands these run are reached by the keyboard.
 */
async function viaSurfaces(
  page: Page,
  reach: Reach,
  openers: readonly string[],
  reopen: () => Promise<void>,
): Promise<void> {
  const open = async (opener: string) => {
    if (await openByPalette(page, opener)) return true;
    const st = await page.evaluate((i) => (window as unknown as W).__kiln?.registry.getState(i), opener);
    if (!st?.enabled || !reach.reached.has(opener)) return false;
    await page.evaluate((i) => (window as unknown as W).__kiln?.registry.execute(i), opener);
    await page.waitForTimeout(300);
    return true;
  };
  for (const opener of openers) {
    // What opening it sends (a pane that loads its data, a dialog that reads the selection).
    let opened = false;
    await viaRunning(page, reach, `palette ▸ ${opener}`, async () => {
      opened = await open(opener);
    });
    if (!opened) continue;
    const kind = await page.evaluate(() =>
      document.querySelector('[role="dialog"]:not(.kiln-popover):not(.kiln-backstage), [role="alertdialog"]')
        ? 'dialog'
        : 'pane',
    );
    if (kind === 'dialog') {
      const seen = new Set<string>();
      let primaryAt = -1;
      for (let i = 0; i < 150; i++) {
        const s = await stop(page);
        if (!s || seen.has(s.key) || s.region !== 'dialog') break;
        seen.add(s.key);
        note(reach, s, `dialog via ${opener} ▸ ${s.label}`);
        const here = await page.evaluate(() => {
          const a = document.activeElement as HTMLInputElement | null;
          const primary = !!a?.matches('.kiln-btn--primary:not([disabled]):not([aria-disabled="true"])');
          const empty = !!a?.matches('input[type="text"], input:not([type]), textarea') && a.value === '';
          return { primary, empty };
        });
        if (here.empty) await page.keyboard.type('Kiln');
        if (here.primary && primaryAt < 0) primaryAt = seen.size;
        await key(page, 'Tab');
      }
      if (primaryAt > 0) {
        // Round again to the default button, then Enter.
        for (let i = 0; i < 150; i++) {
          if (await page.evaluate(() => !!document.activeElement?.matches('.kiln-btn--primary'))) break;
          await key(page, 'Tab');
        }
        await viaRunning(page, reach, `dialog via ${opener} ▸ Enter on its default button`, () =>
          key(page, 'Enter'),
        );
      }
      await closeAll(page, reopen);
      continue;
    }
    // A pane: every button, one after another (the pane opened again when one closes it).
    // The pane just opened (Document Recovery, open after a reload, is not it).
    const pane = '.kiln-appframe__body aside:not([aria-label="Document Recovery"])';
    const sel = `${pane} button:not([disabled]), ${pane} [role="option"], ${pane} [role="treeitem"]`;
    const labels = await page.evaluate(
      (q) =>
        [...document.querySelectorAll<HTMLElement>(q)].map((b) =>
          (b.getAttribute('aria-label') ?? b.textContent ?? '').trim(),
        ),
      sel,
    );
    for (const label of [...new Set(labels)].slice(0, 30)) {
      if (!label || /^(Close|Resize|Task pane options)/.test(label)) continue;
      if (!(await page.locator('.kiln-appframe__body aside, .kiln-taskpane').count())) await open(opener);
      // F6 into the pane, Tab to the button.
      let found = false;
      for (let f = 0; f < 6 && !found; f++) {
        await key(page, 'F6');
        const inPane = await page.evaluate((p) => !!document.activeElement?.closest(p), pane);
        if (!inPane) continue;
        for (let t = 0; t < 60; t++) {
          const on = await page.evaluate(
            (l) =>
              (
                document.activeElement?.getAttribute('aria-label') ??
                document.activeElement?.textContent ??
                ''
              ).trim() === l,
            label,
          );
          if (on) {
            found = true;
            break;
          }
          await key(page, 'Tab');
          if (!(await page.evaluate((p) => !!document.activeElement?.closest(p), pane))) break;
        }
      }
      if (!found) continue;
      const s = await stop(page);
      if (s) note(reach, s, `pane via ${opener} ▸ ${label}`);
      await viaRunning(page, reach, `pane via ${opener} ▸ ${label}`, () => key(page, 'Enter'));
      for (
        let i = 0;
        i < 3 && (await page.locator('[role="dialog"], [role="alertdialog"], [role="menu"]').count());
        i++
      )
        await key(page, 'Escape');
    }
    await closeAll(page, reopen);
  }
}

/** The document's own keys (typing, the caret and editing keys, an IME), each in a fresh document. */
const DOCUMENT_KEYS: Readonly<Record<Exclude<App, 'notes'>, readonly (readonly string[])[]>> = {
  write: [
    [
      'type:Kiln text',
      'Backspace',
      'Home',
      'Delete',
      'End',
      'Control+Backspace',
      'Home',
      'Control+Delete',
      'Tab',
      'Enter',
    ],
    ['type:one two', 'ArrowLeft', 'Shift+ArrowLeft', 'Control+ArrowLeft', 'Home', 'End', 'Control+a'],
    ['ime:かな', 'ime-commit:かな', 'ime:x', 'Escape'],
    // The Text View: its caret and selection go to the model with the arrow keys.
    [
      'palette:ui.view.text_view',
      'type:Kiln view',
      'Shift+ArrowLeft',
      'ArrowLeft',
      'palette:ui.view.text_view',
    ],
  ],
  grid: [
    [
      'type:5',
      'Enter',
      'ArrowUp',
      'F2',
      'type:0',
      'Escape',
      'Delete',
      'Tab',
      'Shift+Tab',
      'PageDown',
      'PageUp',
      'Home',
      'Control+Home',
    ],
    ['type:=SU', 'ArrowDown', 'Tab', 'type:A1', 'Escape'],
    ['type:=', 'ArrowRight', 'Escape', 'Alt+ArrowDown', 'Escape'],
    [
      'type:1',
      'Enter',
      'type:2',
      'Enter',
      'Shift+ArrowUp',
      'Shift+ArrowUp',
      'Control+d',
      'Control+Space',
      'Shift+Space',
    ],
  ],
  deck: [
    [
      'Tab',
      'Tab',
      'ArrowRight',
      'F2',
      'type:Kiln',
      'Enter',
      'type:b',
      'Backspace',
      'Shift+Enter',
      'Control+Backspace',
      'ArrowLeft',
      'Escape',
    ],
    ['Tab', 'Delete', 'Control+z', 'PageDown', 'PageUp'],
    ['F2', 'ime:かな', 'ime-commit:かな'],
    // The thumbnail rail (Shift+Tab from the slide): select, new slide, move, delete.
    ['Shift+Tab', 'Enter', 'ArrowUp', 'Shift+ArrowDown', 'Control+ArrowDown', 'Control+ArrowUp', 'Delete'],
  ],
};

async function viaDocumentKeys(page: Page, app: Exclude<App, 'notes'>, reach: Reach): Promise<void> {
  const cdp = await page.context().newCDPSession(page);
  for (const seq of DOCUMENT_KEYS[app]) {
    await openApp(page, app);
    // The document's caret (F6 brings the keys there: the focus order test).
    await page
      .locator('[data-testid="kiln-input-proxy"], [data-kiln-grid-proxy], [data-kiln-deck-proxy]')
      .first()
      .focus();
    for (const step of seq) {
      await viaRunning(page, reach, `document keys: ${seq.join(', ')}`, async () => {
        if (step.startsWith('palette:')) await openByPalette(page, step.slice(8));
        else if (step.startsWith('type:')) await page.keyboard.type(step.slice(5));
        else if (step.startsWith('ime-commit:')) await cdp.send('Input.insertText', { text: step.slice(11) });
        else if (step.startsWith('ime:'))
          await cdp.send('Input.imeSetComposition', {
            text: step.slice(4),
            selectionStart: 1,
            selectionEnd: 1,
          });
        else await key(page, step);
      });
    }
  }
  await cdp.detach();
}

// ── Shortcuts pressed ────────────────────────────────────────────────────────────────────────

/** The document each app's shortcuts are pressed in: some text or cells, selected. */
const SHORTCUT_SETUP: Readonly<Record<Exclude<App, 'notes'>, (page: Page) => Promise<void>>> = {
  write: async (page) => {
    await openApp(page, 'write');
    await inDocument(page);
    await page.keyboard.type('Kiln shortcut text');
    await key(page, 'Shift+Home');
  },
  grid: async (page) => {
    await openApp(page, 'grid');
    await inDocument(page);
    await page.keyboard.type('1');
    await key(page, 'Enter');
    await page.keyboard.type('2');
    await key(page, 'Enter');
    await key(page, 'Shift+ArrowUp');
    await key(page, 'Shift+ArrowUp');
  },
  deck: async (page) => {
    await openApp(page, 'deck');
    await inDocument(page);
    await key(page, 'Tab');
  },
};

const SHORTCUT_SETUP_WRITE = (page: Page) => SHORTCUT_SETUP.write(page);

/** Shortcuts not pressed: they leave the window or wait on the system (print, files, windows). */
const NOT_PRESSED = /print|close|\.open|open_window|new_window|save|exit|quit|full_screen|help/;

/** Playwright's name of a key binding (`Mod+Shift+C` → `Control+Shift+C`). */
function pressName(k: string): string {
  const parts = k.endsWith('++') ? [...k.slice(0, -2).split('+'), '+'] : k.split('+');
  const main = parts.pop() ?? '';
  const mods = parts.map(
    (p) => ({ mod: 'Control', ctrl: 'Control', shift: 'Shift', alt: 'Alt' })[p.toLowerCase()] ?? p,
  );
  const named: Record<string, string> = { Esc: 'Escape', Space: 'Space' };
  return [...mods, named[main] ?? main].join('+');
}

/**
 * Each of the app's shortcuts pressed in a document with a selection: the commands the keys
 * send (the core's own as well as the chrome's) are reached by them.
 */
async function pressShortcuts(page: Page, app: Exclude<App, 'notes'>, reach: Reach): Promise<void> {
  const bindings = [...commonKeymap, ...KEYMAPS[app]].filter((b) => !NOT_PRESSED.test(b.command));
  await SHORTCUT_SETUP[app](page);
  for (const b of bindings) {
    if (!page.url().includes(`#/${app}`)) await SHORTCUT_SETUP[app](page);
    try {
      await viaRunning(page, reach, `shortcut ${b.key} pressed`, async () => {
        await page.keyboard.press(pressName(b.key));
        await page.waitForTimeout(120);
      });
      for (
        let i = 0;
        i < 3 && (await page.locator('[role="dialog"], [role="alertdialog"], [role="menu"]').count());
        i++
      )
        await key(page, 'Escape');
      if (await page.getByTestId('deck-show').count()) await key(page, 'Escape');
      if (!(await page.evaluate(() => document.activeElement && document.activeElement !== document.body)))
        await inDocument(page).catch(() => undefined);
    } catch {
      await SHORTCUT_SETUP[app](page);
    }
  }
}

// ── Ribbon controls pressed ─────────────────────────────────────────────────────────────────

/** Ribbon commands not pressed: they leave the window, print, or start something long. */
const NOT_RIBBON_PRESSED =
  /^suite\.file\.|print|close|open_window|new_window|share|collab|show\.(from|start|in_tab|presenter|rehearse)|help|feedback|account|macros|screenshot|record|dictate|exit/;

/**
 * The states each app's ribbon is pressed in, each with the contextual tabs it shows (a table, a
 * picture, a chart).
 */
const RIBBON_STATES: Readonly<Record<Exclude<App, 'notes'>, readonly ((page: Page) => Promise<void>)[]>> = {
  write: [
    SHORTCUT_SETUP_WRITE,
    ...[1, 2, 3].map((i) => async (page: Page) => CONTEXTS.write[i]?.setup(page)),
  ],
  grid: [1, 2, 3, 4].map((i) => async (page: Page) => CONTEXTS.grid[i]?.setup(page)),
  deck: [
    (page: Page) => SHORTCUT_SETUP.deck(page),
    ...[1, 2, 3, 4, 5].map((i) => async (page: Page) => CONTEXTS.deck[i]?.setup(page)),
  ],
};

/**
 * Every ribbon control run by its Key Tips (Alt, the tab's letters, its own), once, in the first
 * state that shows its tab: the commands it sends to the core are reached by the keyboard.
 */
async function pressRibbon(page: Page, app: Exclude<App, 'notes'>, reach: Reach): Promise<void> {
  const audit = JSON.parse(readFileSync(repo('packages/commands/src/keytips-audit.json'), 'utf8')) as Record<
    string,
    KeyTipsAuditTab[]
  >;
  const done = new Set<string>();
  for (const state of RIBBON_STATES[app]) {
    await state(page).catch(() => undefined);
    for (const tab of audit[app] ?? []) {
      const shown = async () =>
        page.evaluate(
          (tip) => !!document.querySelector(`.kiln-ribbon__tab[data-keytip="${tip}"]:not([hidden])`),
          tab.tip,
        );
      if (!(await shown())) continue;
      for (const item of tab.items) {
        const id = item.command;
        if (!id || done.has(`${tab.label}:${id}`) || NOT_RIBBON_PRESSED.test(id)) continue;
        done.add(`${tab.label}:${id}`);
        if (!page.url().includes(`#/${app}`) || !(await shown())) {
          await state(page).catch(() => undefined);
          if (!(await shown())) continue;
        }
        try {
          await viaRunning(
            page,
            reach,
            `ribbon ${tab.label} ▸ ${item.label} pressed (Alt, ${tab.tip}, ${item.tip})`,
            async () => {
              await key(page, 'Alt');
              await page.waitForTimeout(80);
              await typeTip(page, tab.tip);
              await typeTip(page, item.tip);
              await page.waitForTimeout(150);
            },
          );
          for (
            let i = 0;
            i < 4 &&
            (await page
              .locator(
                '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-testid="keytips"]',
              )
              .count());
            i++
          )
            await key(page, 'Escape');
          if (await page.getByTestId('deck-show').count()) await key(page, 'Escape');
        } catch (e) {
          if (process.env.KILN_A11Y_DEBUG) console.log('RIBBON-ERR', tab.label, id, String(e).slice(0, 160));
          await state(page).catch(() => undefined);
        }
      }
    }
  }
  if (process.env.KILN_A11Y_DEBUG) console.log('RIBBON-DONE', done.size, [...done].join(' '));
}

// ── Scenarios: document states the walk above does not make, then keys only ────────────────────

/**
 * One step of a scenario, from the keyboard: `key:Control+f`, `type:text`, `palette:<command>`
 * (Alt+Q, its title, Enter), `tab:<pattern>` / `stab:<pattern>` (Tab or Shift+Tab until the focus
 * is on a control whose name matches; each stop must show its focus), `arrow:<Key>:<pattern>`
 * (the arrow key until the name matches), `f6:<region>`, `wait:<ms>`. A function is a setup
 * step (state the keys then act on).
 */
type Step = string | ((page: Page) => Promise<void>);

interface Scenario {
  name: string;
  setup: (page: Page) => Promise<void>;
  steps: readonly Step[];
  /** After the steps, whatever happened (a server the setup started). */
  teardown?: () => Promise<void>;
}

async function focusName(page: Page): Promise<string> {
  return page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (!a || a === document.body) return '';
    const by = a.getAttribute('aria-labelledby');
    const labelled = by
      ? by
          .split(/\s+/)
          .map((i) => document.getElementById(i)?.textContent ?? '')
          .join(' ')
      : '';
    const label = (a as HTMLInputElement).labels?.[0]?.textContent ?? '';
    return (
      a.getAttribute('aria-label') ??
      (labelled || label || a.getAttribute('title') || a.getAttribute('placeholder')) ??
      a.textContent ??
      ''
    )
      .replace(/\s+/g, ' ')
      .trim();
  });
}

/** Moves the focus with `move` until its name matches `pattern`; each stop is checked. */
async function moveTo(page: Page, reach: Reach, path: string, move: string, pattern: string): Promise<void> {
  const re = new RegExp(pattern, 'i');
  const seen = new Set<string>();
  for (let i = 0; i < 120; i++) {
    const s = await stop(page);
    if (s) note(reach, s, path);
    const name = await focusName(page);
    if (re.test(name)) return;
    const k = s?.key ?? `none${i}`;
    if (seen.has(k) && i > 2) break;
    seen.add(k);
    // The arrows move the caret in a ribbon's text box until it is at the end.
    if (s?.textEntry && s.region === 'ribbon' && move === 'ArrowRight') await key(page, 'End');
    await key(page, move);
  }
  throw new Error(`${path}: no control named ${pattern} (focus on "${await focusName(page)}")`);
}

/** The Key Tips that run a ribbon command (`[tab, item]`), from the audit. */
function tipsOf(app: Exclude<App, 'notes'>, command: string): [string, string] | null {
  const audit = JSON.parse(readFileSync(repo('packages/commands/src/keytips-audit.json'), 'utf8')) as Record<
    string,
    KeyTipsAuditTab[]
  >;
  for (const tab of audit[app] ?? []) {
    const item = tab.items.find((i) => i.command === command);
    if (item) return [tab.tip, item.tip];
  }
  return null;
}

async function runStep(
  page: Page,
  reach: Reach,
  path: string,
  step: Step,
  app?: Exclude<App, 'notes'>,
): Promise<void> {
  if (typeof step === 'function') return step(page);
  const [kind, ...rest] = step.split(':');
  const arg = rest.join(':');
  switch (kind) {
    case 'key':
      return key(page, arg);
    case 'type':
      return page.keyboard.type(arg);
    case 'palette':
      expect(await openByPalette(page, arg), `${path}: ${arg} is in the palette`).toBe(true);
      return;
    case 'tab':
      return moveTo(page, reach, path, 'Tab', arg);
    case 'stab':
      return moveTo(page, reach, path, 'Shift+Tab', arg);
    case 'arrow': {
      const [k, ...pat] = arg.split(':');
      return moveTo(page, reach, path, k as string, pat.join(':'));
    }
    case 'f6':
      expect(await f6To(page, arg), `${path}: F6 reaches the ${arg}`).toBe(true);
      return;
    case 'wait':
      return page.waitForTimeout(Number(arg));
    case 'cmd': {
      // A ribbon control by its Key Tips (`cmd:<command>`): Alt, its tab's letters, its own.
      const tips = app ? tipsOf(app, arg) : null;
      if (!tips) throw new Error(`${path}: no Key Tips for ${arg}`);
      await key(page, 'Alt');
      await page.waitForTimeout(150);
      for (const level of tips) await typeTip(page, level);
      await page.waitForTimeout(250);
      return;
    }
    case 'ribbon': {
      // `ribbon:<tab label>><control name>`: Alt, the tab's Key Tip as drawn, the control's.
      const [tabLabel, itemName] = arg.split('>');
      await key(page, 'Alt');
      await page.waitForTimeout(150);
      const tabTip = await page.evaluate(
        (l) =>
          [...document.querySelectorAll<HTMLElement>('.kiln-ribbon__tab[data-keytip]')].find(
            (t) => t.textContent?.trim() === l,
          )?.dataset.keytip ?? null,
        tabLabel ?? '',
      );
      if (!tabTip) throw new Error(`${path}: no ribbon tab ${tabLabel}`);
      await typeTip(page, tabTip);
      await page.waitForTimeout(250);
      const found = await page.evaluate((n) => {
        const re = new RegExp(n, 'i');
        const names: string[] = [];
        for (const el of document.querySelectorAll<HTMLElement>(
          '[data-keytip]:not([data-keytip-level="root"])',
        )) {
          const name = (
            el.getAttribute('aria-label') ??
            (el.textContent?.trim() || el.querySelector('[aria-label]')?.getAttribute('aria-label')) ??
            ''
          ).trim();
          names.push(name);
          if (re.test(name) && el.getBoundingClientRect().width > 0)
            return { tip: el.dataset.keytip ?? null, names };
        }
        return { tip: null, names };
      }, itemName ?? '');
      const itemTip = found.tip;
      if (!itemTip) throw new Error(`${path}: no ${itemName} on ${tabLabel} (${found.names.join(' | ')})`);
      await typeTip(page, itemTip);
      await page.waitForTimeout(250);
      return;
    }
    case 'tips': {
      // Key Tips: Alt, then each level's letters (`tips:H,FP`).
      await key(page, 'Alt');
      await page.waitForTimeout(150);
      for (const level of arg.split(',')) await typeTip(page, level);
      await page.waitForTimeout(250);
      return;
    }
    case 'pane': {
      // F6 until the keys are in a task pane (`pane:<its name>`, or any pane).
      const went: string[] = [];
      for (let i = 0; i < 8; i++) {
        await key(page, 'F6');
        const at = await page.evaluate((name) => {
          const a = document.activeElement;
          const p = a?.closest<HTMLElement>(
            '.kiln-appframe__body aside, .kiln-taskpane, [role="complementary"], [data-kiln-region="pane"]',
          );
          return {
            inPane: !!p && (!name || (p.getAttribute('aria-label') ?? '').includes(name)),
            where: `${a?.tagName}:${p?.getAttribute('aria-label') ?? a?.getAttribute('aria-label') ?? ''}`,
          };
        }, arg);
        if (at.inPane) return;
        went.push(at.where);
      }
      throw new Error(`${path}: F6 does not reach the ${arg || 'task'} pane (${went.join(' > ')})`);
    }
    case 'menu': {
      // In an open menu: the arrows to the item, then Enter.
      await moveTo(page, reach, path, 'ArrowDown', arg);
      await key(page, 'Enter');
      return;
    }
    case 'press': {
      // Tab to the control and press Enter (`press:<pattern>`).
      await moveTo(page, reach, path, 'Tab', arg);
      await key(page, 'Enter');
      await page.waitForTimeout(200);
      return;
    }
    default:
      throw new Error(`unknown step ${step}`);
  }
}

async function viaScenarios(page: Page, app: Exclude<App, 'notes'>, reach: Reach): Promise<string[]> {
  const failed: string[] = [];
  const only = process.env.KILN_A11Y_ONLY;
  for (const sc of SCENARIOS[app]) {
    if (only && !sc.name.includes(only)) continue;
    try {
      await sc.setup(page);
      await recordCore(page);
      for (const step of sc.steps)
        await viaRunning(page, reach, `scenario ${sc.name}`, () =>
          runStep(page, reach, `scenario ${sc.name}`, step, app),
        );
    } catch (e) {
      failed.push(`${sc.name}: ${String(e).split('\n')[0]}`);
    } finally {
      await sc.teardown?.().catch(() => undefined);
    }
  }
  return failed;
}

/** Keys in the document (F6 brings them there: the focus order test). */
async function inDocument(page: Page): Promise<void> {
  // A slide show still loading covers the document: the walk ends it at the next key.
  await page
    .locator('[data-testid="kiln-input-proxy"], [data-kiln-grid-proxy], [data-kiln-deck-proxy]')
    .first()
    .focus({ timeout: 5_000 });
}

/** A document with `text` typed into it, the keys in it. */
const typed = (app: Exclude<App, 'notes'>, text: string, fixture?: string) => async (page: Page) => {
  if (fixture) await openFixture(page, app, fixture);
  else await openApp(page, app);
  await inDocument(page);
  await page.waitForTimeout(300);
  if (text) await page.keyboard.type(text, { delay: 20 });
  await page.waitForTimeout(300);
};

/** A blank sheet with rows of values typed into it (Tab between cells, Enter after a row). */
const sheet = (rows: readonly (readonly string[])[]) => async (page: Page) => {
  await typed('grid', '')(page);
  for (const row of rows) {
    for (const [i, v] of row.entries()) {
      if (i > 0) await key(page, 'Tab');
      await page.keyboard.type(v, { delay: 15 });
    }
    await key(page, 'Enter');
  }
  await page.waitForTimeout(200);
};

const PICTURE = 'fixtures/golden/deck/theme-colors.png';

/** The next file chooser the keys open gets a picture (the system's dialog, out of the page). */
const choosePicture: Step = async (page) => {
  page.once('filechooser', (fc) => void fc.setFiles(repo(PICTURE)).catch(() => undefined));
};

/**
 * Sharing through a collaboration server started for the test: Share, the link opened as
 * another person (Open Shared Document), Stop sharing.
 */
function collabScenario(app: 'write' | 'deck'): Scenario {
  let server: CollabServer | null = null;
  let link = '';
  return {
    name: 'Share through a collaboration server, open the link, stop sharing',
    setup: async (page) => {
      server = await startServer();
      await typed(app, app === 'write' ? 'Shared words' : '')(page);
    },
    steps: [
      'palette:suite.file.share',
      'tab:^Your name',
      'type:Kiln Tester',
      'tab:^Collaboration server',
      async (page) => page.keyboard.type(server?.url ?? ''),
      'tab:^Access token',
      `type:${TOKEN}`,
      'press:^Share$',
      async (page) => {
        const input = page.getByTestId('share-link');
        await expect(input).toHaveValue(/#\/(write|deck)\?doc=[a-z]/, { timeout: 30_000 });
        link = await input.inputValue();
      },
      'press:^Done$',
      async (page) => {
        await page.goto('about:blank');
        await page.goto(link);
        await expect(page.getByTestId('join-dialog')).toBeVisible({ timeout: 60_000 });
      },
      'tab:^Your name',
      'type:Guest',
      'press:^Open$',
      async (page) => {
        await expect(page).toHaveURL(/#\/(write|deck)\?doc=\d+$/, { timeout: 60_000 });
        await page.waitForTimeout(800);
      },
      'palette:suite.file.share',
      'press:^Stop sharing$',
      'wait:600',
    ],
    teardown: async () => {
      await server?.stop();
      server = null;
    },
  };
}

const SCENARIOS: Readonly<Record<Exclude<App, 'notes'>, readonly Scenario[]>> = {
  write: [
    {
      name: 'Find and Replace: Find Next, Replace, Replace All',
      setup: typed('write', 'Kiln fire kiln ash kiln.'),
      steps: [
        'key:Control+Home',
        'key:Control+h',
        'type:kiln',
        'tab:^Replace with',
        'type:oven',
        'press:^Find Next$',
        'stab:^Replace$',
        'key:Enter',
        'wait:200',
        'press:^Replace All$',
        'key:Escape',
      ],
    },
    {
      name: 'comments: new, reply, edit, select, delete',
      setup: typed('write', 'A sentence to comment on.'),
      steps: [
        'key:Shift+Home',
        'key:Control+Alt+m',
        'wait:300',
        'type:First comment',
        'key:Control+Enter',
        'wait:400',
        'palette:ui.write.comments_pane',
        'pane:Comments',
        'press:^Reply$',
        'type:A reply',
        'key:Control+Enter',
        'wait:400',
        'pane:Comments',
        'tab:^More actions',
        'key:Enter',
        'menu:^Edit comment$',
        'type: edited',
        'key:Control+Enter',
        'wait:400',
        'pane:Comments',
        'tab:^More actions',
        'key:Enter',
        'menu:^Delete thread$',
        'wait:300',
      ],
    },
    {
      name: 'Format Painter: armed from the ribbon, Esc cancels',
      setup: typed('write', 'Painted words'),
      steps: ['key:Shift+Home', 'tips:H,FP', 'wait:200', 'key:Escape', 'wait:200'],
    },
    {
      name: 'Options: user name, spelling as you type, AutoFormat, AutoCorrect entries',
      setup: typed('write', ''),
      steps: [
        'palette:ui.options.open',
        'press:^General$',
        'tab:^User name',
        'key:Control+a',
        'type:Kiln Tester',
        'press:^Proofing$',
        'tab:^Check spelling as you type',
        'key:Space',
        'key:Space',
        'tab:^Replace text as you type',
        'key:Space',
        'key:Space',
        'tab:^Replace$',
        'type:kln',
        'tab:^With$',
        'type:kiln',
        'key:Enter',
        'wait:200',
        'key:Escape',
      ],
    },
    {
      name: 'Spelling pane: Ignore Once, Ignore All, Add to Dictionary',
      setup: typed('write', 'Teh kiln hass a fier zorpq blarvle. '),
      steps: [
        'wait:500',
        'key:F7',
        'wait:600',
        'pane:Editor',
        'press:^Ignore Once$',
        'pane:Editor',
        'press:^Ignore All$',
        'pane:Editor',
        'press:^Add to Dictionary$',
      ],
    },
    {
      name: 'Insert Hyperlink (Ctrl+K)',
      setup: typed('write', 'Kiln site'),
      steps: [
        'key:Shift+Home',
        'key:Control+k',
        'tab:^Address',
        'key:Control+a',
        'type:https://example.com',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'Symbol dialog: a symbol font character inserted',
      setup: typed('write', ''),
      steps: [
        'palette:suite.insert.symbol',
        'tab:^Font',
        'key:ArrowDown',
        'wait:300',
        'arrow:ArrowDown:^(Symbol|Wingdings)',
        'key:Enter',
        'wait:300',
        'press:^Insert ',
        'key:Escape',
      ],
    },
    {
      name: 'Icons inserted as a picture, then Change Picture',
      setup: typed('write', ''),
      steps: [
        'palette:suite.insert.icons',
        'wait:800',
        'type:star',
        'wait:300',
        'tab:^Star',
        'key:Enter',
        'press:^Insert$',
        'wait:800',
        choosePicture,
        'palette:ui.write.change_picture',
        'wait:800',
      ],
    },
    {
      name: 'Crop dialog on a picture, Align on a floating one',
      setup: async (page) => {
        await openFixture(page, 'write', 'fixtures/docx/features/images-anchored.docx');
        const pic = (await treeNodes(page, 'write', 'A11yDocument')).find((n) => n.role === 'Image');
        if (pic) await core(page, 'write', { Write: { SelectObject: { id: pic.id } } });
        await inDocument(page);
      },
      steps: [
        'palette:ui.write.crop_dialog',
        'tab:^Crop left',
        'key:ArrowUp',
        'key:ArrowUp',
        'press:^OK$',
        'wait:300',
        'cmd:suite.arrange.align',
        'menu:^Align Left',
        'wait:300',
      ],
    },
    {
      name: 'Selection pane: select an object, hide and show it',
      setup: typed('write', '', 'fixtures/docx/features/text-boxes.docx'),
      steps: [
        'palette:suite.arrange.selection_pane',
        'wait:400',
        'pane:Selection',
        'press:^Click to select',
        'pane:Selection',
        'press:^Hide ',
        'pane:Selection',
        'press:^Show ',
      ],
    },
    {
      name: 'Shapes: a rectangle drawn from the keyboard',
      setup: typed('write', 'Shape here'),
      steps: [
        'cmd:suite.insert.shapes',
        'wait:300',
        'arrow:ArrowRight:^Rectangle',
        'key:Enter',
        'wait:300',
        'key:Enter',
        'wait:400',
      ],
    },
    {
      name: 'Table: row height and column width, Table Properties',
      setup: async (page) => {
        await openFixture(page, 'write', 'fixtures/docx/features/tables.docx');
        const nodes = await treeNodes(page, 'write', 'A11yDocument');
        const cell = nodes.find((n) => n.role === 'Cell' || n.role === 'ColumnHeader');
        const para = cell ? nodes.find((n) => n.id === cell.children[0]) : undefined;
        if (para)
          await core(page, 'write', {
            Write: { SelectTo: { pos: { block: para.id, offset: 0 }, extend: false } },
          });
      },
      steps: [
        'cmd:ui.write.table_row_height',
        'key:Control+a',
        'type:30',
        'key:Enter',
        'wait:300',
        'palette:ui.write.table_properties_dialog',
        'wait:300',
        'tab:^Indent from left',
        'key:ArrowUp',
        'press:^OK$',
      ],
    },
    {
      name: 'Underline styles from the Underline menu',
      setup: typed('write', 'Wavy words'),
      steps: [
        'key:Shift+Home',
        'f6:ribbon',
        'key:Home',
        'key:ArrowDown',
        'arrow:ArrowRight:^More options for Underline$',
        'key:Enter',
        'menu:^Wavy Underline$',
        'wait:300',
      ],
    },
    {
      name: 'View: Side to Side and Vertical page movement',
      setup: typed('write', ''),
      steps: ['cmd:suite.view.side_to_side', 'wait:300', 'cmd:suite.view.vertical_pages', 'wait:300'],
    },
    {
      name: 'A link opened from the context menu',
      setup: typed('write', '', 'fixtures/docx/features/hyperlinks.docx'),
      steps: [
        'key:Control+Home',
        'key:ArrowDown',
        'key:ArrowDown',
        'key:ArrowDown',
        'key:ArrowDown',
        'key:End',
        'key:ArrowLeft',
        'key:ArrowLeft',
        'key:Shift+F10',
        'menu:^Open Link$',
        'wait:300',
      ],
    },
    {
      name: 'File Conversion: a text file read in another encoding',
      setup: async (page) => {
        await openApp(page, 'write');
        const doc = await page.evaluate(
          async (b) => {
            const c = (window as unknown as W).__kiln?.client;
            if (!c) throw new Error('no client');
            return (await c.open(Uint8Array.from(b), { name: 'greeting.txt', path: null, read_only: false }))
              .doc;
          },
          [0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2],
        );
        await openApp(page, 'write', `&doc=${doc}`, false);
        await page.waitForTimeout(600);
      },
      steps: ['tab:^Text encoding', 'key:ArrowDown', 'press:^OK$', 'wait:300'],
    },
    {
      name: 'An IME composition cancelled',
      setup: typed('write', 'Kiln '),
      steps: [
        async (page) => {
          const cdp = await page.context().newCDPSession(page);
          await cdp.send('Input.imeSetComposition', { text: 'かな', selectionStart: 2, selectionEnd: 2 });
          await page.waitForTimeout(150);
          await cdp.send('Input.imeSetComposition', { text: '', selectionStart: 0, selectionEnd: 0 });
          await page.waitForTimeout(150);
          await cdp.detach();
        },
      ],
    },
    {
      name: 'Numbering: Set Numbering Value',
      setup: typed('write', '1. One'),
      steps: [
        'key:Enter',
        'type:Two',
        'palette:ui.write.numbering_value_dialog',
        'tab:^Set value to',
        'key:Control+a',
        'type:5',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'File ▸ New ▸ Blank document: the new document gets the options and the dictionary',
      setup: typed('write', ''),
      steps: ['tips:F', 'press:^New$', 'press:^Blank document', 'wait:1500'],
    },
    {
      name: 'Properties and File ▸ Info: Edit Properties, Inspect Document, Remove All',
      setup: typed('write', 'A sentence to comment on.'),
      steps: [
        'key:Shift+Home',
        'key:Control+Alt+m',
        'wait:300',
        'type:Check this',
        'key:Control+Enter',
        'wait:400',
        'palette:ui.write.properties_dialog',
        'wait:400',
        'tab:^Subject',
        'type:Kilns',
        'press:^OK$',
        'wait:400',
        'tips:F',
        'press:^Info$',
        'press:^Edit Properties$',
        'wait:400',
        'tab:^Title',
        'type:Firing log',
        'press:^OK$',
        'wait:400',
        'press:^Inspect Document',
        'wait:600',
        'press:^Remove All: Comments$',
        'wait:600',
        'press:^Done$',
        'key:Escape',
      ],
    },
    {
      name: 'File ▸ Export ▸ Export PDF',
      setup: typed('write', 'Exported words'),
      steps: ['tips:F', 'press:^Export$', 'press:^Export PDF$', 'wait:1500', 'key:Escape'],
    },
    {
      name: 'Document Recovery: a kept document opened',
      setup: async (page) => {
        await typed('write', 'Kept by the journal')(page);
        await page.waitForTimeout(1500);
        await openApp(page, 'write');
        await expect(page.locator('aside[aria-label="Document Recovery"]')).toBeVisible({ timeout: 20_000 });
      },
      steps: ['pane:Document Recovery', 'press:^Open ', 'wait:1000'],
    },
    {
      name: "Close with unsaved changes: Don't Save",
      setup: async (page) => {
        await typed('write', 'Thrown away')(page);
        await page.waitForTimeout(1500);
      },
      steps: ['key:Control+w', 'wait:400', "press:^Don't Save$", 'wait:600'],
    },
    {
      name: 'Kiln Home: a recent file pinned and removed from the list',
      setup: async (page) => {
        await typed('write', 'A recent file')(page);
        await core(page, 'write', {
          Suite: { SaveAs: { path: 'opfs:documents/Recent.docx', format: 'Docx' } },
        });
        await page.waitForTimeout(800);
        // Kiln starts on its Home.
        await page.goto('about:blank');
        await page.goto('/#/home');
        await expect(page.getByRole('row', { name: /Recent/ }).first()).toBeVisible({ timeout: 45_000 });
      },
      steps: ['press:^Pin Recent', 'wait:300', 'press:^Remove Recent', 'wait:300'],
    },
    {
      name: 'Version History: a saved version shown, named and restored',
      setup: async (page) => {
        await typed('write', 'First draft')(page);
        await core(page, 'write', {
          Suite: { SaveAs: { path: 'opfs:documents/Versions.docx', format: 'Docx' } },
        });
        await page.waitForTimeout(800);
        await inDocument(page);
        await key(page, 'End');
        await page.keyboard.type(' grows');
        await key(page, 'Control+s');
        await page.waitForTimeout(1200);
      },
      steps: [
        'palette:suite.file.version_history',
        'wait:800',
        'pane:Version',
        'press:Saved',
        'wait:600',
        'pane:Version',
        'press:^Name$',
        'type:Kept',
        'key:Enter',
        'wait:400',
        'pane:Version',
        'press:^Restore$',
        'wait:600',
      ],
    },
    collabScenario('write'),
    {
      name: 'The file changed by another program: Keep my version',
      setup: async (page) => {
        await page.addInitScript(() => {
          const pick = async () => {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('picked', { create: true });
            return dir.getFileHandle('Changed.docx', { create: true });
          };
          const w = window as unknown as Record<string, unknown>;
          w.showSaveFilePicker = pick;
          w.showOpenFilePicker = async () => [await pick()];
        });
        await typed('write', 'My words')(page);
        await core(page, 'write', { Suite: { SaveAs: { path: null, format: 'Docx' } } });
        await page.waitForTimeout(800);
        await page.getByTestId('save-status').click();
        const theirs = Array.from(readFileSync(repo('fixtures/docx/features/tables.docx')));
        await page.evaluate(async (b) => {
          const root = await navigator.storage.getDirectory();
          const dir = await root.getDirectoryHandle('picked', { create: true });
          const stream = await (await dir.getFileHandle('Changed.docx', { create: true })).createWritable();
          await stream.write(new Uint8Array(b));
          await stream.close();
        }, theirs);
        await inDocument(page);
        await key(page, 'End');
        await page.keyboard.type(' mine');
        await expect(page.getByTestId('file-changed')).toBeVisible({ timeout: 20_000 });
      },
      steps: ['press:^Keep my version$', 'wait:800'],
    },
  ],
  grid: [
    {
      name: 'Find and Replace: Find Next, Replace, Replace All',
      setup: (page) => CONTEXTS.grid[1]?.setup(page) ?? Promise.resolve(),
      steps: [
        'key:Control+h',
        'type:East',
        'tab:^Replace with',
        'type:West',
        'press:^Find Next$',
        'stab:^Replace$',
        'key:Enter',
        'wait:200',
        'press:^Replace All$',
        'key:Escape',
        'key:Escape',
      ],
    },
    {
      name: 'Define Name, then delete it in the Name Manager',
      setup: (page) => CONTEXTS.grid[1]?.setup(page) ?? Promise.resolve(),
      steps: [
        'palette:grid.formulas.define_name',
        'type:KilnName',
        'press:^OK$',
        'wait:300',
        'palette:grid.formulas.name_manager',
        'tab:KilnName',
        'key:Enter',
        'press:^Delete$',
        'wait:300',
        'key:Escape',
      ],
    },
    {
      name: 'Goal Seek',
      setup: sheet([['5', '=A1*2']]),
      steps: [
        'key:Control+Home',
        'key:ArrowRight',
        'palette:grid.data.goal_seek',
        'tab:^To value',
        'type:30',
        'tab:^By changing cell',
        'type:A1',
        'press:^OK$',
        'wait:500',
        'press:^OK$',
      ],
    },
    {
      name: 'Lock Cell, Protect Sheet, Protect Workbook',
      setup: typed('grid', 'Locked'),
      steps: [
        'key:Enter',
        'key:ArrowUp',
        'tips:H,O',
        'menu:^Lock Cell$',
        'wait:300',
        'palette:grid.review.protect_sheet',
        'press:^OK$',
        'wait:300',
        'palette:grid.review.protect_workbook',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'Tables: create, Total Row and its function, options, resize',
      setup: typed('grid', 'Item'),
      steps: [
        'key:Enter',
        'key:ArrowUp',
        'palette:grid.insert.table',
        'press:^OK$',
        'wait:400',
        async (page) => CONTEXTS.grid[1]?.setup(page),
        'cmd:ui.grid.table_total_row',
        'wait:300',
        'cmd:ui.grid.table_banded_columns',
        'wait:300',
        'key:Control+Home',
        'key:Control+ArrowDown',
        'key:ArrowRight',
        'key:ArrowRight',
        'wait:200',
        'key:Alt+ArrowDown',
        'wait:300',
        'menu:^Average$',
        'wait:300',
        'palette:ui.grid.resize_table',
        'key:Control+a',
        'type:$A$1:$E$5',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'AutoFilter menu: sort, then filter a column',
      setup: (page) => CONTEXTS.grid[1]?.setup(page) ?? Promise.resolve(),
      steps: [
        'key:Control+Home',
        'key:Alt+ArrowDown',
        'wait:300',
        'stab:^Sort A to Z',
        'key:Enter',
        'wait:300',
        'key:Alt+ArrowDown',
        'wait:300',
        'tab:^East$',
        'key:Space',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'Comments pane: new, edit, resolve, delete',
      setup: typed('grid', ''),
      steps: [
        'palette:suite.review.new_comment',
        'wait:300',
        'type:A comment',
        'key:Control+Enter',
        'wait:400',
        'pane:Comments',
        'press:^Edit comment$',
        'key:End',
        'type: edited',
        'key:Enter',
        'wait:300',
        'pane:Comments',
        'press:^Resolve thread$',
        'pane:Comments',
        'press:^Delete thread$',
      ],
    },
    {
      name: 'Notes: a new note, Show All Notes, Convert to Comments',
      setup: typed('grid', ''),
      steps: [
        'palette:grid.review.new_note',
        'wait:300',
        'type:A note',
        'key:Escape',
        'wait:300',
        'ribbon:Review>^Notes$',
        'menu:^Show All Notes',
        'wait:300',
        'ribbon:Review>^Notes$',
        'menu:^Convert to Comments',
        'wait:300',
      ],
    },
    {
      name: 'Spelling pane: Ignore Once, Ignore All, Change',
      setup: sheet([['Teh'], ['kiln hass'], ['fier'], ['zorpq']]),
      steps: [
        'key:Control+Home',
        'key:F7',
        'wait:800',
        'pane:Spelling',
        'press:^Ignore Once$',
        'pane:Spelling',
        'press:^Ignore All$',
        'pane:Spelling',
        'press:^Change$',
      ],
    },
    {
      name: 'Sheets: next sheet, Move or Copy, Tab Color, Hide and Unhide',
      setup: typed('grid', '', 'fixtures/xlsx/features/pivot-tables.xlsx'),
      steps: [
        'key:Control+PageDown',
        'wait:300',
        'key:Control+PageUp',
        'wait:300',
        'tips:H,O',
        'menu:^Move or Copy Sheet',
        'wait:300',
        'tab:^Before sheet',
        'key:ArrowDown',
        'press:^OK$',
        'wait:300',
        'tips:H,O',
        'menu:^Tab Color',
        'wait:300',
        'press:^Tab color',
        'wait:300',
        'key:ArrowRight',
        'key:Enter',
        'wait:200',
        'press:^OK$',
        'wait:300',
        'tips:H,O',
        'menu:^Hide Sheet$',
        'wait:300',
        'tips:H,O',
        'menu:^Unhide Sheet',
        'wait:300',
        'tab:^Unhide sheet',
        'key:ArrowDown',
        'press:^OK$',
        'wait:300',
        // Find All lists the cells of every sheet: Enter on one of another sheet shows it.
        'key:Control+h',
        'type:e',
        'press:^Options >>$',
        'tab:^Within',
        'key:ArrowDown',
        'wait:200',
        'arrow:ArrowDown:^Workbook$',
        'key:Enter',
        'wait:200',
        'press:^Find All$',
        'wait:400',
        'press:^Report',
        'wait:300',
        'key:Escape',
      ],
    },
    {
      name: 'Fill Series and Custom Lists',
      setup: typed('grid', '1'),
      steps: [
        'key:Enter',
        'key:ArrowUp',
        'key:Shift+ArrowDown',
        'key:Shift+ArrowDown',
        'key:Shift+ArrowDown',
        'palette:ui.grid.fill_series',
        'press:^OK$',
        'wait:300',
        'palette:ui.grid.custom_lists',
        'tab:^List entries',
        'type:North',
        'key:Enter',
        'type:South',
        'press:^Add$',
        'press:^Import from Cells$',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'Format Cells: a custom number format applied, then deleted',
      setup: typed('grid', '12.5'),
      steps: [
        'key:Enter',
        'key:ArrowUp',
        'palette:grid.cells.format_cells',
        'tab:^Type$',
        'key:Control+a',
        'type:0.000',
        'press:^OK$',
        'wait:300',
        'palette:grid.cells.format_cells',
        'tab:^Type$',
        'key:Shift+Tab',
        'key:End',
        'wait:200',
        'tab:^0\\.000$',
        'press:^Delete$',
        'wait:200',
        'press:^OK$',
      ],
    },
    {
      name: 'Sparklines inserted from the dialog',
      setup: sheet([['1', '3', '2']]),
      steps: [
        'key:Control+Home',
        'palette:grid.insert.sparkline_line',
        'tab:^Data Range',
        'key:Control+a',
        'type:A1:C1',
        'tab:^Location Range',
        'key:Control+a',
        'type:E1',
        'press:^OK$',
        'wait:300',
        'key:Control+g',
        'type:E1',
        'key:Enter',
        'wait:300',
        'ribbon:Sparkline>^High Point',
        'wait:300',
      ],
    },
    {
      name: 'A PivotTable created from the data',
      setup: (page) => CONTEXTS.grid[1]?.setup(page) ?? Promise.resolve(),
      steps: [
        'palette:grid.insert.pivot_table',
        'press:^OK$',
        'wait:800',
        async (page) => CONTEXTS.grid[4]?.setup(page),
        'ribbon:PivotTable Analyze>^Select$',
        'menu:^Entire PivotTable',
        'wait:300',
      ],
    },
    {
      name: 'Chart: legend and data labels from Add Chart Element, the Format pane, Select Data',
      setup: (page) => CONTEXTS.grid[3]?.setup(page) ?? Promise.resolve(),
      steps: [
        'tips:JC,AC',
        'menu:^Legend$',
        'menu:^Left',
        'wait:300',
        'tips:JC,AC',
        'menu:^Data Labels$',
        'menu:^Center',
        'wait:300',
        'ribbon:Format>^Format Selection',
        'wait:600',
        'pane:Format',
        'tab:^Chart title',
        'key:Control+a',
        'type:Kiln chart',
        'key:Enter',
        'wait:200',
        'pane:Format',
        'press:^Fill: Jade$',
        'pane:Format',
        'tab:^Minimum$',
        'key:Control+a',
        'type:1',
        'key:Enter',
        'wait:200',
        'pane:Format',
        'press:^Chart Area fill: Jade$',
        'wait:200',
        'tips:JC,SD',
        'wait:500',
        'tab:^Show data in hidden',
        'key:Space',
        'press:^OK$',
        'wait:300',
        'tips:JC,SD',
        'wait:500',
        'press:^Edit$',
        'wait:300',
        'tab:^Series name',
        'key:End',
        'type: 2',
        'press:^Apply$',
        'wait:200',
        'press:^OK$',
        'wait:200',
        'key:Escape',
      ],
    },
    {
      name: 'Insert Hyperlink',
      setup: typed('grid', 'Kiln'),
      steps: [
        'key:Enter',
        'key:ArrowUp',
        'key:Control+k',
        'tab:^Address',
        'type:https://example.com',
        'press:^OK$',
      ],
    },
    {
      name: 'Page Layout: Print Area, Breaks, a theme; Formulas: Calculation Options',
      setup: typed('grid', 'Area'),
      steps: [
        'key:Enter',
        'key:Shift+ArrowUp',
        'ribbon:Page Layout>^Print Area',
        'menu:^Set Print Area',
        'wait:300',
        'key:ArrowDown',
        'key:ArrowDown',
        'ribbon:Page Layout>^Breaks',
        'menu:^Insert Page Break',
        'wait:300',
        'tips:P,TH',
        'wait:300',
        'key:ArrowRight',
        'key:Enter',
        'wait:300',
        'ribbon:Formulas>^Calculation Options',
        'menu:^Manual',
        'wait:300',
        'ribbon:Formulas>^Calculation Options',
        'menu:^Automatic$',
      ],
    },
    {
      name: 'Options: the 1904 date system and the theme',
      setup: typed('grid', ''),
      steps: [
        'palette:ui.options.open',
        'press:^Advanced$',
        'tab:1904',
        'key:Space',
        'key:Space',
        'press:^General$',
        'tab:^Theme',
        'key:ArrowDown',
        'key:ArrowDown',
        'wait:400',
        'key:ArrowUp',
        'key:ArrowUp',
        'wait:300',
        'key:Escape',
      ],
    },
    {
      name: 'A Warning validation: the invalid entry kept',
      setup: async (page) => {
        await typed('grid', '')(page);
        await core(page, 'grid', {
          Grid: {
            SetValidation: {
              rule: {
                kind: 'Whole',
                op: 'Between',
                formula1: '1',
                formula2: '10',
                allow_blank: true,
                dropdown: false,
                show_input: false,
                input_title: '',
                input: '',
                show_error: true,
                style: 'Warning',
                error_title: 'Out of range',
                error: 'From 1 to 10',
              },
            },
          },
        });
        await inDocument(page);
      },
      steps: ['type:99', 'key:Enter', 'wait:400', 'press:^Yes$', 'wait:300'],
    },
    {
      name: 'Data ▸ From Text/CSV: a file imported',
      setup: typed('grid', ''),
      steps: [
        async (page) => {
          page.once(
            'filechooser',
            (fc) => void fc.setFiles(repo('fixtures/csv/rfc4180.csv')).catch(() => undefined),
          );
        },
        'palette:grid.data.from_text_csv',
        'wait:1200',
        'press:^Load$',
        'wait:600',
      ],
    },
    {
      name: 'The error indicator menu (Alt+Shift+F10): Convert to Number, Ignore Error',
      setup: sheet([["'12"], ["'34"]]),
      steps: [
        'key:Control+Home',
        'wait:300',
        'key:Alt+Shift+F10',
        'menu:^Convert to Number$',
        'wait:300',
        'key:ArrowDown',
        'key:Alt+Shift+F10',
        'menu:^Ignore Error$',
        'wait:300',
      ],
    },
    {
      name: 'Objects: Go To Special ▸ Objects, Tab to the next, the object menu, Delete',
      setup: (page) => CONTEXTS.grid[3]?.setup(page) ?? Promise.resolve(),
      steps: [
        async (page) => {
          await core(page, 'grid', { Grid: { SelectDrawing: { index: null } } });
          await inDocument(page);
        },
        'palette:ui.grid.go_to_special',
        'tab:^Blanks$',
        'arrow:ArrowDown:^Objects$',
        'press:^OK$',
        'wait:300',
        'key:Tab',
        'wait:300',
        'cmd:suite.arrange.bring_forward',
        'wait:300',
        'key:Shift+F10',
        'menu:^Delete$',
        'wait:300',
      ],
    },
    {
      name: 'Format Painter: copied, then pasted on a cell',
      setup: typed('grid', 'Painted'),
      steps: ['key:Enter', 'key:ArrowUp', 'tips:H,FP', 'wait:200', 'key:ArrowDown', 'key:Enter', 'wait:300'],
    },
    {
      name: 'Draw Border pen put away with Esc',
      setup: typed('grid', ''),
      steps: ['tips:H,B', 'wait:200', 'menu:^Draw Border$', 'wait:200', 'key:Escape', 'wait:200'],
    },
    {
      name: 'Pictures: inserted from a file, then Reset Picture',
      setup: async (page) => {
        // The browser's own file input (the system's file dialog), not the File System Access one.
        await page.addInitScript(() => {
          const w = window as unknown as Record<string, unknown>;
          delete w.showOpenFilePicker;
          delete w.showSaveFilePicker;
        });
        await typed('grid', '')(page);
      },
      steps: [
        choosePicture,
        'palette:suite.insert.pictures',
        'wait:1200',
        'ribbon:Picture Format>^Reset Picture',
        'menu:^Reset Picture$',
        'wait:300',
      ],
    },
  ],
  deck: [
    {
      name: 'Find and Replace: Find Next, Replace, Replace All',
      setup: typed('deck', '', 'fixtures/pptx/features/notes.pptx'),
      steps: [
        'key:Control+h',
        'wait:300',
        'type:e',
        'tab:^Replace with',
        'type:E',
        'press:^Find Next$',
        'press:^Replace$',
        'press:^Replace All$',
        'key:Escape',
      ],
    },
    {
      name: 'comments: new, reply, edit, resolve, next, select, delete',
      setup: typed('deck', ''),
      steps: [
        'key:Control+Alt+m',
        'wait:400',
        'type:First comment',
        'key:Control+Enter',
        'wait:400',
        'key:Control+Alt+m',
        'wait:400',
        'type:Second comment',
        'key:Control+Enter',
        'wait:400',
        'pane:Comments',
        'press:^Reply$',
        'type:A reply',
        'key:Control+Enter',
        'wait:300',
        'pane:Comments',
        'press:^Edit comment$',
        'key:End',
        'type: edited',
        'key:Control+Enter',
        'wait:300',
        'pane:Comments',
        'press:^Resolve thread$',
        'ribbon:Review>^Next',
        'wait:300',
        'pane:Comments',
        'press:^Delete thread$',
        'wait:300',
      ],
    },
    {
      name: 'Insert: a text box and a shape placed from the keyboard',
      setup: typed('deck', ''),
      steps: [
        'palette:suite.insert.text_box',
        'wait:500',
        'key:Enter',
        'wait:400',
        'type:Boxed',
        'key:Escape',
        'key:Escape',
        'cmd:suite.insert.shapes',
        'wait:300',
        'arrow:ArrowRight:^Rectangle',
        'key:Enter',
        'wait:300',
        'key:Enter',
        'wait:400',
      ],
    },
    {
      name: 'Insert: a symbol, an icon, a code block',
      setup: typed('deck', ''),
      steps: [
        'key:Tab',
        'key:Enter',
        'palette:suite.insert.symbol',
        'wait:400',
        'press:^Insert ',
        'wait:200',
        'key:Escape',
        'palette:suite.insert.icons',
        'wait:800',
        'type:star',
        'wait:300',
        'tab:^Star',
        'key:Enter',
        'press:^Insert$',
        'wait:400',
        'palette:suite.insert.code_block',
        'wait:400',
        'tab:^Code$',
        'type:let kiln = 1;',
        'stab:^(Insert|OK)$',
        'key:Enter',
        'wait:400',
      ],
    },
    {
      name: 'Character spacing and text shadow on selected text',
      setup: typed('deck', ''),
      steps: [
        'key:Tab',
        'type:Spaced',
        'key:Shift+Home',
        'ribbon:Home>^Character Spacing',
        'menu:^Loose',
        'wait:300',
      ],
    },
    {
      name: 'Speaker notes typed in the notes pane',
      setup: typed('deck', ''),
      steps: ['pane:Speaker notes', 'type:Notes for this slide', 'wait:300', 'key:F6', 'wait:600'],
    },
    {
      name: 'Slide thumbnails: select and move slides from the keyboard (F6 to the thumbnails)',
      setup: async (page) => {
        await typed('deck', '')(page);
        await core(page, 'deck', { Deck: { NewSlide: { layout: null } } });
        await core(page, 'deck', { Deck: { NewSlide: { layout: null } } });
        await inDocument(page);
      },
      steps: [
        'pane:Slides',
        'key:ArrowUp',
        'key:Shift+ArrowUp',
        'key:Control+ArrowDown',
        'key:Control+ArrowUp',
        'wait:300',
      ],
    },
    {
      name: 'Sections: rename, move, remove (context menu of a section)',
      setup: typed('deck', '', 'fixtures/pptx/features/sections.pptx'),
      steps: [
        'palette:ui.deck.rename_section',
        'wait:300',
        'key:Control+a',
        'type:Renamed',
        'press:^(OK|Rename)$',
        'wait:300',
        'pane:Slides',
        'key:Shift+F10',
        'menu:^Section$',
        'menu:^Move Section Down',
        'wait:300',
        'pane:Slides',
        'key:Shift+F10',
        'menu:^Section$',
        'menu:^Remove Section$',
        'wait:300',
      ],
    },
    {
      name: 'Slide Master: a layout selected and renamed',
      setup: typed('deck', ''),
      steps: [
        'ribbon:View>^Slide Master',
        'wait:600',
        'pane:Slide masters',
        'key:ArrowDown',
        'wait:300',
        'wait:300',
        'palette:ui.deck.rename_master',
        'key:Control+a',
        'type:Kiln layout',
        'press:^(OK|Rename)$',
        'wait:300',
        'ribbon:Slide Master>^Close Master View',
      ],
    },
    {
      name: 'Outline view: text typed into the outline',
      setup: typed('deck', ''),
      steps: [
        'ribbon:View>^Outline View',
        'wait:600',
        'pane:Outline',
        'tab:^Slide 1 title',
        'type:Outlined',
        'wait:400',
        'key:Enter',
        'wait:300',
      ],
    },
    {
      name: 'Selection pane: a shape selected',
      setup: typed('deck', ''),
      steps: [
        'palette:suite.arrange.selection_pane',
        'wait:400',
        'press:^(Title|Subtitle)',
        'wait:300',
        'key:Escape',
      ],
    },
    {
      name: 'Design Ideas: an idea applied',
      setup: typed('deck', ''),
      steps: [
        'key:Tab',
        'type:A title for ideas',
        'key:Escape',
        'key:Escape',
        'palette:deck.drawing.design_ideas',
        'wait:2000',
        'pane:Designer',
        'tab:^(?!Close).',
        'key:Enter',
        'wait:400',
      ],
    },
    {
      name: 'Design ▸ Browse for Themes: a theme file applied',
      setup: typed('deck', ''),
      steps: [
        async (page) => {
          page.once(
            'filechooser',
            (fc) => void fc.setFiles(repo('fixtures/pptx/features/theme-colors.pptx')).catch(() => undefined),
          );
        },
        'palette:ui.deck.browse_themes',
        'wait:1500',
      ],
    },
    {
      name: 'Animations: the Animation Pane (select, repeat), the Animation Painter with Enter',
      setup: typed('deck', '', 'fixtures/pptx/features/animations.pptx'),
      steps: [
        'palette:deck.animations.animation_pane',
        'wait:500',
        'pane:Animation',
        'tab:^Effects',
        'key:ArrowDown',
        'wait:300',
        'tab:^Repeat',
        'key:Control+a',
        'type:2',
        'key:Enter',
        'wait:300',
        async (page) => inDocument(page),
        'key:Tab',
        'ribbon:Animations>^Animation Painter',
        'wait:200',
        'key:Tab',
        'key:Enter',
        'wait:300',
      ],
    },
    {
      name: 'Slide show: See All Slides and go to one, Set Up Slide Show',
      setup: typed('deck', '', 'fixtures/pptx/features/notes.pptx'),
      steps: [
        'key:F5',
        'wait:1500',
        'press:^See all slides$',
        'wait:500',
        'key:ArrowRight',
        'key:Enter',
        'wait:500',
        'key:Escape',
        'wait:800',
        async (page) => inDocument(page),
        'palette:deck.show.set_up',
        'wait:400',
        'press:^OK$',
        'wait:300',
      ],
    },
    {
      name: 'Table: Select ▸ Select Table',
      setup: (page) => CONTEXTS.deck[3]?.setup(page) ?? Promise.resolve(),
      steps: ['key:Enter', 'wait:300', 'ribbon:Layout>^Select$', 'menu:^Select Table$', 'wait:300'],
    },
    {
      name: 'Chart: Edit Data typed in its sheet',
      setup: (page) => CONTEXTS.deck[4]?.setup(page) ?? Promise.resolve(),
      steps: [
        'palette:ui.deck.edit_chart_data',
        'wait:600',
        'tab:^B2$',
        'type:42',
        'key:Enter',
        'wait:400',
        'key:Escape',
      ],
    },
    {
      name: 'Text: Tab demotes a bullet in a placeholder',
      setup: typed('deck', ''),
      steps: ['key:Tab', 'key:Tab', 'type:Point', 'key:Enter', 'key:Tab', 'type:Sub point', 'wait:300'],
    },
    {
      name: 'Guides: one moved with the arrows and removed with Delete',
      setup: typed('deck', ''),
      steps: [
        'ribbon:View>^Guides',
        'wait:300',
        async (page) => {
          await page.locator('[data-testid="deck-guide"]').first().focus();
        },
        'key:ArrowRight',
        'wait:200',
        'key:Delete',
        'wait:300',
      ],
    },
    {
      name: 'Text View: text typed into a shape (the core places the caret)',
      setup: typed('deck', ''),
      steps: [
        'palette:ui.view.text_view',
        'wait:500',
        'tab:^Title',
        'type:Typed in the view',
        'wait:400',
        'palette:ui.view.text_view',
      ],
    },
    collabScenario('deck'),
  ],
};

/**
 * Catalog commands that are not user commands: the bridge's own checks and the frame's plumbing,
 * which no control runs, in Office or in Kiln.
 */
/**
 * Pointer gestures (WCAG 2.5.7): commands a drag or a click at a point sends. Each has a keyboard
 * command doing the same, which the walk must reach.
 */
const POINTER: Readonly<Record<string, string>> = {
  'write.caret.select_at': 'write.caret.move',
  'write.edit.move_selection': 'suite.edit.cut',
  'write.arrange.transform': 'write.arrange.layout',
  'write.view.split_region': 'suite.view.split',
  'grid.nav.select_cell': 'grid.nav.move',
  'grid.nav.select_rows': 'grid.editing.select_row',
  'grid.nav.select_columns': 'grid.editing.select_column',
  'grid.editing.drag_fill': 'grid.editing.fill_down',
  'grid.editing.drag_cells': 'suite.edit.cut',
  'grid.cells.resize_rows': 'grid.cells.row_height',
  'grid.cells.resize_columns': 'grid.cells.column_width',
  'grid.editing.point_ref': 'grid.editing.point_move',
  'grid.drawing.place': 'grid.drawing.format',
  'grid.font.draw_border': 'grid.font.borders',
  'grid.page_layout.move_page_break': 'grid.page_layout.page_break',
  'deck.stage.pointer_down': 'deck.select.next',
  'deck.stage.pointer_move': 'deck.drawing.nudge',
  'deck.stage.pointer_up': 'deck.select.next',
  'deck.view.move_guide': 'deck.view.add_guide',
  // The outline's level buttons and +/- (clicks beside the headers): Data ▸ Show and Hide Detail.
  'grid.data.show_outline_level': 'grid.data.outline_detail',
  'grid.data.toggle_outline_group': 'grid.data.outline_detail',
  // Auto Fill Options follows a drag of the fill handle: Home ▸ Fill ▸ Series offers the same.
  'grid.editing.auto_fill_option': 'ui.grid.fill_series',
  // The ruler's indent markers: Home ▸ Increase and Decrease List Level.
  'deck.paragraph.indent': 'suite.format.indent',
  // A drag in the Animation Pane: its Move Earlier and Move Later buttons.
  'deck.animations.move': 'deck.animations.reorder',
  // A click on the show: the arrows and Space go on as a click does (ink is drawn by hand).
  'deck.show.pointer': 'deck.show.next',
};

const PLUMBING: ReadonlyMap<string, string> = new Map([
  ['suite.dev.ping', 'the bridge test round trip'],
  ['suite.dev.echo', 'the bridge test round trip'],
  ['suite.view.repaint', 'the frame redraws the canvas'],
  ['suite.home.recents_load', 'Kiln Home reads the recent files when it opens'],
  [
    'grid.alignment.horizontal',
    "the core's form of Home ▸ Align Left, Center and Right and Format Cells ▸ Alignment, which send it as suite.format.align_* and grid.format.apply",
  ],
]);

/**
 * Commands the core runs as a step of another one, which no control sends itself: reached when
 * that command is.
 */
const STEP_OF: Readonly<Record<string, string>> = {
  // The file chosen after Insert ▸ Pictures goes in with it.
  'grid.insert.picture': 'suite.insert.pictures',
  'deck.insert.picture': 'suite.insert.pictures',
  // The browser opens the audience window itself when a show starts in Presenter View; the
  // desktop shell asks the core for it.
  'suite.view.open_window': 'deck.show.presenter_view',
  // Tell me and the ribbon's Delete and Duplicate Sheet send these with the sheet on show.
  'grid.sheet.delete': 'ui.grid.delete_sheet',
  'grid.sheet.duplicate': 'ui.grid.duplicate_sheet',
};

/** The commands that open each app's dialogs and panes (as the dialog sweep finds them). */
const SURFACE_OPENERS: Readonly<Record<Exclude<App, 'notes'>, readonly string[]>> = {
  write: [
    'ui.options.open',
    'suite.file.version_history',
    'suite.edit.paste_special',
    'suite.edit.find',
    'suite.edit.replace',
    'suite.edit.go_to',
    'suite.format.font_dialog',
    'suite.format.paragraph_dialog',
    'suite.insert.link',
    'suite.insert.date_time',
    'suite.insert.table',
    'suite.insert.icons',
    'suite.insert.symbol',
    'suite.review.spelling',
    'suite.review.word_count',
    'write.paragraph.sort',
    'write.styles.pane',
    'write.insert.quick_parts',
    'suite.layout.page_setup',
    'suite.arrange.selection_pane',
    'write.review.reviewing_pane',
    'ui.write.bookmark',
    'ui.write.comments_pane',
    'ui.write.toc_dialog',
    'ui.write.caption_dialog',
    'ui.write.cross_reference_dialog',
    'ui.write.footnote_dialog',
    'ui.write.properties_dialog',
    'ui.write.word_goal_dialog',
    'ui.write.zoom_dialog',
    'ui.write.language_dialog',
    'write.file.text_encoding_dialog',
    'suite.review.new_comment',
  ],
  grid: [
    'ui.options.open',
    'suite.edit.paste_special',
    'suite.edit.find',
    'suite.edit.replace',
    'suite.edit.go_to',
    'suite.insert.link',
    'suite.insert.text_box',
    'suite.insert.symbol',
    'suite.insert.header_footer',
    'suite.review.spelling',
    'suite.review.new_comment',
    'ui.grid.fill_series',
    'ui.grid.custom_lists',
    'grid.styles.format_as_table',
    'grid.cells.format_cells',
    'grid.insert.pivot_table',
    'grid.insert.table',
    'grid.insert.recommended_charts',
    'grid.insert.sparkline_line',
    'grid.review.new_note',
    'grid.formulas.insert_function',
    'grid.formulas.name_manager',
    'grid.formulas.define_name',
    'grid.formulas.create_from_selection',
    'grid.formulas.error_checking',
    'grid.formulas.evaluate_formula',
    'grid.formulas.watch_window',
    'grid.formulas.formula_lab',
    'grid.data.sort',
    'grid.data.text_to_columns',
    'grid.data.remove_duplicates',
    'grid.data.data_validation',
    'grid.data.goal_seek',
    'grid.data.subtotal',
    'grid.review.protect_sheet',
    'grid.review.protect_workbook',
    'ui.grid.row_height',
    'ui.grid.column_width',
    'ui.grid.tab_color',
    'ui.grid.insert_cells',
    'ui.grid.delete_cells',
    'ui.grid.cf_rule',
    'ui.grid.cf_manage',
    'ui.grid.iterative',
    'ui.grid.go_to_special',
    'grid.formulas.use_in_formula',
  ],
  deck: [
    'ui.options.open',
    'suite.file.version_history',
    'suite.edit.paste_special',
    'suite.edit.find',
    'suite.edit.replace',
    'suite.format.font_dialog',
    'suite.format.paragraph_dialog',
    'suite.insert.link',
    'suite.insert.date_time',
    'suite.insert.table',
    'suite.insert.shapes',
    'suite.insert.icons',
    'suite.insert.symbol',
    'suite.insert.code_block',
    'suite.review.spelling',
    'suite.review.new_comment',
    'suite.arrange.selection_pane',
    'ui.deck.format_background',
    'ui.deck.slide_size',
    'ui.deck.replace_fonts',
    'ui.deck.rename_section',
    'ui.deck.insert_chart',
    'deck.drawing.design_ideas',
    'deck.animations.animation_pane',
    'ui.deck.rename_master',
  ],
};

/** The dialogs and panes of the contexts after the blank document (a table, a picture, a chart). */
const CONTEXT_OPENERS: Readonly<Record<Exclude<App, 'notes'>, readonly string[]>> = {
  write: [
    'ui.write.table_properties_dialog',
    'ui.write.layout_dialog',
    'ui.write.crop_dialog',
    'ui.write.corrections_dialog',
    'suite.picture.alt_text',
    'ui.write.text_box_options',
    'ui.write.change_picture',
    'suite.chart.format_area',
  ],
  grid: [
    'ui.grid.resize_table',
    'ui.grid.format_object',
    'suite.picture.alt_text',
    'suite.picture.crop',
    'suite.chart.format_area',
    'ui.grid.pivot_options',
    'ui.grid.pivot_field_settings',
    'ui.grid.pivot_fields',
  ],
  deck: [
    'ui.deck.alt_text',
    'ui.deck.crop',
    'ui.deck.size_position',
    'ui.deck.change_shape',
    'ui.deck.format_shape',
    'ui.deck.split_cells',
    'ui.deck.cell_margins',
    'ui.deck.edit_chart_data',
    'suite.chart.format_area',
  ],
};

/**
 * Core command variant → catalog id, per app (`Write`, `Grid`, `Deck`, `Suite`), read from the
 * protocol's command tables: the document surfaces send commands straight to the core, and this
 * names them.
 */
function variantIds(): Record<string, Record<string, string>> {
  const out: Record<string, Record<string, string>> = {};
  const dir = repo('crates/kiln-protocol/src/commands');
  const files: [string, string][] = [];
  for (const app of ['write', 'grid', 'deck', 'suite', 'notes']) {
    files.push([app, `${dir}/${app}.rs`]);
    try {
      for (const f of readdirSync(`${dir}/${app}`)) files.push([app, `${dir}/${app}/${f}`]);
    } catch {
      // no sub-modules
    }
  }
  for (const [app, file] of files) {
    const text = readFileSync(file, 'utf8');
    out[cap(app)] ??= {};
    const map = out[cap(app)] as Record<string, string>;
    for (const m of text.matchAll(/Self::(\w+)\b[^=]*=>\s*\{\s*id:\s*"([^"]+)"/g))
      map[m[1] as string] ??= m[2] as string;
  }
  return out;
}
const VARIANT_IDS = variantIds();

/**
 * Records every command sent to the core from now on, by any client of the window (the slide show
 * route and the editor share the client class, so the prototype's method is the one wrapped).
 */
async function recordCore(page: Page): Promise<void> {
  await page.evaluate(() => {
    const k = (window as unknown as W).__kiln;
    const c = k?.client as unknown as object | null;
    if (!c) return;
    const proto = Object.getPrototypeOf(c) as {
      execute: (d: unknown, cmd: unknown) => Promise<unknown>;
      __a11y?: boolean;
    };
    const g = window as unknown as { __a11ySent?: string[] };
    g.__a11ySent ??= [];
    if (proto.__a11y) return;
    proto.__a11y = true;
    const run = proto.execute;
    proto.execute = function (this: unknown, d, cmd) {
      if (cmd && typeof cmd === 'object') {
        const [app, inner] = Object.entries(cmd as Record<string, unknown>)[0] ?? [];
        const variant =
          typeof inner === 'string' ? inner : inner && typeof inner === 'object' ? Object.keys(inner)[0] : '';
        const nested =
          inner && typeof inner === 'object'
            ? (Object.values(inner as Record<string, unknown>)[0] as unknown)
            : undefined;
        const sub =
          typeof nested === 'string'
            ? nested
            : nested && typeof nested === 'object'
              ? Object.keys(nested)[0]
              : '';
        (window as unknown as { __a11ySent: string[] }).__a11ySent.push(`${app}:${variant}:${sub ?? ''}`);
      }
      return run.call(this, d, cmd);
    };
  });
}

/** Chart editing: Grid sheets and Deck slides hold charts. */
const CHART_COMMANDS = [
  'suite.chart.data_labels',
  'suite.chart.edit_series',
  'suite.chart.format_area',
  'suite.chart.format_axis',
  'suite.chart.format_series',
  'suite.chart.hidden_empty',
  'suite.chart.legend',
  'suite.chart.title',
] as const;

/** Window and file commands, the same in every app (closing, recovery, Kiln Home, PDF). */
const WINDOW_COMMANDS = [
  'suite.file.close_without_saving',
  'suite.file.keep_mine',
  'suite.file.recovery_open',
  'suite.file.save_pdf',
  'suite.home.recent_pin',
  'suite.home.recent_remove',
  'suite.edit.paste_as',
  // File ▸ Info's Properties and Inspect Document are the same dialogs in every app.
  'suite.file.properties',
  'suite.file.inspect_remove',
] as const;

/**
 * Shared commands an app's walk leaves to another app's, which reaches them the same way (each
 * app's test fails when one of its own is missed, so together they reach the whole catalog).
 */
const REACHED_ELSEWHERE: Readonly<Record<Exclude<App, 'notes'>, ReadonlyMap<string, string>>> = {
  write: new Map(CHART_COMMANDS.map((id) => [id, 'documents hold no charts: Grid and Deck reach it'])),
  grid: new Map([
    ...WINDOW_COMMANDS.map((id) => [id, 'the same in every app: the Write walk reaches it'] as const),
    [
      'grid.editing.put_back_draft',
      "the recovered window's Put it back button: pwa-recovery reaches it with F6 and Enter",
    ],
  ]),
  deck: new Map([
    ...WINDOW_COMMANDS.map((id) => [id, 'the same in every app: the Write walk reaches it'] as const),
    ...CHART_COMMANDS.map((id) => [id, 'the same chart pane as Grid: the Grid walk reaches it'] as const),
    ...(
      ['suite.collab.version_preview', 'suite.collab.version_rename', 'suite.collab.version_restore'] as const
    ).map((id) => [id, 'the same Version History pane: the Write walk reaches it'] as const),
  ]),
};

test.describe('a11y', () => {
  test.describe.configure({ timeout: 900_000 });

  test('Home passes axe in light, dark and high contrast, desktop and phone', async ({ page }) => {
    for (const [w, h] of [
      [1440, 900],
      [390, 844],
    ] as const) {
      await page.setViewportSize({ width: w, height: h });
      for (const theme of ['light', 'dark', 'hc'] as const) {
        await page.goto('/#/home?blink=0');
        await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 60_000 });
        await page.evaluate((t) => document.documentElement.setAttribute('data-theme', t), theme);
        await expectAxe(page, `Home ${theme} ${w}`);
        if (theme === 'hc') expect(await enhancedContrast(page), `Home high contrast ${w}`).toEqual([]);
      }
    }
  });

  for (const app of APPS) {
    test(`${app}: the frame passes axe in light, dark and high contrast, on every ribbon tab`, async ({
      page,
    }) => {
      await openApp(page, app);
      for (const theme of ['light', 'dark', 'hc'] as const) {
        await page.evaluate((t) => document.documentElement.setAttribute('data-theme', t), theme);
        await page.waitForTimeout(200);
        await expectAxe(page, `${app} frame ${theme}`);
      }
      const tabs = page.getByRole('tablist', { name: 'Ribbon tabs' }).getByRole('tab');
      const n = await tabs.count();
      // High contrast at 7:1 on every tab.
      for (let i = 0; i < n; i++) {
        await tabs.nth(i).click();
        await page.waitForTimeout(150);
        expect(await enhancedContrast(page), `${app} high contrast, tab ${i}`).toEqual([]);
      }
      await page.evaluate(() => document.documentElement.setAttribute('data-theme', 'light'));
      for (let i = 0; i < n; i++) {
        await tabs.nth(i).click();
        await expectAxe(page, `${app} ribbon tab ${await tabs.nth(i).textContent()}`, {
          include: ['.kiln-ribbon'],
        });
      }
    });

    test(`${app}: every Backstage page and every Options page passes axe`, async ({ page }) => {
      await openApp(page, app);
      await page.getByRole('button', { name: 'File' }).click();
      const nav = page.locator('.kiln-backstage__nav-item[data-backstage]');
      await expect(nav.first()).toBeVisible();
      const pages = await nav.evaluateAll((els) =>
        els
          .map((e) => e.getAttribute('data-backstage') ?? '')
          .filter((id) => !['save', 'close', 'options'].includes(id)),
      );
      expect(pages.length).toBeGreaterThan(5);
      for (const id of pages) {
        await page.locator(`.kiln-backstage__nav-item[data-backstage="${id}"]`).click();
        await page.waitForTimeout(150);
        await expectAxe(page, `${app} Backstage ${id}`);
      }
      await page.locator('.kiln-backstage__nav-item[data-backstage="options"]').click();
      const options = page.getByRole('dialog', { name: /Options/ });
      await expect(options).toBeVisible();
      const sections = options.locator('.kiln-options__nav-item');
      const count = await sections.count();
      for (let i = 0; i < count; i++) {
        await sections.nth(i).click();
        await expectAxe(page, `${app} Options ${await sections.nth(i).textContent()}`);
      }
    });
  }

  test('write: every dialog and pane the commands open passes axe', async ({ page }) =>
    dialogsPassAxe(page, 'write'));
  test('grid: every dialog and pane the commands open passes axe', async ({ page }) =>
    dialogsPassAxe(page, 'grid'));
  test('deck: every dialog and pane the commands open passes axe', async ({ page }) =>
    dialogsPassAxe(page, 'deck'));

  test('write: catalog commands are reached from the keyboard, with focus shown at every step', async ({
    page,
  }) => keyboardReach(page, 'write'));
  test('grid: catalog commands are reached from the keyboard, with focus shown at every step', async ({
    page,
  }) => keyboardReach(page, 'grid'));
  test('deck: catalog commands are reached from the keyboard, with focus shown at every step', async ({
    page,
  }) => keyboardReach(page, 'deck'));

  test('Key Tips: unique, prefix-free letters on every tab and menu, Office letters kept, audit up to date', async ({
    page,
  }) => {
    // Wide enough that no ribbon group folds into a single button: at a narrower width whether a
    // group folds depends on how each machine's text rendering rounds the labels' widths, and the
    // audit records the tips of what is shown.
    await page.setViewportSize({ width: 1920, height: 1080 });
    const audit: Record<string, KeyTipsAuditTab[]> = {};
    const problems: string[] = [];
    for (const app of EDITORS) {
      const declared = declaredTips(app);
      const tabsDone = new Map<string, KeyTipsAuditTab>();
      for (const context of CONTEXTS[app]) {
        await context.setup(page);
        await page
          .getByRole('main')
          .click({ position: { x: 5, y: 5 } })
          .catch(() => undefined);
        await key(page, 'Escape');
        await key(page, 'Alt');
        await expect(page.getByTestId('keytips')).toBeVisible();
        const root = await tipTargets(page, 'root');
        const rootTips = root.map((r) => r.tip);
        if (new Set(rootTips).size !== rootTips.length || !prefixFree(rootTips))
          problems.push(`${app} root: ${rootTips.join(' ')}`);
        const tabs = await page.evaluate(() =>
          [...document.querySelectorAll<HTMLElement>('[data-keytip-enter^="tab:"]')]
            .filter((e) => e.getBoundingClientRect().width > 0)
            .map((e) => ({
              id: (e.dataset.keytipEnter ?? '').slice(4),
              tip: e.dataset.keytip ?? '',
              label: (e.textContent ?? '').trim(),
            })),
        );
        await key(page, 'Escape');
        for (const tab of tabs) {
          if (tabsDone.has(tab.id)) continue;
          await key(page, 'Alt');
          await typeTip(page, tab.tip);
          const level = `tab:${tab.id}`;
          const targets = await tipTargets(page, level);
          const tips = targets.map((t) => t.tip);
          const shown = await badges(page);
          if (new Set(tips).size !== tips.length || !prefixFree(tips))
            problems.push(`${app} ${tab.label}: duplicate or prefix tips ${tips.join(' ')}`);
          if (shown.length !== targets.length)
            problems.push(`${app} ${tab.label}: ${targets.length} targets, ${shown.length} badges`);
          // Office's letters (the ribbon definition's) are the ones shown.
          for (const d of declared.get(tab.id) ?? []) {
            const mine = targets.filter((t) =>
              d.command ? t.command === d.command : t.command === null && t.label === d.label,
            );
            if (mine.length > 0 && !mine.some((t) => t.tip === d.tip))
              problems.push(
                `${app} ${tab.label}: ${d.command ?? d.label} shows ${mine.map((t) => t.tip).join('/')}, not ${d.tip}`,
              );
          }
          const titles = await page.evaluate(
            (ids) =>
              ids.map((i) =>
                i
                  ? ((window as unknown as W).__kiln?.registry.list({}).find((c) => c.id === i)?.title ??
                    null)
                  : null,
              ),
            targets.map((t) => t.command),
          );
          const items: KeyTipsAuditItem[] = targets.map((t, i) => ({
            // The command's own name (Format Painter, not the ribbon's short Format).
            label: titles[i] ?? t.label,
            command: t.command,
            tip: t.tip,
            source:
              (declared.get(tab.id) ?? []).some(
                (d) =>
                  d.tip === t.tip &&
                  (t.command ? d.command === t.command : d.command === null && d.label === t.label),
              ) &&
              !t.tip.startsWith('Z') &&
              !KILN_LETTERS.has(t.command ?? '')
                ? 'office'
                : 'kiln',
          }));
          tabsDone.set(tab.id, { label: tab.label, tip: tab.tip, items });
          // Every menu of the tab: its own Key Tips.
          for (const m of targets.filter((t) => t.menu)) {
            await typeTip(page, m.tip);
            const menu = page.locator('[role="menu"]').last();
            if (!(await menu.isVisible().catch(() => false))) {
              problems.push(`${app} ${tab.label} ▸ ${m.label}: Key Tip ${m.tip} opens no menu`);
              continue;
            }
            const inMenu = await badges(page);
            const count = await menu
              .locator('[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]')
              .evaluateAll(
                (els) => els.filter((e) => (e as HTMLElement).dataset.disabled === undefined).length,
              );
            if (
              (inMenu.length === 0 && count > 0) ||
              new Set(inMenu).size !== inMenu.length ||
              !prefixFree(inMenu)
            )
              problems.push(`${app} ${tab.label} ▸ ${m.label}: menu tips ${inMenu.join(' ')}`);
            if (inMenu.length !== count)
              problems.push(`${app} ${tab.label} ▸ ${m.label}: ${count} items, ${inMenu.length} badges`);
            await key(page, 'Escape');
          }
          await key(page, 'Escape');
          await key(page, 'Escape');
          if (
            await page
              .getByTestId('keytips')
              .isVisible()
              .catch(() => false)
          )
            await key(page, 'Escape');
        }
      }
      audit[app] = [...tabsDone.values()];
    }
    expect(problems, problems.join('\n')).toEqual([]);
    const file = repo('packages/commands/src/keytips-audit.json');
    const text = `${JSON.stringify(audit, null, 2)}\n`;
    if (process.env.KILN_UPDATE_KEYTIPS === '1') writeFileSync(file, text);
    expect(
      readFileSync(file, 'utf8'),
      'KILN_UPDATE_KEYTIPS=1 and `pnpm --filter @kiln/commands shortcuts:doc` record the audit',
    ).toBe(text);
  });

  // ── The Accessible text view ──────────────────────────────────────────────────────────────

  async function paragraphTexts(page: Page): Promise<string[]> {
    const doc = await docOf(page, 'write');
    return page.evaluate(async (d) => {
      const r = (await (window as unknown as W).__kiln?.client?.query(d, 'Paragraphs')) as {
        Paragraphs?: { text: string }[];
      };
      return (r.Paragraphs ?? []).map((p) => p.text);
    }, doc);
  }

  test('Write text view: semantic DOM of the document, edited through the commands, in step with the canvas', async ({
    page,
  }) => {
    await openFixture(page, 'write', 'fixtures/docx/features/numbering.docx');
    // View ▸ Accessibility ▸ Text View, by its Key Tips (Alt, W, T, V).
    await page.getByRole('main').click({ position: { x: 5, y: 5 } });
    await key(page, 'Alt');
    await typeTip(page, 'W');
    await typeTip(page, 'TV');
    const view = page.getByTestId('text-view');
    await expect(view).toBeVisible();
    const doc = view.locator('article[contenteditable="true"]');
    await expect(doc).toBeFocused();
    await expect(view.getByRole('list').first()).toBeVisible();
    expect(await view.getByRole('listitem').count()).toBeGreaterThan(3);
    await expectAxe(page, 'Write text view', { include: ['[data-testid="text-view"]'] });
    await page.screenshot({ path: shot('write-text-view-light.png') });
    // Tables: header cells and data cells.
    await openFixture(page, 'write', 'fixtures/docx/features/tables.docx');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    await expect(page.getByTestId('text-view').getByRole('table').first()).toBeVisible();
    expect(await page.getByTestId('text-view').getByRole('cell').count()).toBeGreaterThan(3);
    await expectAxe(page, 'Write text view (tables)', { include: ['[data-testid="text-view"]'] });
    // Editing: a new document, typed into the view.
    await openApp(page, 'write');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    const article = page.getByTestId('text-view').locator('article');
    await expect(article).toBeFocused();
    await page.keyboard.type('Kiln heading');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Body textx');
    await page.keyboard.press('Backspace');
    await expect.poll(() => paragraphTexts(page)).toEqual(['Kiln heading', 'Body text']);
    await expect(article.locator('p')).toHaveText(['Kiln heading', 'Body text']);
    // A shortcut acts on the view's selection: Shift+Home selects "Body text", Ctrl+B makes it bold.
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Control+b');
    await expect
      .poll(() => page.evaluate(() => (window as unknown as W).__kiln?.log().map((e) => e.id) ?? []))
      .toContain('suite.format.bold');
    const format = await page.evaluate(async () => {
      const k = (window as unknown as W).__kiln;
      const w = (window as unknown as W).__kilnWrite;
      const r = (await k?.client?.query(w?.doc ?? 0, { Write: 'Format' })) as {
        Write?: { Format?: { bold?: boolean } };
      };
      return r.Write?.Format?.bold ?? null;
    });
    expect(format).toBe(true);
    // Heading 1 from the keyboard (Ctrl+Alt+1) on the first paragraph: the view shows a heading.
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+Alt+1');
    await expect(article.getByRole('heading', { name: 'Kiln heading' })).toBeVisible();
    // In step with the canvas: closing the view leaves the same document and caret.
    await page.getByRole('button', { name: 'Close Text View' }).click();
    await expect(page.getByTestId('text-view')).toBeHidden();
    expect(await paragraphTexts(page)).toEqual(['Kiln heading', 'Body text']);
  });

  test('Grid text view: the sheet as a table with row and column headers, cells edited with Enter', async ({
    page,
  }) => {
    await openApp(page, 'grid');
    await core(page, 'grid', { Grid: { SelectCell: { row: 0, col: 0, extend: false, add: false } } });
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    const view = page.getByTestId('text-view');
    const a1 = view.locator('td[tabindex="0"]');
    await expect(a1).toBeFocused();
    await expect(view.getByRole('columnheader', { name: 'A' })).toBeVisible();
    await expect(view.getByRole('rowheader', { name: '1' })).toBeVisible();
    await page.keyboard.type('42');
    await page.keyboard.press('Enter');
    // Enter saves the entry and moves down, as in the sheet.
    await expect(view.locator('td[tabindex="0"]')).toHaveAttribute('aria-label', 'A2, blank');
    await expect(view.locator('td[tabindex="0"]')).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(view.locator('td[tabindex="0"]')).toHaveText('42');
    await expect(view.locator('td[tabindex="0"]')).toHaveAttribute('aria-label', 'A1, 42');
    await expect(view.locator('td[tabindex="0"]')).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(view.locator('td[tabindex="0"]')).toHaveAttribute('aria-label', 'B1, blank');
    await expect(view.locator('td[tabindex="0"]')).toBeFocused();
    await page.keyboard.press('F2');
    await expect(view.getByRole('textbox', { name: 'Edit B1' })).toBeFocused();
    await page.keyboard.type('=A1*2');
    await page.keyboard.press('Enter');
    await expect(view.getByRole('cell', { name: 'B1, 84' })).toBeVisible();
    await expectAxe(page, 'Grid text view', { include: ['[data-testid="text-view"]'] });
    await page.screenshot({ path: shot('grid-text-view-light.png') });
  });

  test('Deck text view: slides as sections with their shapes and notes, text typed into a shape', async ({
    page,
  }) => {
    await openFixture(page, 'deck', 'fixtures/pptx/features/notes.pptx');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    const view = page.getByTestId('text-view');
    const slides = view.locator('section.kiln-textview__slide');
    await expect.poll(() => slides.count()).toBeGreaterThan(1);
    await expect(slides.first().getByRole('heading', { level: 2 })).toContainText('Slide 1 of 3');
    await expect(view.getByRole('region', { name: 'Notes' }).first()).toBeVisible();
    await expectAxe(page, 'Deck text view', { include: ['[data-testid="text-view"]'] });
    await page.screenshot({ path: shot('deck-text-view-light.png') });
    // Typing at the end of the first text shape changes the slide.
    const shape = view.locator('[data-kiln-shape]').first();
    await shape.focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.type(' typed');
    await expect.poll(async () => (await shape.textContent()) ?? '').toContain(' typed');
    const nodes = await treeNodes(page, 'deck', 'A11yDocument');
    expect(nodes.some((n) => (n.text ?? '').includes(' typed'))).toBe(true);
    // The keys stay in the shape across the redraw: type, wait, type again.
    await page.waitForTimeout(1000);
    await expect(shape).toBeFocused();
    await page.keyboard.press('End');
    await page.keyboard.type('z');
    await page.waitForTimeout(1000);
    await expect(shape).toBeFocused();
    await page.keyboard.type('q');
    await expect.poll(async () => (await shape.textContent()) ?? '').toContain('zq');
    await expect(shape).toBeFocused();
    // A new presentation: the empty title and subtitle placeholders take text.
    await openApp(page, 'deck');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    const boxes = page.getByTestId('text-view').locator('[data-kiln-shape]');
    await expect.poll(() => boxes.count()).toBeGreaterThanOrEqual(2);
    // Each step waits for the core to hold the text and the view to show it, not for time.
    const coreTexts = async () => (await treeNodes(page, 'deck', 'A11yDocument')).map((n) => n.text ?? '');
    await boxes.nth(0).focus();
    await expect(boxes.nth(0)).toBeFocused();
    await page.keyboard.type('Quarterly');
    await expect.poll(coreTexts, { timeout: 20_000 }).toContain('Quarterly');
    await expect(boxes.nth(0)).toHaveText('Quarterly');
    // The keys stay in the title across its redraw.
    await expect(boxes.nth(0)).toBeFocused();
    await page.keyboard.type(' review');
    await expect.poll(coreTexts, { timeout: 20_000 }).toContain('Quarterly review');
    await expect(boxes.nth(0)).toHaveText('Quarterly review');
    await boxes.nth(1).focus();
    await expect(boxes.nth(1)).toBeFocused();
    await page.keyboard.type('Kiln team');
    await expect.poll(coreTexts, { timeout: 20_000 }).toContain('Kiln team');
    await expect(boxes.nth(1)).toHaveText('Kiln team');
    await expect(boxes.nth(0)).toHaveText('Quarterly review');
    await expect(boxes.nth(1)).toBeFocused();
  });

  test('Write text view: tracked changes are insertions and deletions, as the review display shows them', async ({
    page,
  }) => {
    await openFixture(page, 'write', 'fixtures/docx/features/tracked-changes.docx');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.view.text_view'));
    const view = page.getByTestId('text-view');
    const article = view.locator('article');
    await expect(article).toBeVisible();
    const mode = async (m: string) => {
      await core(page, 'write', { Write: { MarkupMode: { mode: m } } });
    };
    // Simple Markup (the default): the text after the changes, deletions not shown.
    await expect.poll(() => article.locator('[data-kiln-hidden]').count()).toBeGreaterThan(0);
    expect(await article.locator('del').count()).toBe(0);
    // All Markup: insertions <ins>, deletions <del>.
    await mode('All');
    await expect.poll(() => article.locator('ins').count()).toBeGreaterThan(0);
    await expect.poll(() => article.locator('del').count()).toBeGreaterThan(0);
    await expectAxe(page, 'Write text view (tracked changes)', { include: ['[data-testid="text-view"]'] });
    await page.screenshot({ path: shot('write-text-view-tracked.png') });
    // Offsets stay the model's: the deleted text is still counted, so the caret placed after a
    // deletion lands where the canvas puts it.
    const del = article.locator('del').first();
    const delText = (await del.textContent()) ?? '';
    expect(delText.length).toBeGreaterThan(0);
    // Original hides insertions and shows deletions as text; No Markup hides deletions.
    await mode('Original');
    await expect.poll(() => article.locator('ins').count()).toBe(0);
    await expect.poll(() => article.locator('del').count()).toBe(0);
    expect(await article.locator('[data-kiln-hidden]').count()).toBeGreaterThan(0);
    expect(await article.innerText()).toContain(delText);
    await mode('None');
    await expect.poll(() => article.locator('del').count()).toBe(0);
    expect(await article.locator('[data-kiln-hidden]').count()).toBeGreaterThan(0);
    await mode('All');
    await expect.poll(() => article.locator('del').count()).toBeGreaterThan(0);
  });

  // ── Announcements, motion, focus order, contrast ──────────────────────────────────────────

  async function announced(page: Page): Promise<string> {
    return page.evaluate(() =>
      [...document.querySelectorAll('[data-kiln-announcer], [aria-live]')]
        .map((e) => e.textContent ?? '')
        .join(' | '),
    );
  }

  test('live announcements: Grid modes and cell moves, slide changes, selections', async ({ page }) => {
    await openApp(page, 'grid');
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.type('7');
    await expect.poll(() => announced(page)).toContain('Enter mode');
    await page.keyboard.press('F2');
    await expect.poll(() => announced(page)).toContain('Edit mode');
    await page.keyboard.press('Enter');
    await expect.poll(() => announced(page)).toContain('Ready mode');
    await page.keyboard.press('ArrowUp');
    await expect.poll(() => announced(page)).toContain('A1, 7');
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => announced(page)).toContain('B1, blank');
    await page.keyboard.press('Shift+ArrowDown');
    await expect.poll(() => announced(page)).toContain('B1 to B2 selected');
    // Deck: the slide shown, the slide show.
    await openApp(page, 'deck');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('deck.slides.new_slide'));
    await expect.poll(() => announced(page)).toContain('slide 2 of 2');
    // Write: selected text.
    await openApp(page, 'write');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Hello world');
    await page.keyboard.press('Shift+Home');
    await expect.poll(() => announced(page)).toContain('Hello world, selected');
  });

  test('reduced motion: the chrome does not animate, slide shows play transitions as fades', async ({
    page,
  }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await openApp(page, 'deck');
    const durations = await page.evaluate(() => {
      const probe = document.querySelector('.kiln-ribbon__tab, .kiln-btn') as HTMLElement | null;
      const cs = probe ? getComputedStyle(probe) : null;
      return {
        transition: cs?.transitionDuration ?? '',
        motion: getComputedStyle(document.documentElement).getPropertyValue('--kiln-motion-base').trim(),
      };
    });
    expect(durations.motion).toBe('0ms');
    expect(durations.transition.split(',').every((d) => Number.parseFloat(d) === 0)).toBe(true);
    await page.getByRole('button', { name: 'File' }).click();
    const anims = await page.evaluate(
      () => document.getAnimations().filter((a) => a.playState === 'running').length,
    );
    expect(anims).toBe(0);
    await page.keyboard.press('Escape');
    // The show starts with reduced motion on (the core turns transitions into fades).
    await page.evaluate(() => {
      const c = (window as unknown as W).__kiln?.client as unknown as {
        execute: (d: unknown, c: unknown) => Promise<unknown>;
      };
      const run = c.execute.bind(c);
      const sent: string[] = [];
      (window as unknown as { __shows: string[] }).__shows = sent;
      c.execute = (d, cmd) => {
        const t = JSON.stringify(cmd);
        if (t.includes('ShowStart')) sent.push(t);
        return run(d, cmd);
      };
    });
    // Transitions ▸ Preview plays the slide's transition as the show would.
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('deck.transitions.preview'));
    await expect
      .poll(() => page.evaluate(() => (window as unknown as { __shows?: string[] }).__shows ?? []))
      .toContainEqual(expect.stringContaining('"reduced_motion":true'));
  });

  test('focus order: F6 cycles document, panes, status bar, title bar and ribbon; Shift+F6 goes back', async ({
    page,
  }) => {
    await openApp(page, 'write');
    await page.evaluate(() =>
      (window as unknown as W).__kiln?.registry.execute('suite.view.navigation_pane'),
    );
    await page.getByTestId('kiln-input-proxy').focus();
    const regions: string[] = [];
    for (let i = 0; i < 5; i++) {
      await key(page, 'F6');
      regions.push(
        await page.evaluate(() => {
          const a = document.activeElement;
          if (a?.closest('.kiln-ribbon')) return 'ribbon';
          if (a?.closest('.kiln-titlebar')) return 'titlebar';
          if (a?.closest('.kiln-statusbar')) return 'statusbar';
          if (a?.closest('.kiln-appframe__body aside, .kiln-appframe__body [role="complementary"]'))
            return 'pane';
          if (a?.closest('.kiln-appframe__canvas')) return 'document';
          if (a?.closest('.kiln-appframe__body')) return 'pane';
          return a?.tagName ?? 'none';
        }),
      );
      const s = await stop(page);
      expect(s?.ring && s.visible, `F6 to ${regions.at(-1)} shows the focus`).toBe(true);
    }
    expect(regions).toEqual(['pane', 'statusbar', 'titlebar', 'ribbon', 'document']);
    await key(page, 'Shift+F6');
    expect(await page.evaluate(() => !!document.activeElement?.closest('.kiln-ribbon'))).toBe(true);
  });

  async function f6Regions(page: Page, n: number): Promise<string[]> {
    const regions: string[] = [];
    for (let i = 0; i < n; i++) {
      await key(page, 'F6');
      regions.push(
        await page.evaluate(() => {
          const a = document.activeElement;
          if (a?.closest('.kiln-ribbon')) return 'ribbon';
          if (a?.closest('.kiln-titlebar')) return 'titlebar';
          if (a?.closest('.kiln-statusbar')) return 'statusbar';
          if (a?.closest('[data-kiln-region="pane"], .kiln-appframe__canvas aside')) return 'pane';
          if (a?.closest('.kiln-appframe__canvas')) return 'document';
          if (a?.closest('.kiln-appframe__body')) return 'pane';
          return a?.tagName ?? 'none';
        }),
      );
      const s = await stop(page);
      expect(s?.ring && s.visible, `F6 to ${regions.at(-1)} shows the focus`).toBe(true);
    }
    return regions;
  }

  test('focus order: F6 in Grid cycles sheet, status bar, title bar and ribbon', async ({ page }) => {
    await openApp(page, 'grid');
    await page.locator('[data-kiln-grid-proxy]').focus();
    const regions = await f6Regions(page, 4);
    expect(regions).toEqual(['statusbar', 'titlebar', 'ribbon', 'document']);
    await key(page, 'Shift+F6');
    expect(await page.evaluate(() => !!document.activeElement?.closest('.kiln-ribbon'))).toBe(true);
  });

  test('focus order: F6 in Deck cycles slide, thumbnails, notes, status bar, title bar and ribbon', async ({
    page,
  }) => {
    await openApp(page, 'deck');
    await page.evaluate(() =>
      (window as unknown as { __kilnDeck?: { stage?: HTMLElement } }).__kilnDeck?.stage?.focus(),
    );
    // The slide thumbnails and the speaker notes are stops of their own, as in PowerPoint.
    const regions = await f6Regions(page, 6);
    expect(regions).toEqual(['pane', 'pane', 'statusbar', 'titlebar', 'ribbon', 'document']);
    await key(page, 'Shift+F6');
    expect(await page.evaluate(() => !!document.activeElement?.closest('.kiln-ribbon'))).toBe(true);
  });

  test('slide show announcements: the slide shown, black and white screens, the end of the show', async ({
    page,
  }) => {
    await openFixture(page, 'deck', 'fixtures/pptx/features/notes.pptx');
    await page.evaluate(() =>
      (window as unknown as { __kilnDeck?: { stage?: HTMLElement } }).__kilnDeck?.stage?.focus(),
    );
    await page.keyboard.press('F5');
    const show = page.getByTestId('deck-show');
    await expect(show).toBeVisible({ timeout: 15_000 });
    await expect.poll(() => announced(page)).toContain('Slide 1 of 3');
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => announced(page)).toContain('Slide 2 of 3');
    await page.keyboard.press('b');
    await expect.poll(() => announced(page)).toContain('Black screen');
    await page.keyboard.press('b');
    await page.keyboard.press('w');
    await expect.poll(() => announced(page)).toContain('White screen');
    await page.keyboard.press('w');
    await page.keyboard.press('End');
    await expect.poll(() => announced(page)).toContain('Slide 3 of 3');
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => announced(page)).toContain('End of slide show');
    await page.keyboard.press('Escape');
  });

  /**
   * WCAG 1.4.12's text spacing (line height 1.5, paragraph spacing 2, letter spacing 0.12, word
   * spacing 0.16) on the frame: text cut off by its box, in the title bar, the ribbon's tabs and
   * Home tab, the status bar and the Font dialog. A picture of text (a style's sample) may be cut.
   */
  async function clippedText(page: Page, scope: string): Promise<string[]> {
    return page.evaluate((sel) => {
      const out: string[] = [];
      for (const root of document.querySelectorAll<HTMLElement>(sel)) {
        for (const el of root.querySelectorAll<HTMLElement>('*')) {
          if (el.closest('[aria-hidden="true"], .kiln-gallery-sample, [hidden], .kiln-visually-hidden'))
            continue;
          const own = [...el.childNodes].some((n) => n.nodeType === 3 && (n.textContent ?? '').trim());
          if (!own) continue;
          const r = el.getBoundingClientRect();
          if (r.width === 0 || r.height === 0) continue;
          const cs = getComputedStyle(el);
          const clips = (v: string) => v === 'hidden' || v === 'clip';
          const cutX = el.scrollWidth > el.clientWidth + 1 && clips(cs.overflowX);
          const cutY = el.scrollHeight > el.clientHeight + 1 && clips(cs.overflowY);
          if (cutX || cutY)
            out.push(`${el.className || el.tagName}: "${(el.textContent ?? '').trim().slice(0, 40)}"`);
        }
      }
      return out;
    }, scope);
  }

  const TEXT_SPACING =
    '* { line-height: 1.5 !important; letter-spacing: 0.12em !important; word-spacing: 0.16em !important; } p { margin-bottom: 2em !important; }';

  test('text spacing (1.4.12): no text of the frame or a dialog is cut off with WCAG spacing', async ({
    page,
  }) => {
    for (const app of APPS) {
      await openApp(page, app);
      await page.addStyleTag({ content: TEXT_SPACING });
      await page.waitForTimeout(400);
      const cut = await clippedText(page, '.kiln-titlebar, .kiln-ribbon, .kiln-statusbar');
      expect(cut, `${app} frame`).toEqual([]);
      await page.screenshot({ path: shot(`${app}-text-spacing.png`) });
      await page.evaluate(() =>
        (window as unknown as W).__kiln?.registry.execute('suite.format.font_dialog'),
      );
      await page.waitForTimeout(400);
      if (await page.locator('[role="dialog"]').count()) {
        expect(await clippedText(page, '[role="dialog"]'), `${app} Font dialog`).toEqual([]);
        await key(page, 'Escape');
      }
    }
  });

  test('resize text (1.4.4): at 200 % zoom the frame works without sideways scrolling', async ({
    browser,
  }) => {
    // 200 % zoom of a 1440 × 900 window is a 720 × 450 CSS viewport.
    const page = await browser.newPage({ viewport: { width: 720, height: 450 }, deviceScaleFactor: 2 });
    for (const app of APPS) {
      await openApp(page, app);
      const wide = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
      expect(wide, `${app}: no sideways scroll`).toBeLessThanOrEqual(1);
      // The ribbon's commands, by Key Tips, and its text not cut off.
      await key(page, 'Alt');
      await typeTip(page, 'H');
      await page.waitForTimeout(200);
      expect(await page.locator('[data-keytip-badge]').count(), `${app}: Key Tips on Home`).toBeGreaterThan(
        5,
      );
      await key(page, 'Escape');
      await key(page, 'Escape');
      expect(await clippedText(page, '.kiln-titlebar, .kiln-statusbar'), `${app} at 200 %`).toEqual([]);
      // A dialog fits the window or scrolls inside it.
      await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.options.open'));
      const dialog = page.locator('[role="dialog"]').first();
      await expect(dialog).toBeVisible();
      const fits = await dialog.evaluate((d) => {
        const r = d.getBoundingClientRect();
        return r.left >= 0 && r.right <= innerWidth + 1 && r.top >= 0 && r.bottom <= innerHeight + 1;
      });
      expect(fits, `${app}: Options fits the window`).toBe(true);
      await page.screenshot({ path: shot(`${app}-zoom-200.png`) });
      await key(page, 'Escape');
    }
    await page.close();
  });

  test('high contrast: the theme and Windows contrast themes (forced colors) keep text, focus and selection visible', async ({
    page,
  }) => {
    await page.emulateMedia({ forcedColors: 'active', colorScheme: 'dark' });
    for (const app of EDITORS) {
      await openApp(page, app);
      // Following the system, the high-contrast theme applies.
      expect(await page.evaluate(() => document.documentElement.dataset.theme)).toBe('hc');
      await expectAxe(page, `${app} forced colors`);
      await key(page, 'F6');
      const s = await stop(page);
      expect(s?.ring, 'focus outline in forced colors').toBe(true);
      await page.screenshot({ path: shot(`${app}-forced-colors.png`) });
    }
    await page.emulateMedia({ forcedColors: 'none', colorScheme: 'light' });
    await openApp(page, 'write');
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.theme.high_contrast'));
    await expect.poll(() => page.evaluate(() => document.documentElement.dataset.theme)).toBe('hc');
    await expectAxe(page, 'Write high contrast');
    expect(await enhancedContrast(page), 'Write high contrast, 7:1').toEqual([]);
    await page.screenshot({ path: shot('write-high-contrast.png') });
    await page.evaluate(() => (window as unknown as W).__kiln?.registry.execute('ui.theme.system'));
  });
});
