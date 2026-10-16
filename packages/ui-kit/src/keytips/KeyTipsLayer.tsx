import { type KeyTipNode, KeyTipSession } from '@kiln/commands';
import { type RefObject, useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import './keytips.css';

/**
 * DOM protocol (the Ribbon and AppFrame set these attributes):
 * - `data-keytip="H"`: the element's Key Tip;
 * - `data-keytip-level="root"` or `"tab:<id>"`: which level it belongs to;
 * - `data-keytip-enter="tab:<id>"`: choosing it opens that level (a ribbon tab).
 */
const isVisible = (el: HTMLElement) =>
  !el.closest('[hidden], [aria-hidden="true"], [inert]') && el.isConnected;

function collect(
  root: HTMLElement,
  level: string,
): { nodes: KeyTipNode[]; elements: Map<string, HTMLElement> } {
  const elements = new Map<string, HTMLElement>();
  const nodes: KeyTipNode[] = [];
  const seen = new Set<string>();
  for (const el of root.querySelectorAll<HTMLElement>(
    `[data-keytip][data-keytip-level="${CSS.escape(level)}"]`,
  )) {
    const tip = el.dataset.keytip;
    if (!tip || seen.has(tip) || !isVisible(el)) continue;
    seen.add(tip);
    const id = `${level}:${tip}`;
    elements.set(id, el);
    nodes.push({
      id,
      tip,
      label: el.getAttribute('aria-label') ?? el.textContent ?? tip,
      kind: el.dataset.keytipEnter ? 'container' : 'action',
    });
  }
  return { nodes, elements };
}

function activate(el: HTMLElement): void {
  const target = el.matches('button, input, [role="combobox"], [role="spinbutton"], a[href]')
    ? el
    : (el.querySelector<HTMLElement>('input, [role="combobox"], [role="spinbutton"], button') ?? el);
  if (target.matches('input, [role="combobox"], [role="spinbutton"]')) {
    target.focus();
    if (target instanceof HTMLInputElement) target.select();
    return;
  }
  target.focus();
  target.click();
}

interface Badge {
  id: string;
  tip: string;
  x: number;
  y: number;
  disabled: boolean;
}

export interface KeyTipsLayerProps {
  /** Whether Key Tips are showing. */
  active: boolean;
  /** Called when the session ends (Esc at the top, a command ran, a click elsewhere). */
  onExit: () => void;
  /** Element to search for Key Tip targets (the app frame). */
  rootRef: RefObject<HTMLElement | null>;
  /** First letters typed with Alt held (Alt+H opens Home directly). */
  initialKeys?: string;
}

/**
 * Key Tips overlay: badges on the File button, tabs and title-bar controls; typing letters opens a
 * tab (badges move to its commands) or runs a command. Esc goes back a level, then exits.
 */
export function KeyTipsLayer({ active, onExit, rootRef, initialKeys }: KeyTipsLayerProps) {
  const session = useRef<KeyTipSession | null>(null);
  const elements = useRef(new Map<string, HTMLElement>());
  const [badges, setBadges] = useState<Badge[]>([]);
  const [prefix, setPrefix] = useState('');
  const levelStack = useRef<string[]>(['root']);

  const refresh = useCallback(() => {
    const s = session.current;
    if (!s) return setBadges([]);
    const out: Badge[] = [];
    for (const n of s.visible) {
      const el = elements.current.get(n.id);
      if (!el?.isConnected) continue;
      const r = el.getBoundingClientRect();
      const isTab = !!el.dataset.keytipEnter || el.getAttribute('role') === 'tab';
      out.push({
        id: n.id,
        tip: n.tip,
        x: r.left + r.width / 2,
        y: isTab ? r.bottom - 4 : r.top + r.height - 2,
        disabled: el.getAttribute('aria-disabled') === 'true' || (el as HTMLButtonElement).disabled === true,
      });
    }
    setBadges(out);
    setPrefix(s.prefix);
  }, []);

  const loadLevel = useCallback(
    (level: string, enter: boolean) => {
      const root = rootRef.current;
      if (!root) return;
      const { nodes, elements: els } = collect(root, level);
      for (const [k, v] of els) elements.current.set(k, v);
      if (!session.current || !enter) session.current = new KeyTipSession(nodes);
      else session.current.enter(nodes);
      refresh();
    },
    [rootRef, refresh],
  );

  const exit = useCallback(() => {
    session.current?.cancel();
    session.current = null;
    elements.current.clear();
    levelStack.current = ['root'];
    setBadges([]);
    onExit();
  }, [onExit]);

  const press = useCallback(
    (ch: string) => {
      const s = session.current;
      if (!s) return;
      const r = s.press(ch);
      if (r.type === 'enter') {
        const el = elements.current.get(r.node.id);
        const level = el?.dataset.keytipEnter;
        if (el && level) {
          activate(el);
          levelStack.current.push(level);
          // The tab's commands render after the click; collect them on the next frame.
          requestAnimationFrame(() => loadLevelInto(level));
        }
      } else if (r.type === 'invoke') {
        const el = elements.current.get(r.node.id);
        exit();
        if (el) activate(el);
        return;
      }
      refresh();
    },
    [exit, refresh],
  );

  const loadLevelInto = (level: string) => {
    const root = rootRef.current;
    const s = session.current;
    if (!root || !s) return;
    const { elements: els, nodes } = collect(root, level);
    for (const [k, v] of els) elements.current.set(k, v);
    // Replace the (empty) level the session pushed with the real children.
    s.back();
    s.enter(nodes);
    refresh();
  };

  // A passive effect: the root element's ref is attached by then.
  useEffect(() => {
    if (!active) return;
    loadLevel('root', false);
    if (initialKeys) for (const ch of initialKeys) press(ch);
  }, [active]);

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (['Alt', 'Shift', 'Control', 'Meta', 'AltGraph'].includes(e.key)) {
        // A second Alt press ends Key Tips (Office toggles them).
        if (e.key === 'Alt' && e.type === 'keydown') {
          e.preventDefault();
          exit();
        }
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const s = session.current;
      if (!s) return exit();
      if (e.key === 'Escape') {
        const r = s.back();
        if (r.type === 'exit') return exit();
        if (s.depth < levelStack.current.length - 1) levelStack.current.pop();
        refresh();
        return;
      }
      if (e.key === 'Backspace') {
        s.erase();
        refresh();
        return;
      }
      if (/^[a-z0-9]$/i.test(e.key)) {
        press(e.key);
        return;
      }
      exit();
    };
    const onPointer = () => exit();
    const onResize = () => refresh();
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('pointerdown', onPointer, true);
    window.addEventListener('resize', onResize);
    window.addEventListener('blur', onPointer);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('pointerdown', onPointer, true);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('blur', onPointer);
    };
  }, [active, exit, press, refresh]);

  if (!active || typeof document === 'undefined') return null;
  return createPortal(
    <div className="kiln-keytips" aria-live="polite" data-testid="keytips">
      <span className="kiln-visually-hidden">Key Tips on. Type a letter, or press Escape to go back.</span>
      {badges.map((b) => (
        <span
          key={b.id}
          className="kiln-keytip"
          data-keytip-badge={b.tip}
          data-disabled={b.disabled || undefined}
          style={{ left: b.x, top: b.y }}
        >
          <span className="kiln-keytip__typed">{b.tip.slice(0, prefix.length)}</span>
          {b.tip.slice(prefix.length)}
        </span>
      ))}
    </div>,
    document.body,
  );
}
