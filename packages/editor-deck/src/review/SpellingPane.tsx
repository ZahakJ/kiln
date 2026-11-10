/**
 * The Spelling pane (Review ▸ Spelling, F7): the misspelled word the core stopped at (selected on
 * the slide or in the notes), its suggestions, and Change, Change All, Ignore Once, Ignore All and
 * Add to Dictionary, each of which goes on to the next word; "Spell check complete" once every
 * slide and its notes are checked.
 */
import { t } from '@kiln/i18n';
import { Button, Icon } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { rememberWord } from './dictionary.ts';
import { openPane } from './reviewCommands.ts';

export function SpellingPane({ model }: { model: DeckEditorModel }) {
  const hit = useDeck(model, (s) => s.deck?.spelling ?? null);
  const done = useDeck(model, (s) => s.deck?.spell_done ?? false);
  const [change, setChange] = useState('');
  useEffect(() => setChange(hit?.suggestions[0] ?? ''), [hit]);
  const close = () => {
    openPane(model, null);
    model.stage?.focus();
  };
  return (
    <aside className="kiln-deckpane" aria-label={t('Spelling')} data-testid="deck-spelling">
      <header className="kiln-deckpane__head">
        <h2>{t('Spelling')}</h2>
        <button
          type="button"
          className="kiln-deckpane__close"
          aria-label={t('Close Spelling')}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-deckpane__body kiln-deckspell">
        {!hit && done ? (
          <>
            <p className="kiln-deckspell__done">{t("Spell check complete. You're good to go!")}</p>
            <Button size="sm" onClick={close}>
              {t('OK')}
            </Button>
          </>
        ) : null}
        {!hit && !done ? <p className="kiln-deckspell__hint">{t('Checking…')}</p> : null}
        {hit ? (
          <>
            <p className="kiln-deckspell__hint">
              {t('Not in dictionary')}
              {hit.notes
                ? t(', in the notes of slide {value}', { value: hit.slide + 1 })
                : t(', on slide {value}', { value: hit.slide + 1 })}
              :
            </p>
            <div className="kiln-deckspell__word" data-testid="deck-spell-word">
              {hit.word}
            </div>
            <div className="kiln-deckspell__row">
              <Button
                size="sm"
                onClick={() => void model.core('deck.spell.ignore', { all: false, add: false })}
              >
                {t('Ignore Once')}
              </Button>
              <Button
                size="sm"
                onClick={() => void model.core('deck.spell.ignore', { all: true, add: false })}
              >
                {t('Ignore All')}
              </Button>
              <Button
                size="sm"
                onClick={async () => {
                  if (await model.core('deck.spell.ignore', { all: false, add: true }))
                    rememberWord(hit.word);
                }}
              >
                {t('Add')}
              </Button>
            </div>
            <div className="kiln-deckspell__list" role="listbox" aria-label={t('Suggestions')}>
              {hit.suggestions.map((s) => (
                <div
                  key={s}
                  role="option"
                  tabIndex={0}
                  aria-selected={s === change}
                  className="kiln-deckspell__option"
                  onClick={() => setChange(s)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') setChange(s);
                  }}
                  onDoubleClick={() => void model.core('deck.spell.change', { with: s, all: false })}
                >
                  {s}
                </div>
              ))}
              {hit.suggestions.length === 0 ? (
                <p className="kiln-deckspell__hint">{t('(No suggestions)')}</p>
              ) : null}
            </div>
            <div className="kiln-deckspell__row">
              <Button
                size="sm"
                variant="primary"
                disabled={!change}
                data-testid="deck-spell-change"
                onClick={() => void model.core('deck.spell.change', { with: change, all: false })}
              >
                {t('Change')}
              </Button>
              <Button
                size="sm"
                disabled={!change}
                onClick={() => void model.core('deck.spell.change', { with: change, all: true })}
              >
                {t('Change All')}
              </Button>
            </div>
          </>
        ) : null}
      </div>
    </aside>
  );
}
