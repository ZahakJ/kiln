import type { ComponentPropsWithRef, ReactNode } from 'react';
import { cx } from './cx.ts';
import { ariaKeyShortcuts } from './shortcut.ts';

export type ButtonVariant = 'default' | 'primary' | 'ember' | 'ghost' | 'toolbar' | 'split' | 'danger';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends ComponentPropsWithRef<'button'> {
  /**
   * `default` outlined, `primary` solid in the app's colour (Share, OK), `ember` solid ember for
   * suite-level actions only (Home's New, the tour), `ghost` borderless, `toolbar` ribbon command,
   * `split` a segment of a SplitButton, `danger` destructive confirmation.
   */
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Leading icon (an `<Icon/>`). */
  icon?: ReactNode;
  /** Trailing icon (e.g. a dropdown chevron). */
  iconAfter?: ReactNode;
  /** `stacked`: big ribbon button, icon above label (Classic ribbon). */
  layout?: 'inline' | 'stacked';
  /** Platform-neutral shortcut exposed as `aria-keyshortcuts`. */
  shortcut?: string;
  /** Shows a spinner-less busy state (`aria-busy`) and blocks clicks. */
  busy?: boolean;
}

/** The base command button. Native `<button>`, so Enter/Space activation and focus come for free. */
export function Button({
  variant = 'default',
  size = 'md',
  icon,
  iconAfter,
  layout = 'inline',
  shortcut,
  busy,
  className,
  children,
  type = 'button',
  disabled,
  onClick,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        'kiln-btn',
        `kiln-btn--${variant}`,
        `kiln-btn--${size}`,
        layout === 'stacked' && 'kiln-btn--stacked',
        !children && 'kiln-btn--icon-only',
        className,
      )}
      aria-keyshortcuts={shortcut ? ariaKeyShortcuts(shortcut) : undefined}
      aria-busy={busy || undefined}
      disabled={disabled}
      onClick={busy ? (e) => e.preventDefault() : onClick}
      {...rest}
    >
      {icon ? <span className="kiln-btn__icon">{icon}</span> : null}
      {children != null && children !== false ? <span className="kiln-btn__label">{children}</span> : null}
      {iconAfter ? <span className="kiln-btn__icon-after">{iconAfter}</span> : null}
    </button>
  );
}
