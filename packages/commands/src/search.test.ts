import { describe, expect, it } from 'vitest';
import { COMMAND_CATALOG } from './catalog/index.ts';
import { RecentCommands } from './recent.ts';
import { fuzzyScore, searchCommands } from './search.ts';
import { MemoryStorage } from './storage.ts';
import { createOfficeSynonyms } from './synonyms.ts';

const withSynonyms = () => {
  const syn = createOfficeSynonyms();
  return COMMAND_CATALOG.map((c) => ({ ...c, synonyms: [...(c.synonyms ?? []), ...(syn[c.id] ?? [])] }));
};

describe('fuzzy search', () => {
  it('scores exact > prefix > word prefixes > substring > subsequence > none', () => {
    const exact = fuzzyScore('bold', 'Bold');
    const prefix = fuzzyScore('fre', 'Freeze Panes');
    const words = fuzzyScore('panes freeze', 'Freeze Panes');
    const sub = fuzzyScore('eeze', 'Freeze Panes');
    const subseq = fuzzyScore('frzpn', 'Freeze Panes');
    expect(exact).toBe(1);
    expect(prefix).toBeLessThan(exact);
    expect(words).toBeLessThan(prefix);
    expect(sub).toBeLessThan(words);
    expect(subseq).toBeGreaterThan(0);
    expect(subseq).toBeLessThan(sub);
    expect(fuzzyScore('xyz', 'Freeze Panes')).toBe(0);
    expect(fuzzyScore('merge & center', 'Merge and Center')).toBe(1);
  });

  it('finds Office terms through titles, Office names and synonyms', () => {
    const cmds = withSynonyms();
    const top = (q: string) => searchCommands(cmds, q, { limit: 3 }).map((m) => m.command.id);
    expect(top('Merge & Center')[0]).toBe('grid.mergeCenter');
    expect(top('Freeze Panes')[0]).toBe('grid.freezePanes');
    expect(top('Track Changes')[0]).toBe('review.trackChanges');
    expect(top('Format Painter')[0]).toBe('edit.formatPainter');
    expect(top('tell me')).toContain('view.commandPalette');
    expect(top('strikeout')[0]).toBe('format.strikethrough');
    const hit = searchCommands(cmds, 'smartart')[0];
    expect(hit?.command.id).toBe('deck.convertToDiagram');
    expect(hit?.matchedOn).not.toBe('title');
  });

  it('returns everything for an empty query, in order', () => {
    const list = [
      { id: 'a.b', title: 'B' },
      { id: 'a.a', title: 'A' },
    ];
    expect(searchCommands(list, '  ').map((m) => m.command.id)).toEqual(['a.b', 'a.a']);
  });
});

describe('RecentCommands', () => {
  it('keeps the newest first, de-duplicated, limited and persisted locally', () => {
    const storage = new MemoryStorage();
    const r = new RecentCommands({ storage, limit: 3 });
    for (const id of ['a.one', 'a.two', 'a.one', 'a.three', 'a.four', 'view.commandPalette']) r.record(id);
    expect(r.list()).toEqual(['a.four', 'a.three', 'a.one']);
    expect(new RecentCommands({ storage, limit: 3 }).list()).toEqual(['a.four', 'a.three', 'a.one']);
    r.remove('a.three');
    expect(r.list()).toEqual(['a.four', 'a.one']);
    r.clear();
    expect(r.list()).toEqual([]);
    storage.setItem('kiln.recentCommands', '{"not":"an array"}');
    expect(new RecentCommands({ storage }).list()).toEqual([]);
  });
});
