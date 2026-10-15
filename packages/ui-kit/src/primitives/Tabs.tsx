import * as RT from '@radix-ui/react-tabs';
import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';

/** WAI-ARIA Tabs (Radix): arrow keys move between tabs, Home/End, automatic activation. */
export const Tabs = RT.Root;

export interface TabsListProps extends ComponentPropsWithRef<typeof RT.List> {
  /** `underline`: ribbon-style accent underline. `segmented`: compact pill switcher. */
  variant?: 'underline' | 'segmented';
}

export function TabsList({ variant = 'underline', className, ...rest }: TabsListProps) {
  return <RT.List className={cx('of-tabs', `of-tabs--${variant}`, className)} {...rest} />;
}

export function TabsTrigger({ className, ...rest }: ComponentPropsWithRef<typeof RT.Trigger>) {
  return <RT.Trigger className={cx('of-tabs__tab', className)} {...rest} />;
}

export function TabsContent({ className, ...rest }: ComponentPropsWithRef<typeof RT.Content>) {
  return <RT.Content className={cx('of-tabs__panel', className)} {...rest} />;
}
