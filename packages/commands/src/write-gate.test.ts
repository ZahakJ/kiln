/**
 * The Write gate: no P0 or P1 ribbon item, keymap binding or Key Tip of Kiln Write is a
 * reservation any more. Each resolves to a command of `catalog.json` (the core implements it), a
 * `ui.*` chrome action, or an id the Write editor implements in its chrome
 * (`WRITE_CHROME_COMMANDS`, whose registration `@kiln/editor-write` tests). Items whose parity
 * row is P2 carry `tier: 'P2'` and stay hidden in release builds.
 */
import { describe, expect, it } from 'vitest';
import { COMMAND_CATALOG, RIBBONS } from './catalog/index.ts';
import {
  COLLAB_CHROME_COMMANDS,
  FRAME_CHROME_COMMANDS,
  WRITE_CHROME_COMMANDS,
} from './catalog/write-office.ts';
import { idKind } from './core/catalog.ts';
import { DEFAULT_KEYMAP_LAYERS } from './keymaps/index.ts';
import type { RibbonMenuEntry } from './ribbon.ts';
import type { Tier } from './types.ts';

const DECLARED = new Map(COMMAND_CATALOG.map((c) => [c.id, c]));

/**
 * P0 rows of collaboration and version history, which the gate's P0 check left out: none now,
 * the frame implements Share and Version History (`COLLAB_CHROME_COMMANDS`).
 */
const COLLAB_ROWS: readonly string[] = [];

interface Ref {
  id: string;
  tier: Tier;
  where: string;
}

function menu(entries: readonly RibbonMenuEntry[], tier: Tier, where: string, out: Ref[]): void {
  for (const e of entries) {
    if ('command' in e && e.command)
      out.push({ id: e.command, tier: DECLARED.get(e.command)?.tier ?? tier, where });
    if (e.kind === 'submenu') menu(e.items, tier, `${where} ▸ ${e.label}`, out);
  }
}

/** Every command id of Write's ribbon with its effective tier. */
function ribbonRefs(): Ref[] {
  const out: Ref[] = [];
  const spec = RIBBONS.write;
  if (!spec) throw new Error('no Write ribbon');
  for (const t of spec.tabs) {
    for (const g of t.groups) {
      const groupTier: Tier = g.tier ?? t.tier ?? 'P0';
      const where = `${t.label} ▸ ${g.label}`;
      if (g.launcher) out.push({ id: g.launcher.command, tier: groupTier, where: `${where} ↘` });
      for (const it of g.items.flatMap((i) => (i.kind === 'row' ? [...i.items] : [i]))) {
        if (it.kind === 'spacer') continue;
        const itemTier: Tier = ('tier' in it && it.tier) || groupTier;
        if ('command' in it && it.command) {
          const decl = DECLARED.get(it.command)?.tier;
          // The stricter of the item's and the declaration's tier hides it.
          const tier: Tier =
            decl === 'P2' || itemTier === 'P2' ? 'P2' : decl === 'P1' || itemTier === 'P1' ? 'P1' : 'P0';
          out.push({ id: it.command, tier, where });
        }
        if (it.kind === 'split' || it.kind === 'dropdown') menu(it.menu, itemTier, where, out);
        if (it.kind === 'gallery' && it.footer) menu(it.footer, itemTier, where, out);
      }
    }
  }
  return out;
}

function keymapRefs(): Ref[] {
  const out: Ref[] = [];
  for (const layer of ['common', 'write'] as const) {
    for (const b of DEFAULT_KEYMAP_LAYERS[layer] ?? []) {
      const d = DECLARED.get(b.command);
      if (d?.apps && !d.apps.includes('write')) continue;
      out.push({ id: b.command, tier: d?.tier ?? 'P0', where: `keymap ${layer} ${b.key}` });
    }
  }
  return out;
}

function resolves(id: string): boolean {
  const kind = idKind(id);
  return (
    kind === 'core' ||
    kind === 'ui' ||
    WRITE_CHROME_COMMANDS.includes(id) ||
    FRAME_CHROME_COMMANDS.includes(id) ||
    COLLAB_CHROME_COMMANDS.includes(id)
  );
}

describe('Write gate: no P0/P1 reservation in Kiln Write', () => {
  it('every P0/P1 ribbon item resolves to a core command or one the Write chrome implements', () => {
    const refs = ribbonRefs();
    expect(refs.length).toBeGreaterThan(250);
    const open = refs
      .filter((r) => r.tier !== 'P2' && !COLLAB_ROWS.includes(r.id) && !resolves(r.id))
      .map((r) => `${r.where}: ${r.id}`);
    expect([...new Set(open)]).toEqual([]);
  });

  it('every P0/P1 keymap binding of Write resolves', () => {
    const open = keymapRefs()
      .filter((r) => r.tier !== 'P2' && !resolves(r.id))
      .map((r) => `${r.where}: ${r.id}`);
    expect(open).toEqual([]);
  });

  it('every Key Tip of a P0/P1 Write ribbon command resolves', () => {
    const ids = new Set(
      ribbonRefs()
        .filter((r) => r.tier !== 'P2')
        .map((r) => r.id),
    );
    const open = COMMAND_CATALOG.filter(
      (c) => c.keyTip && ids.has(c.id) && !COLLAB_ROWS.includes(c.id) && !resolves(c.id),
    ).map((c) => `${c.keyTip}: ${c.id}`);
    expect(open).toEqual([]);
  });

  it('declares every id the Write chrome implements, none of them a core command', () => {
    for (const id of WRITE_CHROME_COMMANDS) {
      expect(DECLARED.has(id), id).toBe(true);
      expect(idKind(id), id).toBe('planned');
    }
  });

  it('leaves only the collaboration rows out', () => {
    const refs = ribbonRefs().filter((r) => r.tier !== 'P2' && !resolves(r.id));
    expect([...new Set(refs.map((r) => r.id))].sort()).toEqual([...COLLAB_ROWS].sort());
  });
});
