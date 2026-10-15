import { ChevronDown, X } from 'lucide-react';
import {
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useId,
  useRef,
  useState,
} from 'react';
import { CommandIcon } from '../chrome/commandIcons.tsx';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import { Menu, MenuContent, MenuItem, MenuTrigger } from '../primitives/Menu.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';
import './panes.css';

export interface TaskPane {
  id: string;
  title: string;
  /** Icon name (glyph or Lucide) for the tab. */
  icon?: string;
  content: ReactNode;
}

export type PaneSide = 'left' | 'right';

export interface TaskPaneHostProps {
  panes: readonly TaskPane[];
  activeId?: string;
  onActiveChange?: (id: string) => void;
  /** Close button (and Esc inside the pane header). */
  onClose?: (id: string) => void;
  side?: PaneSide;
  /** "Move" in the pane options menu. */
  onSideChange?: (side: PaneSide) => void;
  /** Controlled width in px. */
  width?: number;
  defaultWidth?: number;
  onWidthChange?: (width: number) => void;
  minWidth?: number;
  maxWidth?: number;
  className?: string;
}

/**
 * Office task pane host (Styles, Comments, Format Shape…): docks left or right, resizes by dragging
 * or with ←/→ on the splitter, and shows several panes as tabs. Options menu: Move, Size, Close.
 */
export function TaskPaneHost({
  panes,
  activeId,
  onActiveChange,
  onClose,
  side = 'right',
  onSideChange,
  width: widthProp,
  defaultWidth = 320,
  onWidthChange,
  minWidth = 240,
  maxWidth = 640,
  className,
}: TaskPaneHostProps) {
  const id = useId();
  const [widthState, setWidthState] = useState(defaultWidth);
  const width = Math.min(maxWidth, Math.max(minWidth, widthProp ?? widthState));
  const [activeState, setActiveState] = useState(panes[0]?.id);
  const active = panes.find((p) => p.id === (activeId ?? activeState)) ?? panes[0];
  const splitterRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; w: number } | null>(null);

  if (!active) return null;

  const setWidth = (w: number) => {
    const clamped = Math.round(Math.min(maxWidth, Math.max(minWidth, w)));
    if (widthProp === undefined) setWidthState(clamped);
    onWidthChange?.(clamped);
  };
  const activate = (pid: string) => {
    if (activeId === undefined) setActiveState(pid);
    onActiveChange?.(pid);
  };
  // Dragging toward the document shrinks the pane; the sign depends on the docking side.
  const dir = side === 'right' ? -1 : 1;
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    drag.current = { x: e.clientX, w: width };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    setWidth(drag.current.w + dir * (e.clientX - drag.current.x));
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const onSplitterKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 64 : 16;
    if (e.key === 'ArrowLeft') setWidth(width + (side === 'right' ? step : -step));
    else if (e.key === 'ArrowRight') setWidth(width + (side === 'right' ? -step : step));
    else if (e.key === 'Home') setWidth(minWidth);
    else if (e.key === 'End') setWidth(maxWidth);
    else return;
    e.preventDefault();
  };

  const onTabKey = (e: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = panes[(index + d + panes.length) % panes.length];
    if (next) {
      activate(next.id);
      document.getElementById(`${id}-tab-${next.id}`)?.focus();
    }
  };

  return (
    <aside
      className={cx('of-taskpane', `of-taskpane--${side}`, className)}
      style={{ '--pane-width': `${width}px` } as CSSProperties}
      aria-label={active.title}
      onKeyDown={(e) => {
        if (
          e.key === 'Escape' &&
          onClose &&
          !e.defaultPrevented &&
          (e.target as HTMLElement).closest('.of-taskpane__header')
        ) {
          e.preventDefault();
          onClose(active.id);
        }
      }}
    >
      {/* biome-ignore lint/a11y/useSemanticElements: a focusable window splitter (WAI-ARIA separator), not an <hr> */}
      <div
        ref={splitterRef}
        className="of-taskpane__splitter"
        role="separator"
        aria-orientation="vertical"
        aria-label={`Resize ${active.title} pane`}
        aria-valuenow={width}
        aria-valuemin={minWidth}
        aria-valuemax={maxWidth}
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onKeyDown={onSplitterKey}
      />
      <div className="of-taskpane__header">
        {panes.length > 1 ? (
          <div className="of-taskpane__tabs" role="tablist" aria-label="Panes">
            {panes.map((p, i) => (
              <button
                key={p.id}
                id={`${id}-tab-${p.id}`}
                type="button"
                role="tab"
                aria-selected={p.id === active.id}
                aria-controls={`${id}-panel`}
                tabIndex={p.id === active.id ? 0 : -1}
                className={cx('of-taskpane__tab', p.id === active.id && 'is-active')}
                onClick={() => activate(p.id)}
                onKeyDown={(e) => onTabKey(e, i)}
              >
                <CommandIcon name={p.icon} size={14} />
                <span>{p.title}</span>
              </button>
            ))}
          </div>
        ) : (
          <h2 className="of-taskpane__title" id={`${id}-tab-${active.id}`}>
            {active.title}
          </h2>
        )}
        <div className="of-taskpane__actions">
          <Menu modal={false}>
            <Tooltip content="Task pane options">
              <MenuTrigger asChild>
                <button
                  type="button"
                  className="of-btn of-btn--ghost of-btn--sm of-btn--icon-only"
                  aria-label="Task pane options"
                >
                  <Icon icon={ChevronDown} size={14} />
                </button>
              </MenuTrigger>
            </Tooltip>
            <MenuContent align="end">
              {onSideChange ? (
                <MenuItem onSelect={() => onSideChange(side === 'right' ? 'left' : 'right')}>
                  Move to the {side === 'right' ? 'left' : 'right'}
                </MenuItem>
              ) : null}
              <MenuItem
                onSelect={() => requestAnimationFrame(() => splitterRef.current?.focus())}
                description="Then use ← and →"
              >
                Size
              </MenuItem>
              {onClose ? <MenuItem onSelect={() => onClose(active.id)}>Close</MenuItem> : null}
            </MenuContent>
          </Menu>
          {onClose ? (
            <Tooltip content={`Close ${active.title}`}>
              <button
                type="button"
                className="of-btn of-btn--ghost of-btn--sm of-btn--icon-only"
                aria-label={`Close ${active.title}`}
                onClick={() => onClose(active.id)}
              >
                <Icon icon={X} size={14} />
              </button>
            </Tooltip>
          ) : null}
        </div>
      </div>
      <div
        className="of-taskpane__body"
        id={`${id}-panel`}
        {...(panes.length > 1 ? { role: 'tabpanel', 'aria-labelledby': `${id}-tab-${active.id}` } : {})}
      >
        {active.content}
      </div>
    </aside>
  );
}
