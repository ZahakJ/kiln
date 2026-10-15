import { describe, expect, it } from 'vitest';
import { DESCRIPTIONS } from './catalog/descriptions.ts';
import { COMMAND_CATALOG } from './catalog/index.ts';
import { DEFAULT_KEYMAP_LAYERS } from './keymaps/index.ts';
import { createOfficeSynonyms } from './synonyms.ts';

describe('command catalog', () => {
  it('declares unique, well-formed ids with titles', () => {
    const ids = COMMAND_CATALOG.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.length).toBeGreaterThan(350);
    for (const c of COMMAND_CATALOG) {
      expect(c.id).toMatch(/^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9]+)+$/);
      expect(c.title.trim()).not.toBe('');
    }
  });

  it('gives every palette-visible command an icon', () => {
    const missing = COMMAND_CATALOG.filter((c) => c.palette !== false && !c.icon).map((c) => c.id);
    expect(missing).toEqual([]);
  });

  it('declares every command the keymaps and synonyms refer to', () => {
    const ids = new Set(COMMAND_CATALOG.map((c) => c.id));
    for (const [layer, bindings] of Object.entries(DEFAULT_KEYMAP_LAYERS)) {
      for (const b of bindings ?? []) expect(ids.has(b.command), `${layer}: ${b.command}`).toBe(true);
    }
    for (const id of Object.keys(createOfficeSynonyms())) expect(ids.has(id), id).toBe(true);
  });

  it('attaches ScreenTip descriptions to declared commands', () => {
    const ids = new Set(COMMAND_CATALOG.map((c) => c.id));
    for (const id of Object.keys(DESCRIPTIONS)) expect(ids.has(id), id).toBe(true);
    expect(COMMAND_CATALOG.find((c) => c.id === 'format.bold')?.description).toBe('Make your text bold.');
  });

  it('marks the tiers of §1.4 (P1 Goal Seek, P2 Slicer)', () => {
    const tier = (id: string) => COMMAND_CATALOG.find((c) => c.id === id)?.tier ?? 'P0';
    expect(tier('grid.goalSeek')).toBe('P1');
    expect(tier('grid.slicer')).toBe('P2');
    expect(tier('grid.freezePanes')).toBe('P0');
  });
});
