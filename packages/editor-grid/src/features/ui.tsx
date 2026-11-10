/**
 * Building blocks of the feature dialogs: the modal frame (OK/Cancel, Enter submits, Esc and
 * Cancel close, the keys go back to the sheet), labelled fields, check boxes, radio groups and
 * the inline error line a refused command shows in the dialog.
 */
import { t, tx } from '@kiln/i18n';
import { Button, Dialog, DialogContent, Select, type SelectOption } from '@kiln/ui-kit';
import { type ReactNode, useId, useState } from 'react';
import type { GridFeatures } from './model.ts';

export function FeatureFrame({
  features,
  title,
  onOk,
  okLabel = 'OK',
  okDisabled,
  size = 'sm',
  testId,
  footerStart,
  cancelLabel = 'Cancel',
  actions,
  onEscape,
  children,
}: {
  /** The owner of the dialog (the features, or the analysis features). */
  features: Pick<GridFeatures, 'closeDialog' | 'model'>;
  title: string;
  /** Returns an error message to keep the dialog open, or nothing to close it. */
  // biome-ignore lint/suspicious/noConfusingVoidType: callbacks that return nothing close the dialog.
  onOk?: () => Promise<string | null | undefined | void> | string | null | undefined | void;
  okLabel?: string;
  okDisabled?: boolean;
  size?: 'sm' | 'md' | 'lg';
  testId?: string;
  footerStart?: ReactNode;
  cancelLabel?: string;
  /** Buttons that replace OK and Cancel (an alert's Yes, No and Cancel). */
  actions?: ReactNode;
  /** Esc runs this instead of only closing (an alert's Cancel). */
  onEscape?: () => void;
  children: ReactNode;
}) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const close = () => features.closeDialog();
  const ok = async () => {
    if (okDisabled || busy || !onOk) return;
    setBusy(true);
    try {
      const e = await onOk();
      if (typeof e === 'string' && e) setError(e);
      else close();
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open onOpenChange={(o) => (o ? undefined : close())}>
      <DialogContent
        title={title}
        size={size}
        data-testid={testId}
        // Keys typed in the dialog stay in it (Ctrl+A selects the text, Ctrl+B is not Bold).
        data-kiln-shortcuts="off"
        onEscapeKeyDown={(e) => {
          if (!onEscape) return;
          e.preventDefault();
          onEscape();
        }}
        onCloseAutoFocus={(e) => {
          // The keys go back to the sheet, not to the ribbon button that opened the dialog.
          e.preventDefault();
          features.model.surface?.focus();
        }}
        footer={
          actions ?? (
            <>
              {footerStart ? <span className="kiln-gfx__footer-start">{footerStart}</span> : null}
              <Button onClick={close}>{onOk ? cancelLabel : t('Close')}</Button>
              {onOk ? (
                <Button variant="primary" disabled={okDisabled || busy} onClick={ok}>
                  {okLabel}
                </Button>
              ) : null}
            </>
          )
        }
      >
        <form
          className="kiln-griddlg kiln-gfx"
          onSubmit={(e) => {
            e.preventDefault();
            void ok();
          }}
        >
          {children}
          {error ? (
            <p className="kiln-gfx__error" role="alert">
              {error}
            </p>
          ) : null}
          <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function Field({
  label,
  children,
  wide,
}: {
  label: string;
  children: (id: string) => ReactNode;
  wide?: boolean;
}) {
  const id = useId();
  return (
    <div className={wide ? 'kiln-griddlg__field kiln-gfx__wide' : 'kiln-griddlg__field'}>
      <label className="kiln-griddlg__label" htmlFor={id} id={`${id}-label`}>
        {label}
      </label>
      {children(id)}
    </div>
  );
}

export function TextField({
  label,
  value,
  onChange,
  placeholder,
  autoFocus,
  wide,
  testId,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  wide?: boolean;
  testId?: string;
}) {
  return (
    <Field label={label} wide={wide}>
      {(id) => (
        <input
          id={id}
          className="kiln-griddlg__input"
          value={value}
          placeholder={placeholder}
          // biome-ignore lint/a11y/noAutofocus: the dialog's first field takes the keys, as in Excel.
          autoFocus={autoFocus}
          // The first field opens with its text selected, so typing replaces it.
          ref={(el) => {
            if (el && autoFocus && el.dataset.selected !== '1') {
              el.dataset.selected = '1';
              el.select();
            }
          }}
          data-testid={testId}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </Field>
  );
}

export function Check({
  label,
  checked,
  onChange,
  testId,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  testId?: string;
}) {
  return (
    <label className="kiln-griddlg__check">
      <input
        type="checkbox"
        checked={checked}
        data-testid={testId}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

export function Radios<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
}) {
  const name = useId();
  return (
    <fieldset className="kiln-griddlg__radios">
      <legend>{label}</legend>
      {options.map(([v, text]) => (
        <label key={v} className="kiln-griddlg__check">
          <input type="radio" name={name} checked={value === v} onChange={() => onChange(v)} />
          {tx(text)}
        </label>
      ))}
    </fieldset>
  );
}

export function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
  width,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
  width?: number;
}) {
  const opts: SelectOption[] = options.map(([v, l]) => ({ value: v, label: l }));
  return (
    <Field label={label}>
      {(id) => (
        <Select
          id={id}
          labelId={`${id}-label`}
          value={value}
          width={width}
          options={opts}
          onValueChange={(v) => onChange(v as T)}
        />
      )}
    </Field>
  );
}

/**
 * A column's name in a dialog's list: its header text, on one line, or Excel's "Column A" when it
 * has none (an empty cell, or one holding only spaces or line breaks).
 */
export function columnLabel(header: string | undefined, col: number): string {
  const text = (header ?? '').replace(/\s+/g, ' ').trim();
  return text || t('Column {colName}', { colName: colName(col) });
}

/** Cell coordinates as A1 text (`B7`, `$B$7`). */
export function colName(c: number): string {
  let n = c + 1;
  let s = '';
  while (n > 0) {
    const m = (n - 1) % 26;
    s = String.fromCharCode(65 + m) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

export function a1(row: number, col: number, abs = false): string {
  const d = abs ? '$' : '';
  return `${d}${colName(col)}${d}${row + 1}`;
}

export function rangeA1(r: { r0: number; c0: number; r1: number; c1: number }, abs = true): string {
  const a = a1(r.r0, r.c0, abs);
  return r.r0 === r.r1 && r.c0 === r.c1 ? a : `${a}:${a1(r.r1, r.c1, abs)}`;
}
