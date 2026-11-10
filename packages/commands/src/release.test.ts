import { describe, expect, it } from 'vitest';
import { createCommandRegistry } from './bootstrap.ts';
import { DEFAULT_KEYMAP_LAYERS } from './keymaps/index.ts';
import { commandMessages } from './messages.ts';
import { APP_IDS, isReleasedApp, RELEASE_APPS } from './types.ts';

/** Kiln Notes as an app (not speaker notes, notes pages or a cell's notes). */
const NOTES_APP = /Kiln Notes|\bnotebooks?\b|OneNote/i;

describe('the apps of the release (v0.1)', () => {
  it('ships Kiln Write, Grid and Deck; Kiln Notes is built but not shown', () => {
    expect(RELEASE_APPS).toEqual(['write', 'grid', 'deck']);
    expect(APP_IDS).toContain('notes');
    expect(isReleasedApp('notes')).toBe(false);
    expect(isReleasedApp('hub')).toBe(true);
  });

  it('lists no Kiln Notes command in the palette, ribbon or keyboard options of a released app', () => {
    for (const app of [...RELEASE_APPS, 'hub' as const]) {
      const registry = createCommandRegistry({ app, declareCatalog: true });
      const shown = registry.list({ visibleOnly: true });
      for (const c of shown) {
        expect(c.id.startsWith('notes.'), `${app}: ${c.id}`).toBe(false);
        expect(`${c.title} ${c.description ?? ''}`, `${app}: ${c.id}`).not.toMatch(NOTES_APP);
      }
    }
  });

  it('binds no shortcut for Kiln Notes and translates none of its strings', () => {
    expect(DEFAULT_KEYMAP_LAYERS.notes ?? []).toEqual([]);
    for (const [key, text] of Object.entries(commandMessages())) {
      expect(key.startsWith('cmd.notes.'), key).toBe(false);
      expect(text, key).not.toMatch(NOTES_APP);
    }
  });
});
