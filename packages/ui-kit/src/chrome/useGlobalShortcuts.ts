import type { CommandRegistry, Keymap } from '@kiln/commands';
import { useEffect, useRef } from 'react';

export interface GlobalShortcutsOptions {
  registry: CommandRegistry;
  keymap: Keymap | undefined;
  /** Element whose subtree the shortcuts apply to; default the whole window. */
  target?: HTMLElement | Window | null;
  enabled?: boolean;
  /** Alt + a letter with no shortcut of its own (Office: Alt+H opens Home's Key Tips). */
  onAltLetter?: (letter: string) => void;
}

/** Elements that opt out (modal dialogs other than the palette, the Options keyboard capture box). */
const OPT_OUT = '[data-kiln-shortcuts="off"]';

/**
 * Form fields of the chrome that edit text themselves (a ribbon's number box, a pane's search
 * box, a dialog's field). The documents' own inputs keep every shortcut.
 */
const TEXT_FIELD =
  ':is(.kiln-ribbon, .kiln-titlebar, .kiln-statusbar, .kiln-backstage, aside, [role="dialog"]) :is(textarea, input:not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="range"]))';

/**
 * Keys a text field handles itself: in one, Ctrl+A selects its text, not the document's, and the
 * clipboard, undo and word keys act on its text.
 */
function fieldKey(e: KeyboardEvent): boolean {
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if ((e.ctrlKey || e.metaKey) && !e.altKey)
    return [
      'a',
      'c',
      'v',
      'x',
      'z',
      'y',
      'Backspace',
      'Delete',
      'ArrowLeft',
      'ArrowRight',
      'Home',
      'End',
    ].includes(k);
  return !e.altKey && ['Home', 'End', 'Backspace', 'Delete'].includes(k);
}

/**
 * Dispatches key presses through the keymap to the registry. Only implemented, enabled commands
 * take the key (and prevent the browser default); keys bound to pending commands fall through, so
 * native editing (Ctrl+C, Ctrl+Arrows) keeps working until an editor implements them. Bare Alt fires
 * on release when no other key was pressed in between (Key Tips).
 */
export function useGlobalShortcuts({
  registry,
  keymap,
  target,
  enabled = true,
  onAltLetter,
}: GlobalShortcutsOptions) {
  const altAlone = useRef(false);
  const altLetter = useRef(onAltLetter);
  altLetter.current = onAltLetter;

  useEffect(() => {
    if (!enabled || !keymap) return;
    const el: HTMLElement | Window = target ?? window;
    const onKeyDown = (ev: Event) => {
      const e = ev as KeyboardEvent;
      altAlone.current = e.key === 'Alt' && !e.ctrlKey && !e.metaKey && !e.shiftKey;
      if (e.defaultPrevented || e.isComposing) return;
      if ((e.target as Element | null)?.closest?.(OPT_OUT)) return;
      if ((e.target as Element | null)?.matches?.(TEXT_FIELD) && fieldKey(e)) return;
      const ctx = registry.context;
      const binding = keymap.resolve(e, ctx);
      if (binding) {
        const state = registry.getState(binding.command);
        if (state.exists && !state.pending && state.visible && state.enabled) {
          e.preventDefault();
          e.stopPropagation();
          void registry.execute(binding.command, binding.args);
          return;
        }
      }
      if (
        e.altKey &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.shiftKey &&
        /^Key[A-Z]$/.test(e.code) &&
        altLetter.current
      ) {
        e.preventDefault();
        altLetter.current(e.code.slice(3));
      }
    };
    const onKeyUp = (ev: Event) => {
      const e = ev as KeyboardEvent;
      if (e.key !== 'Alt' || !altAlone.current) return;
      altAlone.current = false;
      if ((e.target as Element | null)?.closest?.(OPT_OUT)) return;
      const binding = keymap.resolve(e, registry.context, { release: true });
      if (!binding) return;
      const state = registry.getState(binding.command);
      if (state.enabled) {
        e.preventDefault();
        void registry.execute(binding.command, binding.args);
      }
    };
    const onPointer = () => {
      altAlone.current = false;
    };
    // Any other key while Alt is down (an editor's own Alt+X, handled before this listener sees
    // it) means Alt was not pressed alone: its release opens no Key Tips.
    const onAnyKey = (ev: Event) => {
      if ((ev as KeyboardEvent).key !== 'Alt') altAlone.current = false;
    };
    el.addEventListener('keydown', onAnyKey, true);
    el.addEventListener('keydown', onKeyDown);
    el.addEventListener('keyup', onKeyUp);
    el.addEventListener('pointerdown', onPointer, true);
    return () => {
      el.removeEventListener('keydown', onAnyKey, true);
      el.removeEventListener('keydown', onKeyDown);
      el.removeEventListener('keyup', onKeyUp);
      el.removeEventListener('pointerdown', onPointer, true);
    };
  }, [registry, keymap, target, enabled]);
}
