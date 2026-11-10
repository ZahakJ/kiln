/**
 * Set Up Slide Show (Slide Show ▸ Set Up Slide Show): show type, show options, the slides shown,
 * how slides advance, and Presenter View. Saved with the presentation (`p:showPr`, as PowerPoint
 * keeps it).
 */
import { t } from '@kiln/i18n';
import type { DeckShowSettings, DeckShowType } from '@kiln/protocol';
import { NumberInput } from '@kiln/ui-kit';
import { useState } from 'react';
import { DeckDialogFrame } from '../dialogs/DeckDialogs.tsx';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

const DEFAULTS: DeckShowSettings = {
  kind: 'Speaker',
  loop_show: false,
  no_narration: false,
  no_animation: false,
  from: null,
  to: null,
  manual: false,
  presenter_view: false,
};

export function SetUpShowDialog({ model }: { model: DeckEditorModel }) {
  const current = useDeck(model, (s) => s.deck?.motion.settings ?? DEFAULTS);
  const count = useDeck(model, (s) => s.deck?.slides.length ?? 1);
  const [s, setS] = useState<DeckShowSettings>(current);
  const [range, setRange] = useState(current.from !== null || current.to !== null);
  const set = (patch: Partial<DeckShowSettings>) => setS((x) => ({ ...x, ...patch }));
  const kiosk = s.kind === 'Kiosk';
  const from = s.from ?? 1;
  const to = s.to ?? count;
  const ok = () => {
    const next: DeckShowSettings = {
      ...s,
      loop_show: kiosk ? true : s.loop_show,
      from: range ? Math.max(1, Math.min(count, from)) : null,
      to: range ? Math.max(Math.min(count, from), Math.min(count, to)) : null,
    };
    return model.run('deck.show.settings', { settings: next });
  };
  const kind = (k: DeckShowType, label: string) => (
    <label>
      <input type="radio" name="show-type" checked={s.kind === k} onChange={() => set({ kind: k })} />
      {label}
    </label>
  );
  return (
    <DeckDialogFrame model={model} title={t('Set Up Show')} testId="deck-setup-show" size="md" onOk={ok}>
      <div className="kiln-deckdlg__row kiln-setupshow">
        <fieldset className="kiln-deckdlg__radios">
          <legend>{t('Show type')}</legend>
          {kind('Speaker', 'Presented by a speaker (full screen)')}
          {kind('Window', 'Browsed by an individual (window)')}
          {kind('Kiosk', 'Browsed at a kiosk (full screen)')}
        </fieldset>
        <fieldset className="kiln-deckdlg__radios">
          <legend>{t('Show slides')}</legend>
          <label>
            <input type="radio" name="show-slides" checked={!range} onChange={() => setRange(false)} />
            {t('All')}
          </label>
          <label className="kiln-setupshow__range">
            <input type="radio" name="show-slides" checked={range} onChange={() => setRange(true)} />
            {t('From')}
            <NumberInput
              aria-label={t('From slide')}
              value={from}
              min={1}
              max={count}
              step={1}
              precision={0}
              size="sm"
              width={64}
              onValueChange={(v) => {
                setRange(true);
                set({ from: Math.round(v) });
              }}
            />
            {t('to')}
            <NumberInput
              aria-label={t('To slide')}
              value={to}
              min={1}
              max={count}
              step={1}
              precision={0}
              size="sm"
              width={64}
              onValueChange={(v) => {
                setRange(true);
                set({ to: Math.round(v) });
              }}
            />
          </label>
        </fieldset>
      </div>
      <div className="kiln-deckdlg__row kiln-setupshow">
        <fieldset className="kiln-deckdlg__radios">
          <legend>{t('Show options')}</legend>
          <label className="kiln-deckdlg__check">
            <input
              type="checkbox"
              checked={kiosk || s.loop_show}
              disabled={kiosk}
              onChange={(e) => set({ loop_show: e.target.checked })}
            />
            {t('Loop continuously until ‘Esc’')}
          </label>
          <label className="kiln-deckdlg__check">
            <input
              type="checkbox"
              checked={s.no_narration}
              onChange={(e) => set({ no_narration: e.target.checked })}
            />
            {t('Show without narration')}
          </label>
          <label className="kiln-deckdlg__check">
            <input
              type="checkbox"
              checked={s.no_animation}
              onChange={(e) => set({ no_animation: e.target.checked })}
              data-testid="deck-setup-no-animation"
            />
            {t('Show without animation')}
          </label>
        </fieldset>
        <fieldset className="kiln-deckdlg__radios">
          <legend>{t('Advance slides')}</legend>
          <label>
            <input type="radio" name="advance" checked={s.manual} onChange={() => set({ manual: true })} />
            {t('Manually')}
          </label>
          <label>
            <input type="radio" name="advance" checked={!s.manual} onChange={() => set({ manual: false })} />
            {t('Using timings, if present')}
          </label>
          <label className="kiln-deckdlg__check">
            <input
              type="checkbox"
              checked={s.presenter_view}
              onChange={(e) => set({ presenter_view: e.target.checked })}
            />
            {t('Use Presenter View')}
          </label>
        </fieldset>
      </div>
    </DeckDialogFrame>
  );
}
