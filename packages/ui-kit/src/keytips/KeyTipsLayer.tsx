import { assignKeyTips, type KeyTipNode, KeyTipSession, normalizeKeyTip } from '@kiln/commands';
import { isRtl, t } from '@kiln/i18n';
import { type RefObject, useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import './keytips.css';

/** Keys that leave Key Tips and go on to the focused control. */
const NAVIGATION: ReadonlySet<string> = new Set([
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
  'Enter',
  ' ',
  'Tab',
  'Home',
  'End',
]);

/**
 * DOM protocol (the Ribbon and AppFrame set these attributes):
 * - `data-keytip="H"`: the element's Key Tip;
 * - `data-keytip-level="root"` or `"tab:<id>"`: which level it belongs to;
 * - `data-keytip-enter="tab:<id>"`: choosing it opens that level (a ribbon tab).
 *
 * A Key Tip on a menu button (`aria-haspopup="menu"`) opens the menu and shows Key Tips on its
 * items (Alt, H, B, B = Bottom Border), as Office does; an item that opens a submenu goes one
 * level deeper. Item letters are the item's `data-keytip` where set, else derived from its label
 * (unique and prefix-free within the menu).
 */
const isVisible = (el: HTMLElement) =>
  !el.closest('[hidden], [aria-hidden="true"], [inert]') && el.isConnected;

/** The element a Key Tip opens a menu from, if it does. */
function menuTrigger(el: HTMLElement): HTMLElement | null {
  const popup = el.getAttribute('aria-haspopup');
  return popup === 'menu' || popup === 'true' ? el : null;
}

const ITEM = '[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]';

/** The open menu a trigger controls (Radix links them with `aria-controls`), else the last one. */
function openMenuOf(trigger: HTMLElement): HTMLElement | null {
  const id = trigger.getAttribute('aria-controls');
  const byId = id ? document.getElementById(id) : null;
  if (byId?.matches('[role="menu"]')) return byId;
  const all = [...document.querySelectorAll<HTMLElement>('[role="menu"][data-state="open"]')];
  return all.at(-1) ?? null;
}

/** The Key Tips of a menu's items. */
export function menuKeyTips(menu: HTMLElement): { el: HTMLElement; tip: string; label: string }[] {
  const items = [...menu.querySelectorAll<HTMLElement>(ITEM)].filter(
    (el) => el.closest('[role="menu"]') === menu && el.dataset.disabled === undefined,
  );
  const labelOf = (el: HTMLElement) =>
    (
      el.querySelector('.kiln-menu__label')?.textContent ??
      el.getAttribute('aria-label') ??
      el.textContent ??
      ''
    ).trim();
  const tips = assignKeyTips(
    items.map((el, i) => ({
      id: String(i),
      label: labelOf(el),
      preferred: normalizeKeyTip(el.dataset.keytip),
    })),
  );
  return items.map((el, i) => ({ el, tip: tips.get(String(i)) ?? '', label: labelOf(el) }));
}

/**
 * The menu button of a Key Tip target: the target itself, or a split button's arrow (a split
 * button's letters open its menu, as in Office; Enter there runs the button's own command).
 */
function menuOf(el: HTMLElement): HTMLElement | null {
  return menuTrigger(el) ?? el.querySelector<HTMLElement>(':scope .kiln-split__arrow[aria-haspopup="menu"]');
}

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
      kind: el.dataset.keytipEnter || menuOf(el) ? 'container' : 'action',
    });
  }
  return { nodes, elements };
}

/** Opens a menu from the keyboard (Radix opens on keys and pointer presses, not on `click`). */
function openMenu(trigger: HTMLElement, key: 'ArrowDown' | 'ArrowRight'): void {
  trigger.focus();
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
}

function activate(el: HTMLElement): void {
  // A gallery: the keys go to its chosen (or first) item, to pick one with the arrows and Enter.
  if (!el.matches('button, [role="option"]')) {
    const option =
      el.querySelector<HTMLElement>('[role="option"][aria-selected="true"]') ??
      el.querySelector<HTMLElement>('[role="option"]');
    if (option) {
      option.focus();
      return;
    }
  }
  const target = el.matches('button, input, [role="combobox"], [role="spinbutton"], a[href]')
    ? el
    : (el.querySelector<HTMLElement>('input, [role="combobox"], [role="spinbutton"], button') ?? el);
  // A check box (Table Style Options, View ▸ Gridlines) toggles, as a click does.
  if (target.matches('input[type="checkbox"], input[type="radio"]')) {
    target.focus();
    target.click();
    return;
  }
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
  /** A key the layer sends itself (opening a menu) passes its own listener. */
  const opening = useRef(false);

  const refresh = useCallback(() => {
    const s = session.current;
    if (!s) return setBadges([]);
    const out: Badge[] = [];
    const rtl = isRtl();
    for (const n of s.visible) {
      const el = elements.current.get(n.id);
      if (!el?.isConnected) continue;
      const r = el.getBoundingClientRect();
      const isTab = !!el.dataset.keytipEnter || el.getAttribute('role') === 'tab';
      const isItem = el.matches(ITEM);
      out.push({
        id: n.id,
        tip: n.tip,
        // An item's tip sits at its start edge: the left, or the right in a right-to-left interface.
        x: isItem ? (rtl ? r.right - 14 : r.left + 14) : r.left + r.width / 2,
        y: isItem ? r.top + r.height / 2 - 2 : isTab ? r.bottom - 4 : r.top + r.height - 2,
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
        } else if (el) {
          const sub = el.matches(ITEM);
          const trigger = sub ? el : (menuOf(el) ?? el);
          opening.current = true;
          openMenu(trigger, sub ? 'ArrowRight' : 'ArrowDown');
          opening.current = false;
          const levelId = `menu:${r.node.id}`;
          levelStack.current.push(levelId);
          // The menu renders in a portal after the key; collect its items once it is there.
          const tryLoad = (left: number) =>
            requestAnimationFrame(() => {
              const menu = sub
                ? document.getElementById(el.getAttribute('aria-controls') ?? '')
                : openMenuOf(trigger);
              if (menu || left === 0) loadMenuInto(menu, levelId);
              else tryLoad(left - 1);
            });
          tryLoad(10);
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

  const loadMenuInto = (menu: HTMLElement | null, levelId: string) => {
    const s = session.current;
    if (!s) return;
    const nodes: KeyTipNode[] = [];
    for (const { el, tip, label } of menu ? menuKeyTips(menu) : []) {
      const id = `${levelId}:${tip}`;
      elements.current.set(id, el);
      nodes.push({
        id,
        tip,
        label,
        kind: el.getAttribute('aria-haspopup') === 'menu' ? 'container' : 'action',
      });
    }
    s.back();
    s.enter(nodes);
    refresh();
  };

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
      if (opening.current) return;
      if (['Alt', 'Shift', 'Control', 'Meta', 'AltGraph'].includes(e.key)) {
        // A second Alt press ends Key Tips (Office toggles them).
        if (e.key === 'Alt' && e.type === 'keydown') {
          e.preventDefault();
          exit();
        }
        return;
      }
      // The arrows, Enter, Space, Tab, Home and End end Key Tips and act where the focus is (an
      // item of the menu the letters opened, a control of the ribbon), as in Office.
      if (NAVIGATION.has(e.key)) {
        exit();
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const s = session.current;
      if (!s) return exit();
      if (e.key === 'Escape') {
        const typed = s.prefix !== '';
        const r = s.back();
        if (r.type === 'exit') return exit();
        if (s.depth < levelStack.current.length - 1) {
          const left = levelStack.current.pop();
          // Leaving a menu's Key Tips closes the menu, as Esc does in Office.
          if (!typed && left?.startsWith('menu:')) {
            const menu = [...document.querySelectorAll<HTMLElement>('[role="menu"][data-state="open"]')].at(
              -1,
            );
            const item = menu?.querySelector<HTMLElement>(ITEM);
            opening.current = true;
            (item ?? menu)?.dispatchEvent(
              new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
            );
            opening.current = false;
          }
        }
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
      <span className="kiln-visually-hidden">
        {t('Key Tips on. Type a letter, or press Escape to go back.')}
      </span>
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
