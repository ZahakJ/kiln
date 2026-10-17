/** Building blocks of the Write dialogs: labelled fields, rows and the OK/Cancel footer. */
import { Button, Dialog, DialogContent } from '@kiln/ui-kit';
import { type ReactNode, useId } from 'react';
import { usesInches } from '../hooks.ts';
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
          {text}
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
        footerStart={footerStart}
        footer={
          onOk ? (
            <>
              <Button onClick={close}>Cancel</Button>
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
              Close
            </Button>
          )
        }
      >
        <div className="kiln-write-dialog">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

/** The length unit of the dialogs: inches where Word uses them (US English), else centimetres. */
export const LEN_UNIT: 'in' | 'cm' = usesInches() ? 'in' : 'cm';
const PT_PER_UNIT = LEN_UNIT === 'in' ? 72 : 72 / 2.54;
/** A length in points, in the dialogs' unit (two decimals). */
export const cmOf = (pt: number) => Math.round((pt / PT_PER_UNIT) * 100) / 100;
/** Points from a length in the dialogs' unit. */
export const ptOf = (len: number) => Math.round(len * PT_PER_UNIT * 100) / 100;
