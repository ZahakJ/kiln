/**
 * The notes pane under the stage (the Deck mockup's "Speaker notes"): the current slide's notes
 * as plain text (its text box is also Notes Page view's notes box). Typing is sent to the core
 * after a short pause (`deck.notes.set_text`, one undo step per pause); the core's text replaces
 * the pane's only while the pane is not being edited.
 */
import { t } from '@kiln/i18n';
import { useEffect, useRef, useState } from 'react';
import { useDeck } from './hooks.ts';
import type { DeckEditorModel } from './model.ts';

const PAUSE_MS = 400;

/**
 * The notes' text box: typing is sent after a short pause and on leaving it, to the slide it was
 * typed on (the pane under the stage, and Notes Page view's notes box).
 */
export function NotesText({ model, className }: { model: DeckEditorModel; className: string }) {
  const notes = useDeck(model, (s) => s.deck?.notes ?? '');
  const slide = useDeck(model, (s) => s.deck?.slides[s.deck.current]?.id ?? 0);
  const [text, setText] = useState(notes);
  const dirty = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const editingSlide = useRef(slide);

  const flush = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (!dirty.current) return;
    dirty.current = false;
    void model.core('deck.notes.set_text', { text: textRef.current });
  };
  const textRef = useRef(text);
  textRef.current = text;

  // Another slide: the pending notes go to the slide they were typed on first.
  useEffect(() => {
    if (editingSlide.current !== slide) {
      flush();
      editingSlide.current = slide;
    }
    if (!dirty.current) setText(notes);
  }, [notes, slide]);
  useEffect(() => () => flush(), []);

  return (
    <textarea
      className={className}
      value={text}
      placeholder={t('Click to add notes')}
      aria-label={t('Speaker notes')}
      spellCheck
      onChange={(e) => {
        setText(e.target.value);
        dirty.current = true;
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(flush, PAUSE_MS);
      }}
      onBlur={flush}
      onKeyDown={(e) => {
        // The notes keep their own keys (Ctrl+A, arrows); Esc goes back to the slide.
        if (e.key === 'Escape') {
          flush();
          model.stage?.focus();
        }
        e.stopPropagation();
      }}
    />
  );
}

export function NotesPane({ model }: { model: DeckEditorModel }) {
  return (
    <section
      className="kiln-decknotes"
      aria-label={t('Speaker notes')}
      data-kiln-region="pane"
      data-testid="deck-notes"
    >
      <h2 className="kiln-decknotes__title">{t('Speaker notes')}</h2>
      <NotesText model={model} className="kiln-decknotes__text" />
    </section>
  );
}
