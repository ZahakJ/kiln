import type {
  AppId,
  CommandDefinition,
  CommandRegistry,
  Disposable,
  ExecuteResult,
  Keymap,
} from '@kiln/commands';
import { MessageSquare, Redo2, Save, Search, Share2, Undo2 } from 'lucide-react';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Backstage, type BackstagePageId } from '../backstage/Backstage.tsx';
import { Flame, Spark } from '../icons/brand/BrandMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { KeyTipsLayer } from '../keytips/KeyTipsLayer.tsx';
import { CommandPalette } from '../palette/CommandPalette.tsx';
import { cx } from '../primitives/cx.ts';
import { Kbd } from '../primitives/Kbd.tsx';
import { ariaKeyShortcuts } from '../primitives/shortcut.ts';
import { ToastProvider, useToast } from '../primitives/Toast.tsx';
import { Tooltip, TooltipProvider } from '../primitives/Tooltip.tsx';
import { Ribbon, type RibbonLayoutPreference } from '../ribbon/Ribbon.tsx';
import type { RibbonSpec } from '../ribbon/types.ts';
import { useTheme } from '../theme/ThemeProvider.tsx';
import { APP_NAMES, APP_SHORT_NAMES, AppMark } from './AppMark.tsx';
import { CommandsProvider, useCommandShortcut, useCommandState, useExecute } from './CommandContext.tsx';
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
  /** File extension shown after the title, muted (`.docx`). */
  extension?: string;
  onRename?: (title: string) => void;
  /** Save state in the title chip: "Saved", "Saving…", "Not saved yet". */
  saveStatus?: string;
  /** The AutoSave switch (the title chip). Omit to show the save state only. */
  autoSave?: { enabled: boolean; onChange: (enabled: boolean) => void; disabledReason?: string };
  ribbon?: RibbonSpec;
  defaultTab?: string;
  /** Classic / Simplified / automatic ribbon (controlled; default: File ▸ Options ▸ Display). */
  ribbonLayout?: RibbonLayoutPreference;
  onRibbonLayoutChange?: (layout: RibbonLayoutPreference) => void;
  /** Tabs only (Ctrl+F1), controlled; default: remembered in this browser. */
  ribbonCollapsed?: boolean;
  onRibbonCollapsedChange?: (collapsed: boolean) => void;
  /** Content of the File menu pages; the File button appears when given. */
  backstage?: Partial<Record<BackstagePageId, ReactNode>>;
  /** Opens the File menu on this page (and reports page changes). */
  backstagePage?: BackstagePageId;
  onBackstagePageChange?: (page: BackstagePageId) => void;
  statusBar?: ReactNode;
  /** Navigation pane (left) and task panes (right): usually `TaskPaneHost`s. */
  leftPane?: ReactNode;
  rightPane?: ReactNode;
  /** The signed-in user's avatar and collaborators. */
  presence?: ReactNode;
  /** Commands at the right of the tab strip, before Share (Comments, Editing mode, Present). */
  ribbonTrailing?: ReactNode;
  /** Desktop window controls (frameless windows). */
  windowControls?: ReactNode;
  /** App-specific sections for File ▸ Options. */
  optionsSections?: readonly OptionsExtraSection[];
  /** Called after every command run from the chrome or a shortcut. */
  onExecuted?: (result: ExecuteResult) => void;
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
 * The app window (PLAN.md §1.3, brand mockups `docs/design/mockups/<app>-*.png`): title bar (app
 * tile and name, Save/Undo/Redo, the document title with its save chip, Search/Tell me, presence,
 * window controls), the tab strip with the File button and Share, the ribbon slab, the document
 * area with optional side panes, and the status bar. Wires the global shortcuts, Key Tips, the
 * command palette, the shortcut cheat sheet and File ▸ Options, all through the command registry.
 */
export function AppFrame(props: AppFrameProps) {
  return (
    <CommandsProvider registry={props.registry} keymap={props.keymap} app={props.app}>
      <TooltipProvider>
        <ToastProvider>
          <AppFrameInner {...props} />
        </ToastProvider>
      </TooltipProvider>
    </CommandsProvider>
  );
}

function AppFrameInner({
  app,
  registry,
  keymap,
  title,
  extension,
  onRename,
  saveStatus,
  autoSave,
  ribbon,
  defaultTab,
  ribbonLayout: ribbonLayoutProp,
  onRibbonLayoutChange,
  ribbonCollapsed,
  onRibbonCollapsedChange,
  backstage,
  backstagePage,
  onBackstagePageChange,
  statusBar,
  leftPane,
  rightPane,
  presence,
  ribbonTrailing,
  windowControls,
  optionsSections,
  onExecuted,
  children,
  className,
}: AppFrameProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [backstageOpen, setBackstageOpen] = useState(false);
  const [page, setPage] = useState<BackstagePageId | undefined>(backstagePage);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [keyTips, setKeyTips] = useState<{ on: boolean; keys?: string }>({ on: false });
  const [collapsedState, setCollapsedState] = useState(readCollapsed);
  const [options, updateOptions] = useOfficeOptions();
  const theme = useTheme();
  const { toast } = useToast();
  const appName = APP_NAMES[app];

  const collapsed = ribbonCollapsed ?? collapsedState;
  const setCollapsed = (c: boolean) => {
    if (onRibbonCollapsedChange) onRibbonCollapsedChange(c);
    if (ribbonCollapsed !== undefined) return;
    setCollapsedState(c);
    try {
      localStorage.setItem(COLLAPSE_KEY, c ? '1' : '0');
    } catch {
      // Private mode: the choice lasts for this window only.
    }
  };
  const ribbonLayout = ribbonLayoutProp ?? (options.display.ribbonLayout as RibbonLayoutPreference);
  const setRibbonLayout = (l: RibbonLayoutPreference) => {
    if (onRibbonLayoutChange) onRibbonLayoutChange(l);
    if (ribbonLayoutProp === undefined) updateOptions({ display: { ribbonLayout: l } });
  };
  const openBackstage = (p?: BackstagePageId) => {
    if (p) {
      setPage(p);
      onBackstagePageChange?.(p);
    }
    setBackstageOpen(true);
  };

  useEffect(() => {
    registry.setContext({ app });
  }, [registry, app]);

  // Classic Office look: colourful icons (PLAN.md §1.9).
  useEffect(() => {
    const root = document.documentElement;
    if (options.general.classicLook) root.dataset.iconStyle = 'colorful';
    else delete root.dataset.iconStyle;
  }, [options.general.classicLook]);

  // A planned command (development builds only show them) says so instead of doing nothing.
  const executed = useRef(onExecuted);
  executed.current = onExecuted;
  useEffect(
    () =>
      registry.subscribe((e) => {
        if (e.type !== 'executed') return;
        executed.current?.(e.result);
        if (e.result.status === 'pending') {
          const meta = registry.meta(e.id);
          toast({
            id: 'kiln-planned',
            title: `${meta?.title ?? e.id} is not in this build yet`,
            description: 'The core does not implement this command yet; release builds hide it.',
            tone: 'info',
            duration: 3500,
          });
        } else if (e.result.status === 'failed') {
          const err = e.result.error as { message?: string } | undefined;
          toast({
            id: 'kiln-failed',
            title: `${registry.meta(e.id)?.title ?? e.id} did not work`,
            description: err?.message ?? String(e.result.error),
            tone: 'danger',
            duration: 6000,
          });
        }
      }),
    [registry, toast],
  );

  // Chrome commands, registered unless the app implements them itself.
  const live = useRef({
    collapsed,
    ribbonLayout,
    theme,
    backstage,
    openBackstage,
    setCollapsed,
    setRibbonLayout,
  });
  live.current = { collapsed, ribbonLayout, theme, backstage, openBackstage, setCollapsed, setRibbonLayout };
  useEffect(() => {
    const hasBackstage = () => !!live.current.backstage;
    const defs: CommandDefinition<never>[] = [
      { id: 'ui.palette.open', title: 'Command Palette', run: () => setPaletteOpen((o) => !o) },
      { id: 'ui.keytips.show', title: 'Key Tips', run: () => setKeyTips((k) => ({ on: !k.on })) },
      { id: 'ui.shortcuts.show', title: 'Keyboard Shortcuts', run: () => setSheetOpen(true) },
      {
        id: 'ui.ribbon.collapse',
        title: 'Collapse the Ribbon',
        run: () => live.current.setCollapsed(!live.current.collapsed),
        isChecked: () => live.current.collapsed,
        when: () => !!ribbon,
      },
      {
        id: 'ui.ribbon.classic',
        title: 'Classic Ribbon',
        run: () => live.current.setRibbonLayout('classic'),
        isChecked: () => live.current.ribbonLayout === 'classic',
        when: () => !!ribbon,
      },
      {
        id: 'ui.ribbon.simplified',
        title: 'Simplified Ribbon',
        run: () => live.current.setRibbonLayout('simplified'),
        isChecked: () => live.current.ribbonLayout === 'simplified',
        when: () => !!ribbon,
      },
      { id: 'ui.backstage.open', title: 'File', run: () => live.current.openBackstage(), when: hasBackstage },
      {
        id: 'ui.backstage.home',
        title: 'Home',
        run: () => live.current.openBackstage('home'),
        when: hasBackstage,
      },
      {
        id: 'ui.backstage.info',
        title: 'Info',
        run: () => live.current.openBackstage('info'),
        when: hasBackstage,
      },
      {
        id: 'ui.backstage.account',
        title: 'Account',
        run: () => live.current.openBackstage('account'),
        when: hasBackstage,
      },
      { id: 'ui.options.open', title: 'Options', run: () => setOptionsOpen(true) },
      {
        id: 'ui.macros.explain',
        title: 'Macros',
        run: () =>
          toast({
            id: 'kiln-macros',
            title: 'Kiln never runs macros',
            description:
              'Macros in .docm, .xlsm and .pptm files are kept exactly as they are, so the file is not damaged, but they do not run (PLAN.md §9).',
            tone: 'info',
            duration: 8000,
          }),
      },
      {
        id: 'ui.theme.light',
        title: 'Light Theme',
        run: () => live.current.theme.setPreference('light'),
        isChecked: () => live.current.theme.preference === 'light',
      },
      {
        id: 'ui.theme.dark',
        title: 'Dark Theme',
        run: () => live.current.theme.setPreference('dark'),
        isChecked: () => live.current.theme.preference === 'dark',
      },
      {
        id: 'ui.theme.system',
        title: 'Use System Theme',
        run: () => live.current.theme.setPreference('system'),
        isChecked: () => live.current.theme.preference === 'system',
      },
      {
        id: 'ui.theme.high_contrast',
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
  }, [registry, !!ribbon, toast]);

  // Toggle states shown in the ribbon and palette follow the frame's state.
  useEffect(() => {
    registry.invalidate([
      'ui.ribbon.collapse',
      'ui.ribbon.classic',
      'ui.ribbon.simplified',
      'ui.theme.light',
      'ui.theme.dark',
      'ui.theme.system',
      'ui.theme.high_contrast',
    ]);
  }, [registry, collapsed, ribbonLayout, theme.preference]);

  useGlobalShortcuts({
    registry,
    keymap,
    enabled: !paletteOpen && !optionsOpen && !backstageOpen,
    onAltLetter: ribbon ? (letter) => setKeyTips({ on: true, keys: letter }) : undefined,
  });

  const shareAndTrailing = (
    <>
      {ribbonTrailing}
      <ShareButton />
    </>
  );

  return (
    <div
      ref={rootRef}
      className={cx('kiln-appframe', options.general.classicLook && 'kiln-appframe--classic', className)}
      data-app={app}
    >
      <header className="kiln-titlebar">
        <div className="kiln-titlebar__start">
          <span className="kiln-titlebar__brand">
            <AppMark app={app} size={20} label={appName} />
            <span className="kiln-titlebar__appname" aria-hidden="true">
              {APP_SHORT_NAMES[app]}
            </span>
          </span>
          <TitleBarCommand command="suite.file.save" icon={Save} label="Save" keyTip="3" />
          <TitleBarCommand command="suite.edit.undo" icon={Undo2} label="Undo" keyTip="1" />
          <TitleBarCommand command="suite.edit.redo" icon={Redo2} label="Redo" keyTip="2" />
        </div>
        <div className="kiln-titlebar__center">
          <DocumentTitle title={title} extension={extension} appName={appName} onRename={onRename} />
          <SaveChip status={saveStatus} autoSave={autoSave} />
        </div>
        <div className="kiln-titlebar__end">
          <SearchBox onOpen={() => setPaletteOpen(true)} />
          {presence ? <div className="kiln-titlebar__presence">{presence}</div> : null}
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
          onFileClick={backstage ? () => openBackstage() : undefined}
          fileButton={
            <>
              <Spark size={13} />
              <span>File</span>
            </>
          }
          trailing={shareAndTrailing}
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
          page={page}
          onPageChange={(p) => {
            setPage(p);
            onBackstagePageChange?.(p);
          }}
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
        aria-keyshortcuts={shortcut ? ariaShortcut(shortcut) : undefined}
        aria-disabled={!state.enabled || undefined}
        data-keytip={keyTip}
        data-keytip-level="root"
        data-command={command}
        onClick={() => state.enabled && execute(command)}
      >
        <Icon icon={icon} size={17} />
      </button>
    </Tooltip>
  );
}

/** `Mod+Shift+S` → `Control+Shift+S` (the aria-keyshortcuts form). */
export function ariaShortcut(shortcut: string): string {
  return ariaKeyShortcuts(shortcut);
}

/**
 * The chip after the title (mockup: flame + "Saved to Studio"). With `autoSave` it is the AutoSave
 * switch: the flame glows while AutoSave is on.
 */
function SaveChip({ status, autoSave }: { status?: string; autoSave?: AppFrameProps['autoSave'] }) {
  if (!autoSave) {
    return status ? (
      <span className="kiln-chip kiln-titlebar__chip" data-testid="save-status">
        <Flame size={13} />
        <span>{status}</span>
      </span>
    ) : null;
  }
  const { enabled, onChange, disabledReason } = autoSave;
  return (
    <Tooltip
      content={disabledReason ?? (enabled ? 'AutoSave is on' : 'AutoSave is off')}
      description={disabledReason ? undefined : 'Changes are saved automatically as you work.'}
    >
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        aria-label="AutoSave"
        aria-disabled={!!disabledReason || undefined}
        className="kiln-chip kiln-titlebar__chip kiln-autosave"
        data-on={enabled || undefined}
        data-testid="save-status"
        onClick={() => !disabledReason && onChange(!enabled)}
      >
        <Flame size={13} className="kiln-autosave__flame" />
        <span>{status ?? (enabled ? 'AutoSave on' : 'AutoSave off')}</span>
      </button>
    </Tooltip>
  );
}

function DocumentTitle({
  title,
  extension,
  appName,
  onRename,
}: {
  title: string;
  extension?: string;
  appName: string;
  onRename?: (t: string) => void;
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
            e.stopPropagation();
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
      {extension ? <span className="kiln-titlebar__ext">{extension}</span> : null}
    </>
  );
  return onRename ? (
    <Tooltip content="Rename" description={`${title} — ${appName}`}>
      <button
        type="button"
        className="kiln-titlebar__title"
        data-testid="document-title"
        onClick={() => {
          setDraft(title);
          setEditing(true);
        }}
      >
        {content}
      </button>
    </Tooltip>
  ) : (
    <span className="kiln-titlebar__title" data-testid="document-title">
      {content}
    </span>
  );
}

function SearchBox({ onOpen }: { onOpen: () => void }) {
  const shortcut = useCommandShortcut('ui.palette.open');
  return (
    <button
      type="button"
      className="kiln-searchbox"
      onClick={onOpen}
      data-keytip="Q"
      data-keytip-level="root"
      aria-keyshortcuts={shortcut ? ariaShortcut(shortcut) : undefined}
      aria-haspopup="dialog"
    >
      <Icon icon={Search} size={15} />
      <span className="kiln-searchbox__text">
        Search<span className="kiln-visually-hidden"> commands</span>
      </span>
      {shortcut ? <Kbd shortcut={shortcut} variant="inline" className="kiln-searchbox__kbd" /> : null}
    </button>
  );
}

function ShareButton() {
  const state = useCommandState('suite.file.share');
  const execute = useExecute();
  if (!state.visible) return null;
  return (
    <button
      type="button"
      className="kiln-appframe__share"
      data-keytip="ZS"
      data-keytip-level="root"
      data-command="suite.file.share"
      aria-disabled={!state.enabled || undefined}
      onClick={() => state.enabled && execute('suite.file.share')}
    >
      <Icon icon={Share2} size={15} />
      <span>Share</span>
    </button>
  );
}

/** A tab-strip command button in the mockups' outline style (Comments, Record, Follow). */
export function StripButton({
  command,
  label,
  icon = MessageSquare,
  badge,
}: {
  command: string;
  label: string;
  icon?: typeof Save;
  badge?: ReactNode;
}) {
  const state = useCommandState(command);
  const execute = useExecute();
  if (!state.visible) return null;
  return (
    <button
      type="button"
      className="kiln-appframe__strip-btn"
      data-command={command}
      aria-disabled={!state.enabled || undefined}
      onClick={() => state.enabled && execute(command)}
    >
      <Icon icon={icon} size={15} />
      <span>{label}</span>
      {badge !== undefined ? <span className="kiln-appframe__badge">{badge}</span> : null}
    </button>
  );
}
