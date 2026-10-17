import { describe, expect, it } from 'vitest';
import { createCommandRegistry } from './bootstrap.ts';
import { DESCRIPTIONS } from './catalog/descriptions.ts';
import { COMMAND_CATALOG, RIBBONS } from './catalog/index.ts';
import { CORE_BUILDERS } from './core/build.ts';
import { CORE_CATALOG, idKind, isCoreCommand } from './core/catalog.ts';
import { DEFAULT_KEYMAP_LAYERS } from './keymaps/index.ts';
import { TAB_KEYTIPS } from './keytips.ts';
import { ribbonCommandRefs } from './ribbon.ts';
import { createOfficeSynonyms } from './synonyms.ts';
import { RELEASE_TIERS } from './tiers.ts';
import type { AppId } from './types.ts';

const DECLARED = new Map(COMMAND_CATALOG.map((c) => [c.id, c]));

/** Every command id the chrome refers to, with where it was found. */
function referencedIds(): { id: string; where: string }[] {
  const out: { id: string; where: string }[] = [];
  for (const [app, spec] of Object.entries(RIBBONS)) {
    for (const r of ribbonCommandRefs(spec))
      out.push({ id: r.command, where: `ribbon ${app} ▸ ${r.tab} ▸ ${r.group}` });
  }
  for (const [layer, bindings] of Object.entries(DEFAULT_KEYMAP_LAYERS)) {
    for (const b of bindings ?? []) out.push({ id: b.command, where: `keymap ${layer} ${b.key}` });
  }
  for (const id of Object.keys(createOfficeSynonyms())) out.push({ id, where: 'synonyms' });
  for (const c of COMMAND_CATALOG) if (c.keyTip) out.push({ id: c.id, where: 'key tip' });
  return out;
}

describe('command catalog', () => {
  it('declares unique, well-formed ids with titles', () => {
    const ids = COMMAND_CATALOG.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.length).toBeGreaterThan(350);
    for (const c of COMMAND_CATALOG) {
      expect(c.id).toMatch(/^(suite|write|grid|deck|notes|ui)\.[a-z0-9_]+(\.[a-z0-9_]+)+$/);
      expect(c.title.trim()).not.toBe('');
    }
  });

  it('gives every palette-visible command an icon', () => {
    const missing = COMMAND_CATALOG.filter((c) => c.palette !== false && !c.icon).map((c) => c.id);
    expect(missing).toEqual([]);
  });

  it('attaches ScreenTip descriptions to declared commands', () => {
    for (const id of Object.keys(DESCRIPTIONS)) expect(DECLARED.has(id), id).toBe(true);
    expect(DECLARED.get('suite.format.bold')?.description).toBe('Make your text bold.');
  });

  it('marks the tiers of §1.4 (P1 Goal Seek, P2 Slicer)', () => {
    const tier = (id: string) => DECLARED.get(id)?.tier ?? 'P0';
    expect(tier('grid.data.goal_seek')).toBe('P1');
    expect(tier('grid.insert.slicer')).toBe('P2');
    expect(tier('grid.view.freeze_panes')).toBe('P0');
  });
});

/*
 * WP-58's acceptance names an "every id exists in catalog.json" test. The core's catalog.json
 * (WP-52) lists only the commands the core implements (38 today), so that literal check cannot
 * pass until the core grows; it is an open plan decision (docs/qa/handoffs.md, WP-58 fix round 1).
 * What this suite enforces instead: every referenced id is declared; an id is either a core
 * command from catalog.json, a `ui.*` chrome action, or a reservation for a command the core does
 * not implement yet, and every reservation is hidden in release builds of every app.
 */
describe('every id is declared: catalog.json core commands, chrome actions, or release-hidden reservations', () => {
  it('reads the generated catalog of the core', () => {
    expect(CORE_CATALOG.length).toBeGreaterThanOrEqual(38);
    expect(isCoreCommand('suite.file.save')).toBe(true);
    // WP-64: the Write editor implements the shared formatting commands.
    expect(isCoreCommand('suite.format.bold')).toBe(true);
    expect(isCoreCommand('suite.format.text_effects')).toBe(false);
  });

  it('gives every core command a presentation (title, and an icon when it is in the palette)', () => {
    for (const info of CORE_CATALOG) {
      const decl = DECLARED.get(info.id);
      expect(decl, `${info.id} has no declaration in src/catalog`).toBeDefined();
    }
  });

  it('builds a typed Command for exactly the ids of catalog.json', () => {
    expect(Object.keys(CORE_BUILDERS).sort()).toEqual(CORE_CATALOG.map((c) => c.id).sort());
  });

  it('resolves every ribbon item, keymap binding, Key Tip and synonym to a catalog id', () => {
    const refs = referencedIds();
    expect(refs.length).toBeGreaterThan(900);
    const unknown = refs.filter((r) => !DECLARED.has(r.id)).map((r) => `${r.where}: ${r.id}`);
    expect(unknown).toEqual([]);
    // Every id is a core command (catalog.json), a chrome action, or a reservation for a command
    // the core does not implement yet; reservations never shadow a core id (idKind is exclusive).
    const kinds = new Set(refs.map((r) => idKind(r.id)));
    expect([...kinds].every((k) => k === 'core' || k === 'ui' || k === 'planned')).toBe(true);
    const coreRefs = refs.filter((r) => idKind(r.id) === 'core').map((r) => r.id);
    for (const id of ['suite.file.save', 'suite.edit.undo', 'suite.view.mode', 'write.caret.move']) {
      expect(coreRefs, id).toContain(id);
    }
  });

  it('fails on an unknown id', () => {
    expect(DECLARED.has('suite.format.boldd')).toBe(false);
    expect(() => createCommandRegistry({ app: 'write' }).declare({ id: 'Format.Bold', title: 'x' })).toThrow(
      /Invalid command id/,
    );
  });

  it('hides every reserved (planned) command in release builds of every app', () => {
    const apps: AppId[] = ['write', 'grid', 'deck', 'notes'];
    let checked = 0;
    for (const app of apps) {
      const registry = createCommandRegistry({ app, tiers: RELEASE_TIERS });
      for (const c of COMMAND_CATALOG.filter((d) => idKind(d.id) === 'planned')) {
        expect(registry.getState(c.id).visible, `${app}: ${c.id}`).toBe(false);
        checked++;
      }
    }
    expect(checked).toBeGreaterThan(400);
  });

  it('takes apps and tier of core commands from catalog.json', () => {
    const registry = createCommandRegistry({ app: 'write' });
    const info = CORE_CATALOG.find((c) => c.id === 'suite.chart.move');
    expect(info?.apps).toEqual(['Grid']);
    expect(registry.meta('suite.chart.move')?.apps).toEqual(['grid']);
  });
});

describe('ribbon definitions (PLAN.md §1.4)', () => {
  const apps: AppId[] = ['write', 'grid', 'deck', 'notes'];

  it('defines a ribbon for every editor', () => {
    for (const app of apps) expect(RIBBONS[app]?.app).toBe(app);
  });

  for (const app of apps) {
    it(`${app}: every item is a command the app offers`, () => {
      const spec = RIBBONS[app];
      if (!spec) throw new Error('missing');
      const wrong = ribbonCommandRefs(spec)
        .filter((r) => {
          const decl = DECLARED.get(r.command);
          return decl?.apps && !decl.apps.includes(app);
        })
        .map((r) => `${r.tab} ▸ ${r.group}: ${r.command}`);
      expect(wrong).toEqual([]);
    });

    it(`${app}: tab ids are unique and have Office's Key Tip letters`, () => {
      const spec = RIBBONS[app];
      if (!spec) throw new Error('missing');
      const ids = spec.tabs.map((t) => t.id);
      expect(new Set(ids).size).toBe(ids.length);
      for (const t of spec.tabs) {
        if (t.contextual) continue;
        expect(t.keyTip, t.id).toBe(TAB_KEYTIPS[app][t.id]);
      }
      expect(spec.tabs[0]?.id).toBe('home');
    });
  }

  it('follows §1.4 tab order', () => {
    const order = (app: AppId) => RIBBONS[app]?.tabs.filter((t) => !t.contextual).map((t) => t.label);
    expect(order('write')).toEqual(['Home', 'Insert', 'Layout', 'References', 'Review', 'View']);
    expect(order('grid')).toEqual(['Home', 'Insert', 'Page Layout', 'Formulas', 'Data', 'Review', 'View']);
    expect(order('deck')).toEqual([
      'Home',
      'Insert',
      'Design',
      'Transitions',
      'Animations',
      'Slide Show',
      'Review',
      'View',
    ]);
    expect(order('notes')).toEqual(['Home', 'Insert', 'Draw', 'View']);
  });

  it('puts Merge & Center in Grid ▸ Home ▸ Alignment and Bold at Alt, H, 1', () => {
    const grid = RIBBONS.grid;
    const write = RIBBONS.write;
    if (!grid || !write) throw new Error('missing');
    expect(
      ribbonCommandRefs(grid).some(
        (r) => r.command === 'grid.alignment.merge_center' && r.group === 'alignment',
      ),
    ).toBe(true);
    const font = write.tabs[0]?.groups.find((g) => g.id === 'font');
    const leaves = font?.items.flatMap((i) => (i.kind === 'row' ? i.items : [i])) ?? [];
    const bold = leaves.find((l) => 'command' in l && l.command === 'suite.format.bold');
    expect(bold && 'keyTip' in bold ? bold.keyTip : undefined).toBe('1');
  });
});
