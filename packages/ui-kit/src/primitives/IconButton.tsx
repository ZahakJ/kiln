import type { ReactNode } from 'react';
import { Button, type ButtonProps } from './Button.tsx';
import { Tooltip } from './Tooltip.tsx';

export interface IconButtonProps extends Omit<ButtonProps, 'children' | 'icon' | 'iconAfter' | 'layout'> {
  /** The icon (an `<Icon/>`). */
  icon: ReactNode;
  /** Accessible name and tooltip title. Required: icon-only buttons must be labelled (§3.10). */
  label: string;
  /** Tooltip description (super tooltip). */
  description?: ReactNode;
  /** Show the tooltip. Default true. */
  tooltip?: boolean;
  tooltipSide?: 'top' | 'right' | 'bottom' | 'left';
}

/** Square icon-only button with an accessible name and a tooltip ("Bold (Ctrl+B)"). */
export function IconButton({
  icon,
  label,
  description,
  tooltip = true,
  tooltipSide = 'bottom',
  variant = 'toolbar',
  shortcut,
  ...rest
}: IconButtonProps) {
  const button = <Button aria-label={label} variant={variant} icon={icon} shortcut={shortcut} {...rest} />;
  if (!tooltip) return button;
  return (
    <Tooltip content={label} shortcut={shortcut} description={description} side={tooltipSide}>
      {button}
    </Tooltip>
  );
}
