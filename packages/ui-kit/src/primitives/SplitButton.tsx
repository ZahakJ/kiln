import * as DM from '@radix-ui/react-dropdown-menu';
import * as RP from '@radix-ui/react-popover';
import { ChevronDown } from 'lucide-react';
import type { KeyboardEvent, MouseEvent, ReactNode } from 'react';
import { useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';
import { MenuContent } from './Menu.tsx';
import { PopoverContent } from './Popover.tsx';
import { ariaKeyShortcuts } from './shortcut.ts';
import { Tooltip } from './Tooltip.tsx';

export interface SplitButtonProps {
  /** Visible label (inline/stacked) or accessible name (icon-only). */
  label: string;
  /** Hide the label visually (icon-only toolbar split button, e.g. Font Colour ▾). */
  iconOnly?: boolean;
  icon?: ReactNode;
  /** Primary action (e.g. apply the last used colour). */
  onClick: (e: MouseEvent<HTMLButtonElement>) => void;
  /** Menu items (`MenuItem`…) or any panel content when `panel` is set. */
  children: ReactNode;
  /** Render `children` as a free panel (colour picker, table grid) instead of menu items. */
  panel?: boolean;
  shortcut?: string;
  /** Tooltip description. */
  description?: ReactNode;
  /** `stacked`: Classic-ribbon large button with the arrow under the label. */
  layout?: 'inline' | 'stacked';
  disabled?: boolean;
  /** Accessible name of the arrow part. Default "More options for {label}". */
  menuLabel?: string;
  className?: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

/**
 * Office split button: the main part runs the default action, the arrow opens a menu or panel.
 * Two real buttons (Tab reaches both), Alt+↓ on the main part opens the menu, as in Office.
 */
export function SplitButton({
  label,
  iconOnly = false,
  icon,
  onClick,
  children,
  panel = false,
  shortcut,
  description,
  layout = 'inline',
  disabled,
  menuLabel,
  className,
  open: openProp,
  onOpenChange,
}: SplitButtonProps) {
  const [openState, setOpenState] = useState(false);
  const open = openProp ?? openState;
  const setOpen = (o: boolean) => {
    if (openProp === undefined) setOpenState(o);
    onOpenChange?.(o);
  };
  const onMainKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.altKey && e.key === 'ArrowDown') {
      e.preventDefault();
      setOpen(true);
    }
  };
  const main = (
    <button
      type="button"
      className={cx('of-btn', 'of-btn--split', 'of-split__main', iconOnly && 'of-btn--icon-only')}
      aria-label={iconOnly ? label : undefined}
      aria-keyshortcuts={shortcut ? ariaKeyShortcuts(shortcut) : undefined}
      disabled={disabled}
      onClick={onClick}
      onKeyDown={onMainKeyDown}
    >
      {icon ? <span className="of-btn__icon">{icon}</span> : null}
      {!iconOnly ? <span className="of-btn__label">{label}</span> : null}
    </button>
  );
  const arrow = (
    <button
      type="button"
      className="of-btn of-btn--split of-split__arrow"
      aria-label={menuLabel ?? `More options for ${label}`}
    >
      <Icon icon={ChevronDown} size={12} />
    </button>
  );
  const shell = (trigger: ReactNode) => (
    <span
      className={cx('of-split', layout === 'stacked' && 'of-split--stacked', open && 'is-open', className)}
    >
      {iconOnly ? (
        <Tooltip content={label} shortcut={shortcut} description={description}>
          {main}
        </Tooltip>
      ) : (
        main
      )}
      {trigger}
    </span>
  );
  if (panel) {
    // Free-form panels (colour picker, table grid) get a popover: their own keyboard model stays intact.
    return (
      <RP.Root open={open} onOpenChange={setOpen}>
        <RP.Anchor asChild>
          {shell(
            <RP.Trigger asChild disabled={disabled} aria-haspopup="dialog">
              {arrow}
            </RP.Trigger>,
          )}
        </RP.Anchor>
        <PopoverContent aria-label={label}>{children}</PopoverContent>
      </RP.Root>
    );
  }
  return (
    <DM.Root open={open} onOpenChange={setOpen} modal={false}>
      {shell(
        <DM.Trigger asChild disabled={disabled}>
          {arrow}
        </DM.Trigger>,
      )}
      <MenuContent>{children}</MenuContent>
    </DM.Root>
  );
}
