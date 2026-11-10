/**
 * A bottom sheet: a modal panel that rises from the bottom edge of a phone, with a
 * grab handle. It is a Radix dialog (`role="dialog"`, `data-state="open"`), so Escape, the Back
 * button and Back in the Android app close it like any other layer; dragging the handle down
 * closes it too. The ribbon sheet, the share sheet and the editors' phone pickers use it.
 */
import { t } from '@kiln/i18n';
import * as RD from '@radix-ui/react-dialog';
import { X } from 'lucide-react';
import { type PointerEvent, type ReactNode, useRef } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import './mobile.css';

/** How far the handle must travel down before the sheet closes, in CSS px. */
const DISMISS_PX = 72;

export interface BottomSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The sheet's name (its heading, or only its accessible name with `titleHidden`). */
  title: ReactNode;
  titleHidden?: boolean;
  /** A sentence under the title (`aria-describedby`). */
  description?: ReactNode;
  /** Content of the header after the title (a tab strip, an action). */
  header?: ReactNode;
  children?: ReactNode;
  className?: string;
  /** `data-testid` of the panel. */
  testId?: string;
  /** Called when the sheet has closed and given the keys back. */
  onCloseAutoFocus?: (e: Event) => void;
}

export function BottomSheet({
  open,
  onOpenChange,
  title,
  titleHidden,
  description,
  header,
  children,
  className,
  testId,
  onCloseAutoFocus,
}: BottomSheetProps) {
  const panel = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: number; y: number; dy: number } | null>(null);

  const onDown = (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    drag.current = { id: e.pointerId, y: e.clientY, dy: 0 };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onMove = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    d.dy = Math.max(0, e.clientY - d.y);
    panel.current?.style.setProperty('--kiln-sheet-drag', `${d.dy}px`);
  };
  const onUp = (e: PointerEvent<HTMLElement>) => {
    const d = drag.current;
    if (!d || d.id !== e.pointerId) return;
    drag.current = null;
    panel.current?.style.removeProperty('--kiln-sheet-drag');
    if (d.dy >= DISMISS_PX) onOpenChange(false);
  };

  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Overlay className="kiln-sheet-overlay" />
        <RD.Content
          ref={panel}
          className={cx('kiln-sheet', className)}
          data-testid={testId}
          {...(description ? {} : { 'aria-describedby': undefined })}
          onCloseAutoFocus={onCloseAutoFocus}
        >
          <div
            className="kiln-sheet__grip"
            aria-hidden="true"
            onPointerDown={onDown}
            onPointerMove={onMove}
            onPointerUp={onUp}
            onPointerCancel={onUp}
          >
            <span className="kiln-sheet__handle" />
          </div>
          <div className="kiln-sheet__header">
            <RD.Title className={cx('kiln-sheet__title', titleHidden && 'kiln-visually-hidden')}>
              {title}
            </RD.Title>
            {header}
            <RD.Close className="kiln-sheet__close" aria-label={t('Close')}>
              <Icon icon={X} size={20} />
            </RD.Close>
          </div>
          {description ? (
            <RD.Description className="kiln-sheet__description">{description}</RD.Description>
          ) : null}
          <div className="kiln-sheet__body">{children}</div>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  );
}
