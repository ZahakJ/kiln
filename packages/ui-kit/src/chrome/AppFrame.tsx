import type { AppId, CommandDefinition, CommandRegistry, Disposable, Keymap } from '@kiln/commands';
import { ChevronDown, Redo2, Save, Search, Share2, Undo2 } from 'lucide-react';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Backstage, type BackstagePageId } from '../backstage/Backstage.tsx';
import { Icon } from '../icons/Icon.tsx';
import { KeyTipsLayer } from '../keytips/KeyTipsLayer.tsx';
import { CommandPalette } from '../palette/CommandPalette.tsx';
import { cx } from '../primitives/cx.ts';
import { Kbd } from '../primitives/Kbd.tsx';
import { Tooltip, TooltipProvider } from '../primitives/Tooltip.tsx';
import { Ribbon, type RibbonLayoutPreference } from '../ribbon/Ribbon.tsx';
import type { RibbonSpec } from '../ribbon/types.ts';
import { useTheme } from '../theme/ThemeProvider.tsx';
import { APP_NAMES, AppMark } from './AppMark.tsx';
import {
  CommandsProvider,
  useCommandShortcut,
  useCommandState,
  useCommands,
  useExecute,
} from './CommandContext.tsx';
import { OptionsDialog, type OptionsExtraSection } from './OptionsDialog.tsx';
import { useOfficeOptions } from './options.ts';
import { ShortcutSheet } from './ShortcutSheet.tsx';
import { useGlobalShortcuts } from './useGlobalShortcuts.ts';
import './appframe.css';

export interface AppFrameProps {
  app: AppId;
  registry: CommandRegistry;
  keymap?: Keymap;
  /** Document title in the title bar (click to rename when `onRename` is set). */
  title: string;
  onRename?: (title: string) => void;
  /** Short save state after the title: "Saved", "Saving…", "Saved to this device". */
  saveStatus?: string;
  /** The AutoSave switch. Omit to hide it. */
  autoSave?: { enabled: boolean; onChange: (enabled: boolean) => void; disabledReason?: string };
  ribbon?: RibbonSpec;
  defaultTab?: string;
  /** Content of the File menu pages; the File tab appears when given. */
  backstage?: Partial<Record<BackstagePageId, ReactNode>>;
  statusBar?: ReactNode;
  /** Navigation pane (left) and task panes (right): usually `TaskPaneHost`s. */
  leftPane?: ReactNode;
  rightPane?: ReactNode;
  /** Collaborators' avatars. */
  presence?: ReactNode;
  /** Desktop window controls (Electron frameless windows). */
  windowControls?: ReactNode;
  /** App-specific sections for File ▸ Options. */
  optionsSections?: readonly OptionsExtraSection[];
  children?: ReactNode;
  className?: string;
}

const COLLAPSE_KEY = 'kiln.ribbonCollapsed';

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === '1';
  } catch {
    return false;
  }
}

/**
 * The Office window (PLAN.md §1.3): title bar (mark, AutoSave, Save/Undo/Redo, title, Search/Tell-me,
 * presence, Share, window controls), the ribbon with File → Backstage, the document area with
 * optional side panes, and the status bar. Wires the global shortcuts, Key Tips, the command palette,
 * the shortcut cheat sheet and File ▸ Options, all through the command registry.
 */
export function AppFrame(props: AppFrameProps) {
  return (
    <CommandsProvider registry={props.registry} keymap={props.keymap} app={props.app}>
      <TooltipProvider>
        <AppFrameInner {...props} />
      </TooltipProvider>
    </CommandsProvider>
  );
}

function AppFrameInner({
  app,
  registry,
  keymap,
  title,
  onRename,
  saveStatus,
  autoSave,
  ribbon,
  defaultTab,
  backstage,
  statusBar,
  leftPane,
  rightPane,
  presence,
  windowControls,
  optionsSections,
  children,
  className,
}: AppFrameProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [backstageOpen, setBackstageOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [keyTips, setKeyTips] = useState<{ on: boolean; keys?: string }>({ on: false });
  const [collapsed, setCollapsedState] = useState(readCollapsed);
  const [options, updateOptions] = useOfficeOptions();
  const theme = useTheme();
  const appName = APP_NAMES[app];

  const setCollapsed = (c: boolean) => {
    setCollapsedState(c);
    try {
      localStorage.setItem(COLLAPSE_KEY, c ? '1' : '0');
    } catch {
      // ignore
    }
  };
  const ribbonLayout = options.display.ribbonLayout as RibbonLayoutPreference;
  const setRibbonLayout = (l: RibbonLayoutPreference) => updateOptions({ display: { ribbonLayout: l } });

  useEffect(() => {
    registry.setContext({ app });
  }, [registry, app]);

  // Classic Office look: colourful icons (PLAN.md §1.9).
  useEffect(() => {
    const root = document.documentElement;
    if (options.general.classicLook) root.dataset.iconStyle = 'colorful';
    else delete root.dataset.iconStyle;
  }, [options.general.classicLook]);

  // Chrome commands, registered unless the app implements them itself.
  const live = useRef({ collapsed, ribbonLayout, theme, backstage });
  live.current = { collapsed, ribbonLayout, theme, backstage };
  useEffect(() => {
    const defs: CommandDefinition<never>[] = [
      { id: 'view.commandPalette', title: 'Command Palette', run: () => setPaletteOpen((o) => !o) },
      { id: 'view.keyTips', title: 'Key Tips', run: () => setKeyTips((k) => ({ on: !k.on })) },
      { id: 'view.shortcuts', title: 'Keyboard Shortcuts', run: () => setSheetOpen(true) },
      {
        id: 'view.toggleRibbon',
        title: 'Collapse the Ribbon',
        run: () => setCollapsed(!live.current.collapsed),
        isChecked: () => live.current.collapsed,
        when: () => !!ribbon,
      },
      {
        id: 'view.ribbonClassic',
        title: 'Classic Ribbon',
        run: () => setRibbonLayout('classic'),
        isChecked: () => live.current.ribbonLayout === 'classic',
        when: () => !!ribbon,
      },
      {
        id: 'view.ribbonSimplified',
        title: 'Simplified Ribbon',
        run: () => setRibbonLayout('simplified'),
        isChecked: () => live.current.ribbonLayout === 'simplified',
        when: () => !!ribbon,
      },
      {
        id: 'file.backstage',
        title: 'File',
        run: () => setBackstageOpen(true),
        when: () => !!live.current.backstage,
      },
      { id: 'file.options', title: 'Options', run: () => setOptionsOpen(true) },
      {
        id: 'view.themeLight',
        title: 'Light Theme',
        run: () => live.current.theme.setPreference('light'),
        isChecked: () => live.current.theme.preference === 'light',
      },
      {
        id: 'view.themeDark',
        title: 'Dark Theme',
        run: () => live.current.theme.setPreference('dark'),
        isChecked: () => live.current.theme.preference === 'dark',
      },
      {
        id: 'view.themeSystem',
        title: 'Use System Theme',
        run: () => live.current.theme.setPreference('system'),
        isChecked: () => live.current.theme.preference === 'system',
      },
      {
        id: 'view.themeHighContrast',
        title: 'High Contrast Theme',
        run: () => live.current.theme.setPreference('hc'),
        isChecked: () => live.current.theme.preference === 'hc',
      },
    ];
    const disposables: Disposable[] = [];
    registry.batch(() => {
      for (const d of defs) {
        const existing = registry.get(d.id);
        if (existing && !existing.pending) continue; // the app provides its own
        disposables.push(registry.register(d as CommandDefinition<unknown>));
      }
    });
    return () => {
      for (const d of disposables) d.dispose();
    };
  }, [registry, !!ribbon]);

  // Toggle states shown in the ribbon and palette follow the frame's state.
  useEffect(() => {
    registry.invalidate([
      'view.toggleRibbon',
      'view.ribbonClassic',
      'view.ribbonSimplified',
      'view.themeLight',
      'view.themeDark',
      'view.themeSystem',
      'view.themeHighContrast',
    ]);
  }, [registry, collapsed, ribbonLayout, theme.preference]);

  useGlobalShortcuts({
    registry,
    keymap,
    enabled: !paletteOpen && !optionsOpen,
    onAltLetter: ribbon ? (letter) => setKeyTips({ on: true, keys: letter }) : undefined,
  });

  return (
    <div
      ref={rootRef}
      className={cx('kiln-appframe', options.general.classicLook && 'kiln-appframe--classic', className)}
      data-app={app}
    >
      <header className="kiln-titlebar">
        <div className="kiln-titlebar__start">
          <AppMark size={22} label={appName} />
          {autoSave ? <AutoSaveSwitch {...autoSave} /> : null}
          <TitleBarCommand command="file.save" icon={Save} label="Save" keyTip="3" />
          <TitleBarCommand command="edit.undo" icon={Undo2} label="Undo" keyTip="1" />
          <TitleBarCommand command="edit.redo" icon={Redo2} label="Redo" keyTip="2" />
          <DocumentTitle title={title} appName={appName} onRename={onRename} saveStatus={saveStatus} />
        </div>
        <div className="kiln-titlebar__center">
          <SearchBox onOpen={() => setPaletteOpen(true)} />
        </div>
        <div className="kiln-titlebar__end">
          {presence ? <div className="kiln-titlebar__presence">{presence}</div> : null}
          <ShareButton />
          {windowControls ? <div className="kiln-titlebar__window">{windowControls}</div> : null}
        </div>
      </header>
      {ribbon ? (
        <Ribbon
          spec={ribbon}
          defaultTab={defaultTab ?? ribbon.tabs.find((t) => t.id === 'home')?.id}
          layout={ribbonLayout}
          onLayoutChange={setRibbonLayout}
          collapsed={collapsed}
          onCollapsedChange={setCollapsed}
          onFileClick={backstage ? () => setBackstageOpen(true) : undefined}
        />
      ) : null}
      <div className="kiln-appframe__body">
        {leftPane}
        <main className="kiln-appframe__canvas" aria-label={`${appName} document`}>
          {children}
        </main>
        {rightPane}
      </div>
      {statusBar}
      {backstage ? (
        <Backstage
          open={backstageOpen}
          onOpenChange={setBackstageOpen}
          pages={backstage}
          appName={appName}
          documentTitle={title}
          autoSave={autoSave?.enabled}
        />
      ) : null}
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
      <OptionsDialog
        open={optionsOpen}
        onOpenChange={setOptionsOpen}
        appName={appName}
        extraSections={optionsSections}
      />
      <ShortcutSheet open={sheetOpen} onOpenChange={setSheetOpen} />
      <KeyTipsLayer
        active={keyTips.on}
        initialKeys={keyTips.keys}
        rootRef={rootRef}
        onExit={() => setKeyTips({ on: false })}
      />
    </div>
  );
}

function TitleBarCommand({
  command,
  icon,
  label,
  keyTip,
}: {
  command: string;
  icon: typeof Save;
  label: string;
  keyTip: string;
}) {
  const state = useCommandState(command);
  const shortcut = useCommandShortcut(command);
  const execute = useExecute();
  if (!state.visible) return null;
  return (
    <Tooltip content={label} shortcut={shortcut}>
      <button
        type="button"
        className="kiln-titlebar__btn"
        aria-label={label}
        aria-disabled={!state.enabled || undefined}
        data-keytip={keyTip}
        data-keytip-level="root"
        onClick={() => state.enabled && execute(command)}
      >
        <Icon icon={icon} size={16} />
      </button>
    </Tooltip>
  );
}

function AutoSaveSwitch({ enabled, onChange, disabledReason }: NonNullable<AppFrameProps['autoSave']>) {
  return (
    <Tooltip
      content={disabledReason ?? (enabled ? 'AutoSave is on' : 'AutoSave is off')}
      description={disabledReason ? undefined : 'Changes are saved automatically as you work.'}
    >
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        aria-disabled={!!disabledReason || undefined}
        className="kiln-autosave"
        onClick={() => !disabledReason && onChange(!enabled)}
      >
        <span className="kiln-autosave__label">AutoSave</span>
        <span className="kiln-autosave__track" aria-hidden="true">
          <span className="kiln-autosave__thumb" />
        </span>
        <span className="kiln-autosave__state" aria-hidden="true">
          {enabled ? 'On' : 'Off'}
        </span>
      </button>
    </Tooltip>
  );
}

function DocumentTitle({
  title,
  appName,
  onRename,
  saveStatus,
}: {
  title: string;
  appName: string;
  onRename?: (t: string) => void;
  saveStatus?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(title);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editing) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing]);
  const commit = () => {
    const t = draft.trim();
    setEditing(false);
    if (t && t !== title) onRename?.(t);
  };
  if (editing) {
    return (
      <input
        ref={inputRef}
        className="kiln-titlebar__title-input"
        aria-label="Document name"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') commit();
          else if (e.key === 'Escape') {
            e.preventDefault();
            setDraft(title);
            setEditing(false);
          }
        }}
      />
    );
  }
  const content = (
    <>
      <span className="kiln-titlebar__title-text">{title}</span>
      {saveStatus ? <span className="kiln-titlebar__status"> · {saveStatus}</span> : null}
      {onRename ? <Icon icon={ChevronDown} size={12} className="kiln-titlebar__title-chevron" /> : null}
    </>
  );
  return onRename ? (
    <Tooltip content="Rename" description={`${title} — ${appName}`}>
      <button
        type="button"
        className="kiln-titlebar__title"
        onClick={() => {
          setDraft(title);
          setEditing(true);
        }}
      >
        {content}
      </button>
    </Tooltip>
  ) : (
    <span className="kiln-titlebar__title">{content}</span>
  );
}

function SearchBox({ onOpen }: { onOpen: () => void }) {
  const shortcut = useCommandShortcut('view.commandPalette');
  return (
    <button
      type="button"
      className="kiln-searchbox"
      onClick={onOpen}
      data-keytip="Q"
      data-keytip-level="root"
      aria-label="Search commands"
      aria-haspopup="dialog"
    >
      <Icon icon={Search} size={15} />
      <span className="kiln-searchbox__text">Search</span>
      {shortcut ? <Kbd shortcut={shortcut} variant="inline" className="kiln-searchbox__kbd" /> : null}
    </button>
  );
}

function ShareButton() {
  const state = useCommandState('file.share');
  const execute = useExecute();
  const { registry } = useCommands();
  if (!state.visible || !registry.has('file.share') || state.pending) return null;
  return (
    <button
      type="button"
      className="kiln-btn kiln-btn--primary kiln-btn--sm kiln-titlebar__share"
      data-keytip="ZS"
      data-keytip-level="root"
      aria-disabled={!state.enabled || undefined}
      onClick={() => state.enabled && execute('file.share')}
    >
      <span className="kiln-btn__icon">
        <Icon icon={Share2} size={14} />
      </span>
      <span className="kiln-btn__label">Share</span>
    </button>
  );
}
