import { describe, expect, it } from 'vitest';
import { createCommandRegistry } from './bootstrap.ts';
import { COMMAND_CATALOG } from './catalog/index.ts';
import { fuzzyScore, searchCommands } from './search.ts';
import { createOfficeSynonyms } from './synonyms.ts';
import { RELEASE_TIERS } from './tiers.ts';
import { type AppId, RELEASE_APPS } from './types.ts';

const DECLARED = new Map(COMMAND_CATALOG.map((c) => [c.id, c]));
const SYNONYMS = createOfficeSynonyms();

/**
 * The palette's own pipeline in a release build: visible, palette-listed commands of one app
 * (each with an implementation, as the app registers them), ranked by search.
 */
function paletteTop(app: AppId, query: string, limit = 5): string[] {
  const registry = createCommandRegistry({ app, tiers: RELEASE_TIERS });
  registry.registerMany(registry.list().map((c) => ({ id: c.id, title: c.title, run: () => {} })));
  const commands = registry.list({ visibleOnly: true }).filter((c) => c.palette !== false);
  return searchCommands(commands, query, { limit, weight: (c) => (c.pending ? 0.8 : 1) }).map(
    (m) => m.command.id,
  );
}

describe('Office synonyms', () => {
  it('maps at least 200 commands', () => {
    expect(Object.keys(SYNONYMS).length).toBeGreaterThanOrEqual(200);
  });

  it('keys only declared commands, none of them Kiln Notes only', () => {
    for (const id of Object.keys(SYNONYMS)) {
      const decl = DECLARED.get(id);
      expect(decl, id).toBeDefined();
      const apps = decl?.apps;
      if (apps)
        expect(
          apps.some((a) => (RELEASE_APPS as readonly string[]).includes(a)),
          id,
        ).toBe(true);
    }
  });

  it('lists each term once per command (case, accents and punctuation folded)', () => {
    const repeats: string[] = [];
    for (const [id, terms] of Object.entries(SYNONYMS)) {
      expect(terms.length, id).toBeGreaterThan(0);
      terms.forEach((term, i) => {
        expect(term.trim(), id).not.toBe('');
        const twin = terms.findIndex((other, j) => j > i && fuzzyScore(term, other) === 1);
        if (twin >= 0) repeats.push(`${id}: "${term}" / "${terms[twin]}"`);
      });
    }
    expect(repeats).toEqual([]);
  });

  it('ranks the Office command first for the English names people type', () => {
    expect(paletteTop('write', 'track changes')[0]).toBe('write.review.track_changes');
    expect(paletteTop('write', 'section break')[0]).toBe('write.layout.section_break');
    expect(paletteTop('write', 'table of contents')).toContain('write.references.table_of_contents');
    expect(paletteTop('grid', 'freeze panes')[0]).toBe('grid.view.freeze_panes');
    expect(paletteTop('grid', 'merge & center')[0]).toBe('grid.alignment.merge_center');
    expect(paletteTop('grid', 'flash fill')[0]).toBe('grid.editing.flash_fill');
    expect(paletteTop('grid', 'sparklines')).toContain('grid.insert.sparkline_line');
    expect(paletteTop('grid', 'protect sheet')[0]).toBe('grid.review.protect_sheet');
    expect(paletteTop('deck', 'presenter view')[0]).toBe('deck.show.presenter_view');
    expect(paletteTop('deck', 'hide slide')[0]).toBe('deck.show.hide_slide');
  });

  it('finds German Office names in any interface language', () => {
    expect(paletteTop('grid', 'Fenster fixieren')[0]).toBe('grid.view.freeze_panes');
    expect(paletteTop('grid', 'fenster fixieren')[0]).toBe('grid.view.freeze_panes');
    expect(paletteTop('write', 'Änderungen nachverfolgen')[0]).toBe('write.review.track_changes');
    // Typed without the umlaut.
    expect(paletteTop('write', 'Anderungen nachverfolgen')[0]).toBe('write.review.track_changes');
    expect(paletteTop('grid', 'Zielwertsuche')[0]).toBe('grid.data.goal_seek');
    expect(paletteTop('deck', 'Referentenansicht')[0]).toBe('deck.show.presenter_view');
  });

  it('finds Arabic Office names as typed with or without hamza seats', () => {
    expect(paletteTop('write', 'تعقب التغييرات')[0]).toBe('write.review.track_changes');
    expect(paletteTop('grid', 'تجميد الأجزاء')[0]).toBe('grid.view.freeze_panes');
    expect(paletteTop('grid', 'تجميد الاجزاء')[0]).toBe('grid.view.freeze_panes');
    expect(paletteTop('grid', 'التنسيق الشرطي')[0]).toBe('grid.styles.conditional_formatting');
    expect(paletteTop('grid', 'دمج وتوسيط')[0]).toBe('grid.alignment.merge_center');
    expect(paletteTop('deck', 'شريحة جديدة')[0]).toBe('deck.slides.new_slide');
  });
});
