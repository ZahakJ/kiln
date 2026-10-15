import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';

export interface SeparatorProps extends ComponentPropsWithRef<'div'> {
  orientation?: 'horizontal' | 'vertical';
  /** Purely visual (hidden from assistive tech). Default false. */
  decorative?: boolean;
}

/** 1 px hairline divider (ribbon groups, menus, toolbars). */
export function Separator({
  orientation = 'horizontal',
  decorative = false,
  className,
  ...rest
}: SeparatorProps) {
  return (
    <div
      {...(decorative
        ? { role: 'none' }
        : { role: 'separator', 'aria-orientation': orientation === 'vertical' ? orientation : undefined })}
      data-orientation={orientation}
      className={cx('of-separator', className)}
      {...rest}
    />
  );
}
