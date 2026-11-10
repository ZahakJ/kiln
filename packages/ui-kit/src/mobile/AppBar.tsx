/**
 * The phone's top app bar (56 px): Back, the document's name (tap to rename), Undo,
 * Redo, Search (the command palette) and the overflow menu (Share, Save, Save As, Print, Version
 * History, Find and Replace, the app's own view toggles, File, Settings). It keeps the class of the
 * desktop title bar, so F6, the Android shell's chrome checks and the classic look still find it.
 */
import { t } from '@kiln/i18n';
import { ArrowLeft, EllipsisVertical, Redo2, Search, Undo2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useCommandShortcut, useCommandState, useExecute } from '../chrome/CommandContext.tsx';
import { CommandMenuItems, type MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import { Icon } from '../icons/Icon.tsx';
import { Menu, MenuContent, MenuTrigger } from '../primitives/Menu.tsx';
import { ariaKeyShortcuts } from '../primitives/shortcut.ts';
import './mobile.css';

export interface PhoneAppBarProps {
  /** The document's name (the title bar's rename button). */
  title: ReactNode;
  /** Back: closes the document (asking about unsaved changes) and shows Kiln Home. */
  onBack?: () => void;
  /** Search: the command palette ("Search / Tell me"). */
  onSearch: () => void;
  /** The overflow menu's entries. */
  menu: readonly MenuEntry[];
  /** The collaborators' avatars while the document is shared. */
  presence?: ReactNode;
}

function BarCommand({ command, icon, label }: { command: string; icon: typeof Undo2; label: string }) {
  const state = useCommandState(command);
  const shortcut = useCommandShortcut(command);
  const execute = useExecute();
  if (!state.visible) return null;
  return (
    <button
      type="button"
      className="kiln-appbar__btn"
      aria-label={label}
      aria-keyshortcuts={shortcut ? ariaKeyShortcuts(shortcut) : undefined}
      aria-disabled={!state.enabled || undefined}
      data-command={command}
      onClick={() => state.enabled && execute(command)}
    >
      <Icon icon={icon} size={20} />
    </button>
  );
}

export function PhoneAppBar({ title, onBack, onSearch, menu, presence }: PhoneAppBarProps) {
  const paletteShortcut = useCommandShortcut('ui.palette.open');
  return (
    <header className="kiln-titlebar kiln-appbar" data-testid="app-bar">
      {onBack ? (
        <button
          type="button"
          className="kiln-appbar__btn kiln-appbar__back"
          aria-label={t('Back')}
          data-testid="app-bar-back"
          onClick={onBack}
        >
          <Icon icon={ArrowLeft} size={22} />
        </button>
      ) : null}
      <div className="kiln-appbar__title">{title}</div>
      {presence ? <div className="kiln-titlebar__presence kiln-appbar__presence">{presence}</div> : null}
      <BarCommand command="suite.edit.undo" icon={Undo2} label={t('Undo')} />
      <BarCommand command="suite.edit.redo" icon={Redo2} label={t('Redo')} />
      <button
        type="button"
        className="kiln-appbar__btn"
        aria-label={t('Search commands')}
        aria-haspopup="dialog"
        aria-keyshortcuts={paletteShortcut ? ariaKeyShortcuts(paletteShortcut) : undefined}
        onClick={onSearch}
      >
        <Icon icon={Search} size={20} />
      </button>
      <Menu modal={false}>
        <MenuTrigger asChild>
          <button
            type="button"
            className="kiln-appbar__btn"
            aria-label={t('More options')}
            data-testid="app-bar-overflow"
          >
            <Icon icon={EllipsisVertical} size={20} />
          </button>
        </MenuTrigger>
        <MenuContent align="end" className="kiln-appbar__menu" sideOffset={6}>
          <CommandMenuItems entries={menu} />
        </MenuContent>
      </Menu>
    </header>
  );
}
