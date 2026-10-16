import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';

/** Content for screen readers only (still in the accessibility tree, invisible on screen). */
export function VisuallyHidden({ className, ...rest }: ComponentPropsWithRef<'span'>) {
  return <span className={cx('kiln-visually-hidden', className)} {...rest} />;
}
