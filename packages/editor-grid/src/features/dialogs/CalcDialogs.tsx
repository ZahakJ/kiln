/**
 * Iterative calculation (Excel's Options ▸ Formulas ▸ Calculation options) and the circular
 * reference warning that a formula referring to its own cell raises while iteration is off.
 */
import { t } from '@kiln/i18n';
import { Button } from '@kiln/ui-kit';
import { useState } from 'react';
import { type GridFeatures, gridFeatures } from '../model.ts';
import { Check, FeatureFrame, TextField } from '../ui.tsx';

export function IterativeDialog({ features }: { features: GridFeatures }) {
  const [on, count, change] = gridFeatures(features.model)?.iterative ?? [false, 100, 0.001];
  const [enabled, setEnabled] = useState(on);
  const [iterations, setIterations] = useState(String(count));
  const [delta, setDelta] = useState(String(change));
  return (
    <FeatureFrame
      features={features}
      title={t('Iterative Calculation')}
      testId="grid-iterative"
      onOk={() =>
        features.tryExec({
          IterativeCalculation: {
            enabled,
            max_iterations: Number(iterations),
            max_change: Number(delta),
          },
        })
      }
    >
      <Check
        label={t('Enable iterative calculation')}
        checked={enabled}
        onChange={setEnabled}
        testId="grid-iterative-on"
      />
      <div className="kiln-gfx__row">
        <TextField
          label={t('Maximum Iterations')}
          value={iterations}
          onChange={setIterations}
          testId="grid-iterations"
        />
        <TextField label={t('Maximum Change')} value={delta} onChange={setDelta} testId="grid-max-change" />
      </div>
    </FeatureFrame>
  );
}

export function CircularWarning({ features }: { features: GridFeatures }) {
  const at = gridFeatures(features.model)?.circular[0] ?? '';
  return (
    <FeatureFrame
      features={features}
      title={t('Circular Reference Warning')}
      testId="grid-circular"
      onOk={() => undefined}
    >
      <p className="kiln-gfx__prompt" role="alert">
        {t(
          'There are one or more circular references where a formula refers to its own cell either directly or indirectly. This might cause them to calculate incorrectly.',
        )}
      </p>
      <p className="kiln-gfx__hint">
        {at
          ? t(
              'Try removing or changing these references, or moving the formulas to different cells ({cells}). To let them iterate, choose Formulas ▸ Calculation Options ▸ Iterative Calculation.',
              { cells: at },
            )
          : t(
              'Try removing or changing these references, or moving the formulas to different cells. To let them iterate, choose Formulas ▸ Calculation Options ▸ Iterative Calculation.',
            )}
      </p>
    </FeatureFrame>
  );
}

/** Excel's data validation alert for an entry the cell's rule refuses. */
export function ValidationAlert({ features }: { features: GridFeatures }) {
  const alert = gridFeatures(features.model)?.alert ?? null;
  const [style, title, message] = alert ?? ['Stop', 'Kiln Grid', ''];
  const m = features.model;
  const keep = async () => {
    await features.exec('AcceptInvalid');
    await m.core('grid.editing.commit', { advance: 'Down' });
  };
  const cancel = async () => {
    await m.core('grid.editing.cancel_edit');
  };
  // Retry and No go back to the entry with its text selected, to type over it (Excel).
  const retry = async () => {
    const text = m.state.grid?.editor?.text ?? '';
    const end = text.length;
    await m.edit('grid.editing.set_text', { text, start: 0, end, formula_bar: false });
  };
  const buttons: readonly [string, () => unknown, boolean][] =
    style === 'Stop'
      ? [
          ['Retry', retry, true],
          ['Cancel', cancel, false],
        ]
      : style === 'Warning'
        ? [
            ['Yes', keep, false],
            ['No', retry, true],
            ['Cancel', cancel, false],
          ]
        : [
            ['OK', keep, true],
            ['Cancel', cancel, false],
          ];
  return (
    <FeatureFrame
      features={features}
      title={title || t('Kiln Grid')}
      testId="grid-validation-alert"
      onEscape={() => {
        features.closeDialog();
        void cancel();
      }}
      actions={buttons.map(([label, act, primary]) => (
        <Button
          key={label}
          variant={primary ? 'primary' : undefined}
          // The default button has the focus (Enter presses it).
          autoFocus={primary}
          onClick={async () => {
            features.closeDialog();
            await act();
          }}
        >
          {label}
        </Button>
      ))}
    >
      <p className="kiln-gfx__prompt" role="alert">
        {message}
      </p>
      {style === 'Warning' ? <p className="kiln-gfx__hint">{t('Continue?')}</p> : null}
    </FeatureFrame>
  );
}
