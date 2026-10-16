import * as RadixTooltip from '@radix-ui/react-tooltip';
import { createContext, type ReactElement, type ReactNode, useContext } from 'react';
import { Kbd } from './Kbd.tsx';

const ProviderPresent = createContext(false);

export interface TooltipProviderProps {
  children?: ReactNode;
  /** ms before the first tooltip opens. Office uses ~500 ms. */
  delayDuration?: number;
  /** ms during which moving to another trigger opens its tooltip instantly. */
  skipDelayDuration?: number;
}

/** Shares open-delay state between tooltips (moving along the ribbon shows tips instantly). */
export function TooltipProvider({
  children,
  delayDuration = 500,
  skipDelayDuration = 400,
}: TooltipProviderProps) {
  return (
    <RadixTooltip.Provider delayDuration={delayDuration} skipDelayDuration={skipDelayDuration}>
      <ProviderPresent.Provider value={true}>{children}</ProviderPresent.Provider>
    </RadixTooltip.Provider>
  );
}

export interface TooltipProps {
  /** The trigger: a single focusable element that accepts a ref. */
  children: ReactElement;
  /** Title line (usually the command name). */
  content: ReactNode;
  /** Platform-neutral shortcut shown after the title, e.g. `Mod+B`. */
  shortcut?: string;
  /** Office-style "super tooltip" description under the title. */
  description?: ReactNode;
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Disable without unmounting the trigger. */
  disabled?: boolean;
  delayDuration?: number;
}

/**
 * Office-style tooltip: title, optional shortcut ("Bold (Ctrl+B)") and optional description.
 * Opens on hover and keyboard focus, closes on Esc; works with or without a `TooltipProvider`.
 */
export function Tooltip({
  children,
  content,
  shortcut,
  description,
  side = 'bottom',
  align = 'center',
  open,
  defaultOpen,
  onOpenChange,
  disabled,
  delayDuration,
}: TooltipProps) {
  const hasProvider = useContext(ProviderPresent);
  if (disabled) return children;
  const tip = (
    <RadixTooltip.Root
      open={open}
      defaultOpen={defaultOpen}
      onOpenChange={onOpenChange}
      delayDuration={delayDuration}
    >
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          className={description ? 'kiln-tooltip kiln-tooltip--rich' : 'kiln-tooltip'}
          side={side}
          align={align}
          sideOffset={6}
          collisionPadding={8}
        >
          <span className="kiln-tooltip__title">
            {content}
            {shortcut ? (
              <>
                {' '}
                <span className="kiln-tooltip__shortcut">
                  (<Kbd shortcut={shortcut} variant="inline" />)
                </span>
              </>
            ) : null}
          </span>
          {description ? <span className="kiln-tooltip__description">{description}</span> : null}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
  return hasProvider ? tip : <TooltipProvider>{tip}</TooltipProvider>;
}
