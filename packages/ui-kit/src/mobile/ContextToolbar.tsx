/**
 * The floating context toolbar: over what a finger selected — text, cells, a shape
 * — Cut, Copy, Paste, Comment and Format, and More, which opens the same context menu as a
 * right-click there. The surfaces publish the touch selection (`touchSelection` of
 * `@kiln/surface`); a mouse, a key or a scroll that takes the selection away ends it. Every button
 * is a command of the registry; Format opens the phone's ribbon sheet on its Home tab.
 */
import { t } from '@kiln/i18n';
import { openContextMenuAt, type TouchSelection, touchSelection } from '@kiln/surface';
import { ClipboardPaste, Copy, EllipsisVertical, MessageSquarePlus, Scissors, Type } from 'lucide-react';
import {
  type CSSProperties,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useCommandState, useExecute } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Toolbar } from '../primitives/Toolbar.tsx';
import './mobile.css';

/** Space kept between the toolbar and the selection, and from the window's edges (CSS px). */
const GAP = 10;
const EDGE = 8;
/** The text handles hang under the selection: the toolbar keeps clear of them below. */
const HANDLE_DROP = 26;

export interface ContextToolbarProps {
  /** Format: the ribbon sheet on a phone (the toolbar leaves it out elsewhere). */
  onFormat?: () => void;
}

function useTouchSelection(): TouchSelection | null {
  return useSyncExternalStore(touchSelection.subscribe, touchSelection.get, () => null);
}

function Action({
  command,
  icon,
  label,
  testId,
}: {
  command: string;
  icon: typeof Copy;
  label: string;
  testId: string;
}) {
  const state = useCommandState(command);
  const execute = useExecute();
  if (!state.visible) return null;
  return (
    <button
      type="button"
      className="kiln-ctxbar__btn"
      aria-disabled={!state.enabled || undefined}
      data-command={command}
      data-testid={testId}
      // The selection keeps the keys (and the on-screen keyboard stays as it was).
      onPointerDown={(e) => e.preventDefault()}
      onClick={() => {
        if (!state.enabled) return;
        touchSelection.set(null);
        execute(command);
      }}
    >
      <Icon icon={icon} size={20} />
      <span className="kiln-ctxbar__label">{label}</span>
    </button>
  );
}

/** Where the toolbar goes: above the selection, else below it, inside the visible window. */
export function placeContextToolbar(
  sel: TouchSelection['rect'],
  size: { width: number; height: number },
  view: { width: number; height: number; top?: number },
  kind: TouchSelection['kind'],
): { left: number; top: number } | null {
  const viewTop = view.top ?? 0;
  if (sel.bottom < viewTop || sel.top > viewTop + view.height || sel.right < 0 || sel.left > view.width)
    return null;
  const cx = (sel.left + sel.right) / 2;
  const left = Math.min(Math.max(EDGE, cx - size.width / 2), Math.max(EDGE, view.width - size.width - EDGE));
  const above = sel.top - GAP - size.height;
  const drop = kind === 'text' ? HANDLE_DROP : kind === 'cells' ? HANDLE_DROP - 8 : GAP;
  const below = sel.bottom + GAP + drop;
  let top: number;
  if (above >= viewTop + EDGE) top = above;
  else if (below + size.height <= viewTop + view.height - EDGE) top = below;
  // A selection taller than the screen: the toolbar sits over its middle.
  else top = viewTop + Math.max(EDGE, (view.height - size.height) / 2);
  return { left: Math.round(left), top: Math.round(top) };
}

export function ContextToolbar({ onFormat }: ContextToolbarProps) {
  const sel = useTouchSelection();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  // A context menu opened (the long press, More): the toolbar steps aside until the next selection.
  const [aside, setAside] = useState<TouchSelection | null>(null);

  useEffect(() => {
    const onMenu = () => setAside(touchSelection.get());
    window.addEventListener('contextmenu', onMenu);
    return () => window.removeEventListener('contextmenu', onMenu);
  }, []);

  const shown = sel && sel !== aside ? sel : null;

  useLayoutEffect(() => {
    const el = ref.current;
    if (!shown || !el) {
      setPos(null);
      return;
    }
    const vv = window.visualViewport;
    let view = {
      width: window.innerWidth,
      height: vv?.height ?? window.innerHeight,
      top: vv?.offsetTop ?? 0,
    };
    // Inside the surface (off the app bar, the formula bar and the toolbars), when it has room.
    const b = shown.bounds;
    if (b) {
      const top = Math.max(view.top, b.top);
      const bottom = Math.min(view.top + view.height, b.bottom);
      if (bottom - top > el.offsetHeight * 2) view = { ...view, top, height: bottom - top };
    }
    setPos(
      placeContextToolbar(shown.rect, { width: el.offsetWidth, height: el.offsetHeight }, view, shown.kind),
    );
  }, [shown]);

  if (!shown) return null;
  const style: CSSProperties = pos
    ? { left: pos.left, top: pos.top }
    : { left: 0, top: 0, visibility: 'hidden' };
  return (
    <div ref={ref} className="kiln-ctxbar" style={style} data-testid="context-toolbar" data-kind={shown.kind}>
      <Toolbar aria-label={t('Selection actions')} className="kiln-ctxbar__row">
        <Action command="suite.edit.cut" icon={Scissors} label={t('Cut')} testId="context-cut" />
        <Action command="suite.edit.copy" icon={Copy} label={t('Copy')} testId="context-copy" />
        <Action command="suite.edit.paste" icon={ClipboardPaste} label={t('Paste')} testId="context-paste" />
        <Action
          command="suite.review.new_comment"
          icon={MessageSquarePlus}
          label={t('Comment')}
          testId="context-comment"
        />
        {onFormat ? (
          <button
            type="button"
            className="kiln-ctxbar__btn"
            aria-haspopup="dialog"
            data-testid="context-format"
            onPointerDown={(e) => e.preventDefault()}
            onClick={() => {
              touchSelection.set(null);
              onFormat();
            }}
          >
            <Icon icon={Type} size={20} />
            <span className="kiln-ctxbar__label">{t('Format')}</span>
          </button>
        ) : null}
        <button
          type="button"
          className="kiln-ctxbar__btn"
          aria-haspopup="menu"
          data-testid="context-more"
          onPointerDown={(e) => e.preventDefault()}
          onClick={() => {
            const s = touchSelection.get();
            if (s) openContextMenuAt(s.target, s.point.x, s.point.y);
          }}
        >
          <Icon icon={EllipsisVertical} size={20} />
          <span className="kiln-ctxbar__label">{t('More')}</span>
        </button>
      </Toolbar>
    </div>
  );
}
