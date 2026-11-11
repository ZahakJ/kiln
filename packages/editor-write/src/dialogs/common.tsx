/** Building blocks of the Write dialogs: labelled fields, rows and the OK/Cancel footer. */
import { t, tx } from '@kiln/i18n';
import {
  Button,
  Dialog,
  DialogContent,
  type MeasureUnit,
  measurementUnit,
  POINTS_PER_UNIT,
  subscribeOptions,
} from '@kiln/ui-kit';
import { type ReactNode, useId } from 'react';
import type { WriteEditorModel } from '../model.ts';

export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: (id: string) => ReactNode;
  hint?: string;
}) {
  const id = useId();
  return (
    <div className="kiln-write-field">
      {/* `id-label` names controls that are not inputs (selects) through aria-labelledby. */}
      <label htmlFor={id} id={`${id}-label`} className="kiln-write-field__label">
        {label}
      </label>
      {children(id)}
      {hint ? <span className="kiln-write-field__hint">{hint}</span> : null}
    </div>
  );
}

export function Check({
  label,
  checked,
  onChange,
  mixed,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  mixed?: boolean;
}) {
  return (
    <label className="kiln-write-check">
      <input
        type="checkbox"
        checked={checked}
        ref={(el) => {
          if (el) el.indeterminate = !!mixed;
        }}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

export function Radio<T extends string>({
  name,
  value,
  options,
  onChange,
  label,
}: {
  name: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <fieldset className="kiln-write-radios">
      <legend>{label}</legend>
      {options.map(([v, text]) => (
        <label key={v} className="kiln-write-check">
          <input type="radio" name={name} value={v} checked={value === v} onChange={() => onChange(v)} />
          {tx(text)}
        </label>
      ))}
    </fieldset>
  );
}

export function Grid({ children, cols = 2 }: { children: ReactNode; cols?: number }) {
  return (
    <div className="kiln-write-grid" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
      {children}
    </div>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="kiln-write-section">
      <h3 className="kiln-write-section__title">{title}</h3>
      {children}
    </section>
  );
}

/** A modal dialog of the editor, closed through the model. */
export function WriteDialogFrame({
  model,
  title,
  size,
  onOk,
  okLabel = 'OK',
  okDisabled,
  footerStart,
  children,
  testId,
}: {
  model: WriteEditorModel;
  title: string;
  size?: 'sm' | 'md' | 'lg' | 'xl';
  onOk?: () => boolean | undefined | Promise<boolean | undefined>;
  okLabel?: string;
  okDisabled?: boolean;
  footerStart?: ReactNode;
  children: ReactNode;
  testId?: string;
}) {
  const close = () => model.closeDialog();
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={title}
        size={size ?? 'md'}
        data-testid={testId}
        // Back to the page, as in Word. Done as the dialog lets go of the focus: before that its
        // focus trap keeps it, and after it the dialog would give it to the ribbon button that
        // opened it, which on a slower machine came after the editor had taken it.
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          model.controller?.focus();
        }}
        footerStart={footerStart}
        footer={
          onOk ? (
            <>
              <Button onClick={close}>{t('Cancel')}</Button>
              <Button
                variant="primary"
                disabled={okDisabled}
                onClick={async () => {
                  const keepOpen = await onOk();
                  if (keepOpen !== true) close();
                }}
              >
                {okLabel}
              </Button>
            </>
          ) : (
            <Button variant="primary" onClick={close}>
              {t('Close')}
            </Button>
          )
        }
      >
        <div className="kiln-write-dialog">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

/**
 * The length unit of the dialogs: the user's measurement unit (File ▸ Options ▸ Display; by
 * default inches where Word uses them, else centimetres), the same as the ribbon and the rulers.
 * A live binding: it follows a change of the option.
 */
export let LEN_UNIT: MeasureUnit = measurementUnit();
subscribeOptions(() => {
  LEN_UNIT = measurementUnit();
});
/** The unit's name in a "Measure in" list. */
export const unitName = (u: MeasureUnit) =>
  ({ in: 'Inches', cm: 'Centimetres', mm: 'Millimetres', pt: 'Points' })[u];
/** A length in points, in the dialogs' unit (two decimals). */
export const cmOf = (pt: number) => Math.round((pt / POINTS_PER_UNIT[LEN_UNIT]) * 100) / 100;
/** Points from a length in the dialogs' unit. */
export const ptOf = (len: number) => Math.round(len * POINTS_PER_UNIT[LEN_UNIT] * 100) / 100;
