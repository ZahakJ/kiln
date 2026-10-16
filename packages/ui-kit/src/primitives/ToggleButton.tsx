import { type ReactNode, useState } from 'react';
import { Button, type ButtonProps } from './Button.tsx';
import { cx } from './cx.ts';
import { Tooltip } from './Tooltip.tsx';

export interface ToggleButtonProps extends Omit<ButtonProps, 'onChange'> {
  pressed?: boolean;
  defaultPressed?: boolean;
  onPressedChange?: (pressed: boolean) => void;
  /** For icon-only toggles: accessible name and tooltip title. */
  label?: string;
  description?: ReactNode;
}

/** Two-state button (`aria-pressed`) — Bold, Italic, Show ¶, Ruler… Controlled or uncontrolled. */
export function ToggleButton({
  pressed,
  defaultPressed = false,
  onPressedChange,
  label,
  description,
  variant = 'toolbar',
  className,
  onClick,
  children,
  shortcut,
  ...rest
}: ToggleButtonProps) {
  const [inner, setInner] = useState(defaultPressed);
  const isOn = pressed ?? inner;
  const button = (
    <Button
      aria-pressed={isOn}
      aria-label={children ? undefined : label}
      data-state={isOn ? 'on' : 'off'}
      variant={variant}
      shortcut={shortcut}
      className={cx('kiln-toggle', className)}
      onClick={(e) => {
        onClick?.(e);
        if (e.defaultPrevented) return;
        if (pressed === undefined) setInner(!isOn);
        onPressedChange?.(!isOn);
      }}
      {...rest}
    >
      {children}
    </Button>
  );
  if (!label || children) return button;
  return (
    <Tooltip content={label} shortcut={shortcut} description={description}>
      {button}
    </Tooltip>
  );
}
