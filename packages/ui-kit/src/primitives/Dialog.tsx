import * as RD from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import type { ComponentPropsWithRef, ReactNode } from 'react';
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

export function DialogContent({
  title,
  description,
  footer,
  footerStart,
  size = 'md',
  closeButton = true,
  closeLabel = 'Close',
  className,
  children,
  ...rest
}: DialogContentProps) {
  return (
    <RD.Portal>
      <RD.Overlay className="of-dialog-overlay" />
      <RD.Content
        className={cx('of-dialog', `of-dialog--${size}`, className)}
        // Without a description, tell Radix there is intentionally none (no console warning).
        {...(description ? {} : { 'aria-describedby': undefined })}
        {...rest}
      >
        <div className="of-dialog__header">
          <RD.Title className="of-dialog__title">{title}</RD.Title>
          {closeButton ? (
            <RD.Close
              className="of-btn of-btn--ghost of-btn--sm of-btn--icon-only of-dialog__close"
              aria-label={closeLabel}
            >
              <Icon icon={X} size={16} />
            </RD.Close>
          ) : null}
        </div>
        {description ? (
          <RD.Description className="of-dialog__description">{description}</RD.Description>
        ) : null}
        <div className="of-dialog__body">{children}</div>
        {footer || footerStart ? (
          <div className="of-dialog__footer">
            <div className="of-dialog__footer-start">{footerStart}</div>
            <div className="of-dialog__footer-end">{footer}</div>
          </div>
        ) : null}
      </RD.Content>
    </RD.Portal>
  );
}
