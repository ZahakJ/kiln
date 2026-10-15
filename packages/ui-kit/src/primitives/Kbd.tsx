import type { ComponentPropsWithRef } from 'react';
import { cx } from './cx.ts';
import { formatShortcut, type Platform, shortcutLabel } from './shortcut.ts';

export interface KbdProps extends Omit<ComponentPropsWithRef<'kbd'>, 'children'> {
  /** Platform-neutral shortcut, e.g. `Mod+Shift+S`. */
  shortcut: string;
  platform?: Platform;
  /** `keys` renders one keycap per key; `inline` renders plain text (menus, tooltips). */
  variant?: 'keys' | 'inline';
}

/** Renders a keyboard shortcut, platform-aware (Ctrl on Windows/Linux, ⌘ on macOS). */
export function Kbd({ shortcut, platform, variant = 'keys', className, ...rest }: KbdProps) {
  const keys = formatShortcut(shortcut, platform);
  const label = shortcutLabel(shortcut, platform);
  if (variant === 'inline') {
    return (
      <kbd className={cx('of-kbd-inline', className)} {...rest}>
        {label}
      </kbd>
    );
  }
  return (
    <kbd className={cx('of-kbd', className)} {...rest}>
      <span className="of-visually-hidden">{label}</span>
      {keys.map((k, i) => (
        <span key={`${i}-${k}`} className="of-kbd__key" aria-hidden="true">
          {k}
        </span>
      ))}
    </kbd>
  );
}
