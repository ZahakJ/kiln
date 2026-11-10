import { t } from '@kiln/i18n';
import * as RD from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { type ComponentPropsWithRef, type ReactNode, type RefObject, useEffect, useRef } from 'react';
import { returnToDocument } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

/** Modal dialog root (Radix): focus trap, Esc to close, focus returns to the trigger, scroll lock. */
export const Dialog = RD.Root;
export const DialogTrigger = RD.Trigger;
export const DialogClose = RD.Close;

export interface DialogContentProps extends Omit<ComponentPropsWithRef<typeof RD.Content>, 'title'> {
  /** Title bar text (required for an accessible name). */
  title: ReactNode;
  /** Optional sentence under the title, wired to `aria-describedby`. */
  description?: ReactNode;
  /** Right-aligned action buttons (OK / Cancel). */
  footer?: ReactNode;
  /** Left side of the footer (e.g. "Set As Default" in Office's Font dialog). */
  footerStart?: ReactNode;
  /** sm 360, md 480 (default), lg 640, xl 840 px. */
  size?: 'sm' | 'md' | 'lg' | 'xl';
  /** Show the × close button. Default true. */
  closeButton?: boolean;
  closeLabel?: string;
}

/** The fields a dialog's keys start in. */
const FIRST_FIELD =
  'input:not([type="hidden"]):not([disabled]):not([tabindex="-1"]), textarea:not([disabled]), select:not([disabled]), [role="combobox"]:not([disabled])';

export function DialogContent({
  title,
  description,
  footer,
  footerStart,
  size = 'md',
  closeButton = true,
  closeLabel = t('Close'),
  className,
  children,
  onCloseAutoFocus,
  onOpenAutoFocus,
  ...rest
}: DialogContentProps) {
  const order = useRef(0);
  const opener = useRef<Element | null>(null);
  return (
    <RD.Portal>
      <RD.Overlay className="kiln-dialog-overlay" />
      <RD.Content
        className={cx('kiln-dialog', `kiln-dialog--${size}`, className)}
        // Without a description, tell Radix there is intentionally none (no console warning).
        {...(description ? {} : { 'aria-describedby': undefined })}
        {...rest}
        onOpenAutoFocus={(e) => {
          onOpenAutoFocus?.(e);
          if (e.defaultPrevented) return;
          // As in Office, the keys start in the dialog's first field (Go To's Reference, Define
          // Name's Name), not on its Close button; a dialog without one starts where Radix puts it.
          const content = e.currentTarget as HTMLElement | null;
          // A field that took the focus itself (autoFocus) keeps it.
          const active = document.activeElement;
          if (content && active && active !== content && content.contains(active)) {
            e.preventDefault();
            return;
          }
          const body = content?.querySelector('.kiln-dialog__body');
          const field = body?.querySelector<HTMLElement>(FIRST_FIELD);
          if (field && content?.getAttribute('role') !== 'alertdialog') {
            e.preventDefault();
            field.focus();
          }
        }}
        onCloseAutoFocus={keepNextDialogFocus(order, onCloseAutoFocus, opener)}
      >
        <OpenOrder order={order} opener={opener} />
        <div className="kiln-dialog__header">
          <RD.Title className="kiln-dialog__title">{title}</RD.Title>
          {closeButton ? (
            <RD.Close
              className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-btn--icon-only kiln-dialog__close"
              aria-label={closeLabel}
            >
              <Icon icon={X} size={16} />
            </RD.Close>
          ) : null}
        </div>
        {description ? (
          <RD.Description className="kiln-dialog__description">{description}</RD.Description>
        ) : null}
        <div className="kiln-dialog__body">{children}</div>
        {footer || footerStart ? (
          <div className="kiln-dialog__footer">
            <div className="kiln-dialog__footer-start">{footerStart}</div>
            <div className="kiln-dialog__footer-end">{footer}</div>
          </div>
        ) : null}
      </RD.Content>
    </RD.Portal>
  );
}

/**
 * The element that last took the focus. A dialog that opens as the menu or Backstage that asked
 * for it unmounts finds the focus already on the page body; this is what had it.
 */
let lastFocused: Element | null = null;
if (typeof document !== 'undefined')
  document.addEventListener(
    'focusin',
    (e) => {
      if (e.target instanceof Element) lastFocused = e.target;
    },
    true,
  );

/** Transient chrome a dialog may be opened from: the Backstage, a menu, a popover. */
const TRANSIENT_OPENER = '.kiln-backstage, [role="menu"], [role="menuitem"], .kiln-menu, .kiln-popover';

/** The open dialogs, by the order they opened in. */
const openDialogs = new Set<number>();
let opened = 0;

/**
 * Rendered inside the content: notes when this dialog opened and while it stays open, and what had
 * the focus before it.
 */
function OpenOrder({ order, opener }: { order: RefObject<number>; opener: RefObject<Element | null> }) {
  // Noted as the dialog first renders.
  const noted = useRef(false);
  if (!noted.current && typeof document !== 'undefined') {
    noted.current = true;
    const a = document.activeElement;
    opener.current = a && a !== document.body ? a : lastFocused;
  }
  useEffect(() => {
    opened += 1;
    const n = opened;
    order.current = n;
    openDialogs.add(n);
    return () => {
      openDialogs.delete(n);
    };
  }, [order]);
  return null;
}

/**
 * A dialog that opens while this one closes (Goal Seek then Goal Seek Status, Protect Sheet then
 * Confirm Password) keeps the keys. Radix moves the focus a task after this dialog unmounts, when
 * the next one may already have focused its first field; then neither the trigger nor the caller's
 * handler (which may send the keys to the document) takes the focus from it. A dialog opened from
 * inside an older one still returns the focus to its trigger there.
 */
function keepNextDialogFocus(
  order: RefObject<number>,
  user?: (e: Event) => void,
  opener?: RefObject<Element | null>,
): (e: Event) => void {
  return (e: Event) => {
    const mine = order.current;
    const newer = Array.from(openDialogs).some((n) => n > mine);
    const a = typeof document !== 'undefined' ? document.activeElement : null;
    const host = a?.closest('[role="dialog"]:not(.kiln-popover), [role="alertdialog"]');
    if (newer || (host?.isConnected && host !== e.currentTarget)) {
      e.preventDefault();
      return;
    }
    user?.(e);
    if (e.defaultPrevented) return;
    // Opened from transient chrome that has gone since (File ▸ Options from the Backstage, a
    // dialog from a context menu's item): Radix would leave the focus on the page body, where no
    // key reaches the document. The keys go back to the document instead, as in Office.
    const from = opener?.current ?? null;
    if (from && !from.isConnected && from.closest(TRANSIENT_OPENER)) {
      e.preventDefault();
      returnToDocument(null);
    }
  };
}
