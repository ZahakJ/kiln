import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';

export interface SpinnerProps extends ComponentPropsWithRef<'span'> {
  /** px. Default 16. */
  size?: number;
  /** Accessible status text. Default "Loading". */
  label?: string;
}

/** Indeterminate progress indicator; announces `label` politely. */
export function Spinner({ size = 16, label = 'Loading', className, style, ...rest }: SpinnerProps) {
  return (
    <span
      role="status"
      className={cx('of-spinner', className)}
      style={{ width: size, height: size, ...style }}
      {...rest}
    >
      <svg viewBox="0 0 20 20" width={size} height={size} aria-hidden="true" focusable="false">
        <circle className="of-spinner__track" cx="10" cy="10" r="8" fill="none" strokeWidth="2.25" />
        <circle
          className="of-spinner__arc"
          cx="10"
          cy="10"
          r="8"
          fill="none"
          strokeWidth="2.25"
          strokeLinecap="round"
          strokeDasharray="14 50.3"
        />
      </svg>
      <span className="of-visually-hidden">{label}</span>
    </span>
  );
}
