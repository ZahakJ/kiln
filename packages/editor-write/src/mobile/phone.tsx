/**
 * Kiln Write on a phone: the text reflows to the screen's width (Web Layout) when a
 * document opens, and the app bar's overflow menu has a Print layout toggle that shows the pages
 * as they print. Both go through `suite.view.mode`, the View tab's own command.
 */
import type { CommandRegistry } from '@kiln/commands';
import { msg, t } from '@kiln/i18n';
import type { ViewMode } from '@kiln/protocol';
import { MenuCheckboxItem, type MenuEntry, useCommandState, useCommands } from '@kiln/ui-kit';
import { useEffect, useRef } from 'react';

/** The view a phone opens documents in. */
export const PHONE_VIEW: ViewMode = 'WebLayout';

const MODE = 'suite.view.mode';

/** The overflow menu's toggle: on shows Print Layout, off the phone's reflowing view. */
function PrintLayoutToggle() {
  const state = useCommandState(MODE);
  const { registry } = useCommands();
  if (!state.visible) return null;
  const on = state.value === 'PrintLayout';
  return (
    <MenuCheckboxItem
      checked={on}
      disabled={!state.enabled}
      data-testid="print-layout-toggle"
      onSelect={() => {
        // The pages fit the screen's width once they show.
        void registry
          .execute(MODE, { mode: on ? PHONE_VIEW : 'PrintLayout' })
          .then(() => (on ? undefined : printLayoutShown(registry)))
          .then((shown) => (shown ? registry.execute('suite.view.page_width') : undefined))
          .catch(() => undefined);
      }}
    >
      {t('Print layout')}
    </MenuCheckboxItem>
  );
}

/** Resolves `true` once the core reports Print Layout (its pages are laid out), within a second. */
function printLayoutShown(registry: CommandRegistry): Promise<boolean> {
  return new Promise((resolve) => {
    const shown = () => registry.getState(MODE).value === 'PrintLayout';
    if (shown()) {
      resolve(true);
      return;
    }
    const timer = setTimeout(() => {
      stop();
      resolve(shown());
    }, 1000);
    const stop = registry.subscribe((e) => {
      if (e.type !== 'state' || !shown()) return;
      clearTimeout(timer);
      stop();
      // A frame later the surface shows the pages it fits.
      requestAnimationFrame(() => resolve(true));
    });
  });
}

/** Write's entries in the phone's overflow menu. */
export const WRITE_PHONE_MENU: readonly MenuEntry[] = [
  { kind: 'heading', label: msg('View') },
  { kind: 'custom', id: 'print-layout', render: () => <PrintLayoutToggle /> },
];

/**
 * On a phone, each document opens reflowed to the screen (Web Layout) once the core can take the
 * view change; a view chosen afterwards (Print layout) stays. Nothing changes on a tablet or a
 * desktop.
 */
export function usePhoneReflow(registry: CommandRegistry | null, doc: number | null, phone: boolean): void {
  const done = useRef(new Set<number>());
  useEffect(() => {
    if (!registry || doc === null || !phone || done.current.has(doc)) return;
    const apply = (): boolean => {
      const st = registry.getState(MODE);
      if (!st.exists || st.pending || !st.enabled || st.value === undefined) return false;
      done.current.add(doc);
      if (st.value === 'PrintLayout')
        void registry.execute(MODE, { mode: PHONE_VIEW }).catch(() => undefined);
      return true;
    };
    if (apply()) return;
    const stop = registry.subscribe((e) => {
      if (e.type === 'state' && apply()) stop();
    });
    return stop;
  }, [registry, doc, phone]);
}
