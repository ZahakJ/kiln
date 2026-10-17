/**
 * The Spelling pane (Review ▸ Spelling & Grammar, F7), as in Word's Editor pane: the misspelled
 * word in its sentence, suggestions, Change / Change All, Ignore Once / Ignore All, Add to
 * Dictionary, and on to the next word until the check is complete. The word is selected in the
 * document while it is shown.
 */
import type { SpellingHit } from '@kiln/protocol';
import { Button } from '@kiln/ui-kit';
import { useState } from 'react';
import { useStore } from 'zustand';
import { addToDictionary } from '../features/dictionary.ts';
import { featuresOf } from '../features/model.ts';
import type { WriteEditorModel } from '../model.ts';

function Context({ hit }: { hit: SpellingHit }) {
  const chars = Array.from(hit.context);
  const a = hit.context_offset;
  const b = a + hit.len;
  return (
    <p className="kiln-spelling-pane__context">
      {chars.slice(0, a).join('')}
      <mark>{chars.slice(a, b).join('')}</mark>
      {chars.slice(b).join('')}
    </p>
  );
}

export function SpellingPane({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const hit = useStore(features.store, (s) => s.spelling);
  const done = useStore(features.store, (s) => s.spellingDone);
  const [choice, setChoice] = useState<string | null>(null);
  const next = () => {
    setChoice(null);
    void features.nextMisspelling();
  };
  if (done || !hit)
    return (
      <div className="kiln-spelling-pane" data-testid="write-spelling-pane">
        <p className="kiln-spelling-pane__done">{done ? 'The spelling check is complete.' : 'Checking…'}</p>
        {done ? (
          <Button size="sm" onClick={next}>
            Check again
          </Button>
        ) : null}
      </div>
    );
  const pick = choice ?? hit.suggestions[0] ?? null;
  const change = async (all: boolean) => {
    if (!pick) return;
    if (await model.run('write.spell.replace', { pos: hit.start, with: pick, all })) next();
  };
  return (
    <div className="kiln-spelling-pane" data-testid="write-spelling-pane">
      <h3 className="kiln-spelling-pane__title">Not in dictionary</h3>
      <Context hit={hit} />
      <h3 className="kiln-spelling-pane__title">Suggestions</h3>
      {hit.suggestions.length === 0 ? (
        <p className="kiln-comments-pane__empty">No suggestions.</p>
      ) : (
        <div className="kiln-spelling-pane__suggestions" role="listbox" aria-label="Suggestions">
          {hit.suggestions.map((s) => (
            <button
              key={s}
              type="button"
              role="option"
              aria-selected={pick === s}
              className="kiln-spelling-pane__suggestion"
              onClick={() => setChoice(s)}
              onDoubleClick={() =>
                void model.run('write.spell.replace', { pos: hit.start, with: s, all: false }).then(next)
              }
            >
              {s}
            </button>
          ))}
        </div>
      )}
      <div className="kiln-spelling-pane__actions">
        <Button size="sm" variant="primary" disabled={!pick} onClick={() => void change(false)}>
          Change
        </Button>
        <Button size="sm" disabled={!pick} onClick={() => void change(true)}>
          Change All
        </Button>
        <Button size="sm" onClick={() => void model.run('write.spell.ignore', { pos: hit.start }).then(next)}>
          Ignore Once
        </Button>
        <Button
          size="sm"
          onClick={() => void model.run('write.spell.ignore_all', { word: hit.word }).then(next)}
        >
          Ignore All
        </Button>
        <Button size="sm" onClick={() => void addToDictionary(model, hit.word).then(next)}>
          Add to Dictionary
        </Button>
      </div>
    </div>
  );
}
