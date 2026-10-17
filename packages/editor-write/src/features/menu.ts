/**
 * Context menu entries of Kiln Write's features: the spelling suggestions for the misspelled word
 * under the pointer (with Ignore Once, Ignore All and Add to Dictionary), as in Word, and the
 * comment and Markdown entries.
 */
import type { SpellingHit } from '@kiln/protocol';
import type { MenuEntry } from '@kiln/ui-kit';

/** The spelling block that heads the context menu over a misspelled word. */
export function spellingEntries(hit: SpellingHit | null): MenuEntry[] {
  if (!hit) return [];
  const suggestions: MenuEntry[] = hit.suggestions.slice(0, 5).map((s) => ({
    command: 'write.spell.replace',
    label: s,
    // No icon: the suggestions stand apart from the commands below them, as in Word.
    icon: '',
    args: { pos: hit.start, with: s, all: false },
  }));
  return [
    { kind: 'heading', label: `Spelling: “${hit.word}”` },
    ...(suggestions.length > 0 ? suggestions : [{ kind: 'heading' as const, label: 'No suggestions' }]),
    { command: 'write.spell.ignore', label: 'Ignore Once', args: { pos: hit.start } },
    { command: 'write.spell.ignore_all', label: 'Ignore All', args: { word: hit.word } },
    { command: 'ui.write.spell_add', label: 'Add to Dictionary', args: { word: hit.word } },
    { kind: 'separator' },
  ];
}

/** Paste as Markdown, under the paste options. */
export const PASTE_MARKDOWN_ENTRY: MenuEntry = {
  command: 'write.insert.paste_markdown_table',
  label: 'Paste as Markdown',
};
