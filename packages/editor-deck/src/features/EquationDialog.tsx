/**
 * Insert ▸ Equation: the linear form PowerPoint's equation editor accepts (`^` superscripts, `_`
 * subscripts, `a/b` fractions, `\sqrt(x)`, `\sum`, `\int`, Greek letters by name), saved as Office
 * Math at the caret or in a new text box.
 */
import { msg, t, tx } from '@kiln/i18n';
import { useId, useState } from 'react';
import { DeckDialogFrame } from '../dialogs/DeckDialogs.tsx';
import type { DeckEditorModel } from '../model.ts';

const EXAMPLES: readonly [string, string][] = [
  ['a^2+b^2=c^2', msg('Pythagoras')],
  ['x=(-b\\pm\\sqrt(b^2-4ac))/(2a)', msg('Quadratic formula')],
  ['\\sum_(i=1)^n i=(n(n+1))/2', msg('Sum')],
  ['e^(i\\pi)+1=0', msg('Euler')],
];

export function EquationDialog({ model }: { model: DeckEditorModel }) {
  const id = useId();
  const [linear, setLinear] = useState('');
  return (
    <DeckDialogFrame
      model={model}
      title={t('Insert Equation')}
      size="md"
      okLabel={t('Insert')}
      okDisabled={!linear.trim()}
      testId="deck-equation-dialog"
      onOk={() => model.core('deck.insert.equation', { linear })}
    >
      <div className="kiln-deckdlg__field">
        <label htmlFor={id}>{t('Equation')}</label>
        <input
          id={id}
          className="kiln-field kiln-deck-math"
          value={linear}
          spellCheck={false}
          // biome-ignore lint/a11y/noAutofocus: the equation box takes focus, as PowerPoint's does
          autoFocus
          placeholder={t('Type equation here.')}
          onChange={(e) => setLinear(e.target.value)}
        />
        <small className="kiln-deckdlg__hint">
          {t(
            '^ superscript, _ subscript, a/b fraction, \\sqrt(x), \\sum, \\int, \\alpha … as in the linear format',
          )}
        </small>
      </div>
      <fieldset className="kiln-deck-math__examples">
        <legend className="kiln-deck-math__legend">{t('Examples')}</legend>
        {EXAMPLES.map(([eq, name]) => (
          <button key={eq} type="button" className="kiln-deck-math__example" onClick={() => setLinear(eq)}>
            <span>{tx(name)}</span>
            <code>{eq}</code>
          </button>
        ))}
      </fieldset>
    </DeckDialogFrame>
  );
}
