import * as DM from '@radix-ui/react-dropdown-menu';
import { Check, ChevronRight, Dot } from 'lucide-react';
import type { ComponentPropsWithRef, ReactNode } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';
import { Kbd } from './Kbd.tsx';
import { ariaKeyShortcuts } from './shortcut.ts';

/** Menu root (Radix DropdownMenu): arrow keys, typeahead, Esc, submenus, focus return. */
export const Menu = DM.Root;
/** Wrap a single button with `asChild`. */
export const MenuTrigger = DM.Trigger;
export const MenuGroup = DM.Group;
export const MenuRadioGroup = DM.RadioGroup;

export interface MenuContentProps extends ComponentPropsWithRef<typeof DM.Content> {}

export function MenuContent({
  className,
  sideOffset = 4,
  align = 'start',
  collisionPadding = 8,
  ...rest
}: MenuContentProps) {
  return (
    <DM.Portal>
      <DM.Content
        className={cx('kiln-menu', className)}
        sideOffset={sideOffset}
        align={align}
        collisionPadding={collisionPadding}
        loop
        {...rest}
      />
    </DM.Portal>
  );
}

interface ItemDecor {
  /** Leading icon (an `<Icon/>`). */
  icon?: ReactNode;
  /** Platform-neutral shortcut shown right-aligned, e.g. `Mod+C`. */
  shortcut?: string;
  /** Secondary line under the label (Office "Paste Special…" style hints). */
  description?: ReactNode;
}

export interface MenuItemProps extends ComponentPropsWithRef<typeof DM.Item>, ItemDecor {
  /** Destructive action styling. */
  danger?: boolean;
}

function ItemBody({ icon, shortcut, description, children }: ItemDecor & { children?: ReactNode }) {
  return (
    <>
      <span className="kiln-menu__icon" aria-hidden="true">
        {icon}
      </span>
      <span className="kiln-menu__text">
        <span className="kiln-menu__label">{children}</span>
        {description ? <span className="kiln-menu__description">{description}</span> : null}
      </span>
      {shortcut ? (
        <Kbd className="kiln-menu__shortcut" shortcut={shortcut} variant="inline" aria-hidden="true" />
      ) : null}
    </>
  );
}

export function MenuItem({
  icon,
  shortcut,
  description,
  danger,
  className,
  children,
  ...rest
}: MenuItemProps) {
  return (
    <DM.Item
      className={cx('kiln-menu__item', danger && 'kiln-menu__item--danger', className)}
      aria-keyshortcuts={shortcut ? ariaKeyShortcuts(shortcut) : undefined}
      {...rest}
    >
      <ItemBody icon={icon} shortcut={shortcut} description={description}>
        {children}
      </ItemBody>
    </DM.Item>
  );
}

export interface MenuCheckboxItemProps extends ComponentPropsWithRef<typeof DM.CheckboxItem>, ItemDecor {}

/** Checkable item (`menuitemcheckbox`), e.g. View ▸ Ruler. */
export function MenuCheckboxItem({
  icon,
  shortcut,
  description,
  className,
  children,
  ...rest
}: MenuCheckboxItemProps) {
  return (
    <DM.CheckboxItem className={cx('kiln-menu__item', className)} {...rest}>
      <DM.ItemIndicator className="kiln-menu__indicator">
        <Icon icon={Check} size={14} />
      </DM.ItemIndicator>
      <ItemBody icon={icon} shortcut={shortcut} description={description}>
        {children}
      </ItemBody>
    </DM.CheckboxItem>
  );
}

export interface MenuRadioItemProps extends ComponentPropsWithRef<typeof DM.RadioItem>, ItemDecor {}

/** Exclusive choice (`menuitemradio`), inside a `MenuRadioGroup`. */
export function MenuRadioItem({
  icon,
  shortcut,
  description,
  className,
  children,
  ...rest
}: MenuRadioItemProps) {
  return (
    <DM.RadioItem className={cx('kiln-menu__item', className)} {...rest}>
      <DM.ItemIndicator className="kiln-menu__indicator">
        <Icon icon={Dot} size={20} strokeWidth={4} />
      </DM.ItemIndicator>
      <ItemBody icon={icon} shortcut={shortcut} description={description}>
        {children}
      </ItemBody>
    </DM.RadioItem>
  );
}

export function MenuLabel({ className, ...rest }: ComponentPropsWithRef<typeof DM.Label>) {
  return <DM.Label className={cx('kiln-menu__group-label', className)} {...rest} />;
}

export function MenuSeparator({ className, ...rest }: ComponentPropsWithRef<typeof DM.Separator>) {
  return <DM.Separator className={cx('kiln-menu__separator', className)} {...rest} />;
}

export interface SubmenuProps extends ItemDecor {
  label: ReactNode;
  children: ReactNode;
  disabled?: boolean;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
}

/** Nested menu: opens on hover, → / Enter; ← or Esc closes back to the parent item. */
export function Submenu({
  label,
  icon,
  description,
  children,
  disabled,
  open,
  defaultOpen,
  onOpenChange,
}: SubmenuProps) {
  return (
    <DM.Sub open={open} defaultOpen={defaultOpen} onOpenChange={onOpenChange}>
      <DM.SubTrigger className="kiln-menu__item kiln-menu__item--sub" disabled={disabled}>
        <ItemBody icon={icon} description={description}>
          {label}
        </ItemBody>
        <span className="kiln-menu__chevron" aria-hidden="true">
          <Icon icon={ChevronRight} size={14} />
        </span>
      </DM.SubTrigger>
      <DM.Portal>
        <DM.SubContent className="kiln-menu" sideOffset={2} alignOffset={-5} collisionPadding={8} loop>
          {children}
        </DM.SubContent>
      </DM.Portal>
    </DM.Sub>
  );
}
