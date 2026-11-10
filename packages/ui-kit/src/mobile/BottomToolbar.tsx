/**
 * The phone's bottom toolbar: the first button names the ribbon tab of the ribbon
 * sheet ("Home ▾") and opens it; after it, a scrolling row of the quick commands for what is
 * selected (text, a cell, a slide, a shape), drawn from the ribbon's own controls. It docks on the
 * on-screen keyboard (`--kiln-keyboard-inset`, see `keyboard.ts`).
 */
import type { AppId } from '@kiln/commands';
import { t } from '@kiln/i18n';
import { ChevronUp } from 'lucide-react';
import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { useCommands } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Toolbar } from '../primitives/Toolbar.tsx';
import { itemId } from '../ribbon/builders.ts';
import { RibbonItemView } from '../ribbon/RibbonItem.tsx';
import type { RibbonSpec } from '../ribbon/types.ts';
import { QUICK_COMMANDS, type QuickContext, quickContextOf, quickItems } from './quick.ts';
import { useSheetTabs } from './RibbonSheet.tsx';
import './mobile.css';

export interface BottomToolbarProps {
  app: AppId;
  spec: RibbonSpec;
  /** The ribbon sheet's tab (its name is on the first button). */
  tab: string;
  sheetOpen: boolean;
  onOpenSheet: () => void;
}

/** The quick-command context, following the registry's context (the editors set it). */
function useQuickContext(app: AppId): QuickContext {
  const { registry } = useCommands();
  // A context change is a state event for every command (no `ids`).
  const subscribe = useCallback(
    (cb: () => void) => registry.subscribe((e) => (e.type === 'state' && !e.ids ? cb() : undefined)),
    [registry],
  );
  const get = useCallback(() => quickContextOf(app, registry.context), [app, registry]);
  return useSyncExternalStore(subscribe, get, get);
}

export function BottomToolbar({ app, spec, tab, sheetOpen, onOpenSheet }: BottomToolbarProps) {
  const tabs = useSheetTabs(spec);
  const context = useQuickContext(app);
  const items = useMemo(() => quickItems(tabs, QUICK_COMMANDS[context]), [tabs, context]);
  const label = (tabs.find((x) => x.id === tab) ?? tabs[0])?.label ?? t('Home');
  return (
    <div className="kiln-bottombar" data-testid="bottom-toolbar" data-context={context}>
      <button
        type="button"
        className="kiln-bottombar__sheet"
        aria-haspopup="dialog"
        aria-expanded={sheetOpen}
        aria-label={t('{tab} commands', { tab: label })}
        data-testid="ribbon-sheet-button"
        onClick={onOpenSheet}
      >
        <span className="kiln-bottombar__sheet-label">{label}</span>
        <Icon icon={ChevronUp} size={16} />
      </button>
      <Toolbar aria-label={t('Quick commands')} className="kiln-bottombar__quick">
        {items.map((it, i) => (
          <RibbonItemView
            key={itemId(it, i)}
            item={it}
            size={it.kind === 'combobox' ? 'medium' : 'small'}
            layout="simplified"
          />
        ))}
      </Toolbar>
    </div>
  );
}
