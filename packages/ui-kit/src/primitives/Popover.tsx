import * as RP from '@radix-ui/react-popover';
import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';

/** Anchored, non-modal floating panel (colour pickers, table grid picker, galleries). */
export const Popover = RP.Root;
export const PopoverTrigger = RP.Trigger;
export const PopoverAnchor = RP.Anchor;
export const PopoverClose = RP.Close;

export interface PopoverContentProps extends ComponentPropsWithRef<typeof RP.Content> {
  /** Draw the little arrow pointing at the trigger. Default false (Office panels have none). */
  arrow?: boolean;
  /** Remove the default padding (for edge-to-edge content such as galleries). */
  flush?: boolean;
}

export function PopoverContent({
  className,
  children,
  arrow = false,
  flush = false,
  sideOffset = 4,
  align = 'start',
  collisionPadding = 8,
  ...rest
}: PopoverContentProps) {
  return (
    <RP.Portal>
      <RP.Content
        className={cx('of-popover', flush && 'of-popover--flush', className)}
        sideOffset={sideOffset}
        align={align}
        collisionPadding={collisionPadding}
        {...rest}
      >
        {children}
        {arrow ? <RP.Arrow className="of-popover__arrow" width={12} height={6} /> : null}
      </RP.Content>
    </RP.Portal>
  );
}
