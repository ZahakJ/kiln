import {
  type AppId,
  type CommandDefinition,
  CommandNotice,
  type CommandRegistry,
  type Disposable,
  type ExecuteResult,
  type Keymap,
} from '@kiln/commands';
import { msg, t } from '@kiln/i18n';
import { useLanguage } from '@kiln/i18n/react';
import { MessageSquare, Redo2, Save, SaveAll, Search, Share2, Undo2 } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import { useRegionCycle } from '../a11y/regions.ts';
import { Backstage, type BackstagePageId } from '../backstage/Backstage.tsx';
import type { MenuEntry } from '../contextmenu/CommandMenuItems.tsx';
import { errorText } from '../errorText.ts';
import { Flame, Spark } from '../icons/brand/BrandMark.tsx';
import { Icon } from '../icons/Icon.tsx';
import { KeyTipsLayer } from '../keytips/KeyTipsLayer.tsx';
import { PhoneAppBar } from '../mobile/AppBar.tsx';
import { BottomToolbar } from '../mobile/BottomToolbar.tsx';
import { ContextToolbar } from '../mobile/ContextToolbar.tsx';
import { useFormFactor } from '../mobile/formFactor.ts';
import { useKeyboardInset } from '../mobile/keyboard.ts';
import { RibbonSheet } from '../mobile/RibbonSheet.tsx';
import { type ShareCopy, ShareSheet } from '../mobile/ShareSheet.tsx';
import { CommandPalette } from '../palette/CommandPalette.tsx';
import { cx } from '../primitives/cx.ts';
import { Kbd } from '../primitives/Kbd.tsx';
import { MenuItem } from '../primitives/Menu.tsx';
import { ariaKeyShortcuts } from '../primitives/shortcut.ts';
import { ToastProvider, useToast } from '../primitives/Toast.tsx';
import { Tooltip, TooltipProvider } from '../primitives/Tooltip.tsx';
import { localizeRibbon } from '../ribbon/localize.ts';
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
  /**
   * The message bar (Office's): notices that ask something of the user, docked full width between
   * the ribbon and the document, which they push down: they never cover the document.
   */
  messageBar?: ReactNode;
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
  /**
   * The Accessible text view (View ▸ Accessibility ▸ Text View): shown over the document, which
   * stays mounted underneath without taking keys or pointer.
   */
  textView?: ReactNode;
  /**
   * Phones: Back in the top app bar (the document closes, asking about unsaved changes,
   * and Kiln Home shows). Without it the app bar has no Back button.
   */
  onBack?: () => void;
  /**
   * The app mark at the start of the title bar becomes a button to Kiln Home (the web build: the
   * document stays open in the tab). Without it the mark is a picture.
   */
  onHome?: () => void;
  /** Phones: the app's entries in the app bar's overflow menu (Write's Print layout). */
  phoneMenu?: readonly MenuEntry[];
  /** Phones: Share ▸ send or download a copy of the document. */
  shareCopy?: ShareCopy;
  children?: ReactNode;
  className?: string;
}

/** The overflow menu's Find and Replace (the command's own title is Replace). */
const FIND_REPLACE = msg('Find and Replace');
/** The overflow menu's name for File ▸ Options. */
const SETTINGS = msg('Settings');

const COLLAPSE_KEY = 'kiln.ribbonCollapsed';

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === '1';
  } catch {
    return false;
  }
}

/**
 * The app window (brand mockups `docs/design/mockups/<app>-*.png`): title bar (app
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
  ribbon: ribbonSpec,
  defaultTab,
  ribbonLayout: ribbonLayoutProp,
  onRibbonLayoutChange,
  ribbonCollapsed,
  onRibbonCollapsedChange,
  backstage,
  backstagePage,
  onBackstagePageChange,
  statusBar,
  messageBar,
  leftPane,
  rightPane,
  presence,
  ribbonTrailing,
  windowControls,
  optionsSections,
  onExecuted,
  textView,
  onBack,
  onHome,
  phoneMenu,
  shareCopy,
  children,
  className,
}: AppFrameProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const language = useLanguage();
  const ribbon = useMemo(
    () => (ribbonSpec ? localizeRibbon(ribbonSpec, language) : undefined),
    [ribbonSpec, language],
  );
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
  // the phone layout (top app bar, bottom toolbar, ribbon sheet) below 640 px.
  const formFactor = useFormFactor();
  const phone = formFactor === 'phone';
  const [ribbonSheetOpen, setRibbonSheetOpen] = useState(false);
  // One active tab for the ribbon and the phone's ribbon sheet: a window that turns from a phone
  // into a tablet (or back) keeps the tab the user was on.
  const [sheetTab, setSheetTab] = useState(
    () =>
      defaultTab ?? ribbonSpec?.tabs.find((x) => x.id === 'home')?.id ?? ribbonSpec?.tabs[0]?.id ?? 'home',
  );
  const [shareOpen, setShareOpen] = useState(false);
  useKeyboardInset(rootRef, formFactor !== 'desktop');
  useEffect(() => {
    if (!phone) {
      setRibbonSheetOpen(false);
      setShareOpen(false);
    }
  }, [phone]);

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

  // Classic Office look: colourful icons; the frame's class gives the title bar.
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
            title: t('{command} is not in this build yet', { command: meta?.title ?? e.id }),
            description: t('The core does not implement this command yet; release builds hide it.'),
            tone: 'info',
            duration: 3500,
          });
        } else if (e.result.status === 'failed' && e.result.error instanceof CommandNotice) {
          toast({
            id: 'kiln-notice',
            title: e.result.error.title,
            description: e.result.error.message,
            tone: 'info',
            duration: 6000,
          });
        } else if (e.result.status === 'failed') {
          toast({
            id: 'kiln-failed',
            title: t('{command} did not work', { command: registry.meta(e.id)?.title ?? e.id }),
            description: errorText(e.result.error),
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
      { id: 'ui.palette.open', title: t('Command Palette'), run: () => setPaletteOpen((o) => !o) },
      { id: 'ui.keytips.show', title: t('Key Tips'), run: () => setKeyTips((k) => ({ on: !k.on })) },
      { id: 'ui.shortcuts.show', title: t('Keyboard Shortcuts'), run: () => setSheetOpen(true) },
      {
        id: 'ui.ribbon.collapse',
        title: t('Collapse the Ribbon'),
        run: () => live.current.setCollapsed(!live.current.collapsed),
        isChecked: () => live.current.collapsed,
        when: () => !!ribbon,
      },
      {
        id: 'ui.ribbon.classic',
        title: t('Classic Ribbon'),
        run: () => live.current.setRibbonLayout('classic'),
        isChecked: () => live.current.ribbonLayout === 'classic',
        when: () => !!ribbon,
      },
      {
        id: 'ui.ribbon.simplified',
        title: t('Simplified Ribbon'),
        run: () => live.current.setRibbonLayout('simplified'),
        isChecked: () => live.current.ribbonLayout === 'simplified',
        when: () => !!ribbon,
      },
      {
        id: 'ui.backstage.open',
        title: t('File'),
        run: () => live.current.openBackstage(),
        when: hasBackstage,
      },
      {
        id: 'ui.backstage.home',
        title: t('Home'),
        run: () => live.current.openBackstage('home'),
        when: hasBackstage,
      },
      {
        id: 'ui.backstage.info',
        title: t('Info'),
        run: () => live.current.openBackstage('info'),
        when: hasBackstage,
      },
      // Ctrl+P and Export as PDF open their Backstage panes, as in Office.
      {
        id: 'suite.file.print',
        title: t('Print'),
        run: () => live.current.openBackstage('print'),
        when: () => !!live.current.backstage?.print,
      },
      {
        id: 'suite.file.export_pdf',
        title: t('Export as PDF'),
        run: () => live.current.openBackstage('export'),
        when: () => !!live.current.backstage?.print,
      },
      {
        id: 'ui.backstage.account',
        title: t('Account'),
        run: () => live.current.openBackstage('account'),
        when: hasBackstage,
      },
      { id: 'ui.options.open', title: t('Options'), run: () => setOptionsOpen(true) },
      {
        id: 'ui.macros.explain',
        title: t('Macros'),
        run: () =>
          toast({
            id: 'kiln-macros',
            title: t('Kiln never runs macros'),
            description: t(
              'Macros in .docm, .xlsm and .pptm files are kept exactly as they are, so the file is not damaged, but they never run. Save as .docm, .xlsm or .pptm to keep them.',
            ),
            tone: 'info',
            duration: 8000,
          }),
      },
      {
        id: 'ui.theme.light',
        title: t('Light Theme'),
        run: () => live.current.theme.setPreference('light'),
        isChecked: () => live.current.theme.preference === 'light',
      },
      {
        id: 'ui.theme.dark',
        title: t('Dark Theme'),
        run: () => live.current.theme.setPreference('dark'),
        isChecked: () => live.current.theme.preference === 'dark',
      },
      {
        id: 'ui.theme.system',
        title: t('Use System Theme'),
        run: () => live.current.theme.setPreference('system'),
        isChecked: () => live.current.theme.preference === 'system',
      },
      {
        id: 'ui.theme.high_contrast',
        title: t('High Contrast Theme'),
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

  // F6 / Shift+F6: document, panes, status bar, title bar, ribbon (Office's order).
  useRegionCycle(rootRef, !paletteOpen && !optionsOpen && !backstageOpen);

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

  const titleNode = (
    <DocumentTitle title={title} extension={extension} appName={appName} onRename={onRename} />
  );
  const hasLiveShare = useCommandState('suite.file.share').visible;
  const overflow: MenuEntry[] = [
    ...(shareCopy || hasLiveShare
      ? [
          {
            kind: 'custom' as const,
            id: 'share',
            render: () => (
              <MenuItem
                icon={<Icon icon={Share2} size={16} />}
                data-testid="overflow-share"
                // The menu closes first, then the sheet takes the keys.
                onSelect={() => requestAnimationFrame(() => setShareOpen(true))}
              >
                {t('Share')}
              </MenuItem>
            ),
          },
        ]
      : []),
    { command: 'suite.file.save' },
    ...(backstage?.saveAs
      ? [
          {
            kind: 'custom' as const,
            id: 'save-as',
            render: () => (
              <MenuItem icon={<Icon icon={SaveAll} size={16} />} onSelect={() => openBackstage('saveAs')}>
                {t('Save As')}
              </MenuItem>
            ),
          },
        ]
      : []),
    { command: 'suite.file.print' },
    { command: 'suite.file.version_history', icon: 'History' },
    { command: 'suite.edit.replace', label: FIND_REPLACE },
    ...(phoneMenu ?? []),
    { kind: 'separator' },
    { command: 'ui.backstage.open' },
    { command: 'ui.options.open', label: SETTINGS },
  ];

  return (
    <div
      ref={rootRef}
      className={cx('kiln-appframe', options.general.classicLook && 'kiln-appframe--classic', className)}
      data-app={app}
      data-form-factor={formFactor}
    >
      {phone ? (
        <PhoneAppBar
          title={titleNode}
          onBack={onBack}
          onSearch={() => setPaletteOpen(true)}
          menu={overflow}
          presence={presence}
        />
      ) : (
        <header className="kiln-titlebar">
          <div className="kiln-titlebar__start">
            {onHome ? (
              <span className="kiln-titlebar__brand">
                <button
                  type="button"
                  className="kiln-titlebar__home"
                  aria-label={t('Kiln Home')}
                  title={t('Kiln Home')}
                  data-testid="titlebar-home"
                  onClick={onHome}
                >
                  <AppMark app={app} size={20} label="" />
                </button>
                <span className="kiln-titlebar__appname" aria-hidden="true">
                  {APP_SHORT_NAMES[app]}
                </span>
              </span>
            ) : (
              <span className="kiln-titlebar__brand">
                <AppMark app={app} size={20} label={appName} />
                <span className="kiln-titlebar__appname" aria-hidden="true">
                  {APP_SHORT_NAMES[app]}
                </span>
              </span>
            )}
            <TitleBarCommand command="suite.file.save" icon={Save} label={t('Save')} keyTip="3" />
            <TitleBarCommand command="suite.edit.undo" icon={Undo2} label={t('Undo')} keyTip="1" />
            <TitleBarCommand command="suite.edit.redo" icon={Redo2} label={t('Redo')} keyTip="2" />
          </div>
          <div className="kiln-titlebar__center">
            {titleNode}
            <SaveChip status={saveStatus} autoSave={autoSave} />
          </div>
          <div className="kiln-titlebar__end">
            <SearchBox onOpen={() => setPaletteOpen(true)} />
            {presence ? <div className="kiln-titlebar__presence">{presence}</div> : null}
            {windowControls ? <div className="kiln-titlebar__window">{windowControls}</div> : null}
          </div>
        </header>
      )}
      {ribbon && !phone ? (
        <Ribbon
          spec={ribbon}
          activeTab={sheetTab}
          onActiveTabChange={setSheetTab}
          layout={ribbonLayout}
          onLayoutChange={setRibbonLayout}
          collapsed={collapsed}
          onCollapsedChange={setCollapsed}
          onFileClick={backstage ? () => openBackstage() : undefined}
          fileButton={
            <>
              <Spark size={13} />
              <span>{t('File')}</span>
            </>
          }
          trailing={shareAndTrailing}
        />
      ) : null}
      {messageBar}
      <div className="kiln-appframe__body">
        {leftPane}
        <main className="kiln-appframe__canvas" aria-label={t('{appName} document', { appName })}>
          <div className="kiln-appframe__doc" inert={textView ? true : undefined}>
            {children}
          </div>
          {textView}
        </main>
        {rightPane}
      </div>
      {statusBar}
      {phone && ribbon ? (
        <BottomToolbar
          app={app}
          spec={ribbon}
          tab={sheetTab}
          sheetOpen={ribbonSheetOpen}
          onOpenSheet={() => setRibbonSheetOpen(true)}
        />
      ) : null}
      {phone && ribbon ? (
        <RibbonSheet
          spec={ribbon}
          open={ribbonSheetOpen}
          onOpenChange={setRibbonSheetOpen}
          tab={sheetTab}
          onTabChange={setSheetTab}
        />
      ) : null}
      {phone ? <ShareSheet open={shareOpen} onOpenChange={setShareOpen} copy={shareCopy} /> : null}
      {/* over a selection a finger made (nothing shows until a touch selects). */}
      <ContextToolbar
        onFormat={
          phone && ribbon
            ? () => {
                setSheetTab(ribbon.tabs.find((x) => x.id === 'home')?.id ?? sheetTab);
                setRibbonSheetOpen(true);
              }
            : undefined
        }
      />
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
      content={disabledReason ?? (enabled ? t('AutoSave is on') : t('AutoSave is off'))}
      description={disabledReason ? undefined : t('Changes are saved automatically as you work.')}
    >
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        aria-label={t('AutoSave')}
        aria-disabled={!!disabledReason || undefined}
        className="kiln-chip kiln-titlebar__chip kiln-autosave"
        data-on={enabled || undefined}
        data-testid="save-status"
        onClick={() => !disabledReason && onChange(!enabled)}
      >
        <Flame size={13} className="kiln-autosave__flame" />
        <span>{status ?? (enabled ? t('AutoSave on') : t('AutoSave off'))}</span>
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
        aria-label={t('Document name')}
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
  // Name and extension read as one file name ("Report.docx"), in its own direction: an English
  // name in the Arabic interface still reads "Report.docx", an Arabic one right to left.
  const content = (
    <span className="kiln-titlebar__filename" dir="auto">
      <span className="kiln-titlebar__title-text">{title}</span>
      {extension ? <span className="kiln-titlebar__ext">{extension}</span> : null}
    </span>
  );
  return onRename ? (
    <Tooltip content={t('Rename')} description={`${title} — ${appName}`}>
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
        <span aria-hidden="true">{t('Search')}</span>
        <span className="kiln-visually-hidden">{t('Search commands')}</span>
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
      aria-label={t('Share')}
      aria-disabled={!state.enabled || undefined}
      onClick={() => state.enabled && execute('suite.file.share')}
    >
      <Icon icon={Share2} size={15} />
      <span>{t('Share')}</span>
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
  // The name holds everything the button shows, its count too.
  const name = typeof badge === 'string' || typeof badge === 'number' ? `${label} ${badge}` : label;
  return (
    <button
      type="button"
      className="kiln-appframe__strip-btn"
      data-command={command}
      aria-label={name}
      aria-disabled={!state.enabled || undefined}
      onClick={() => state.enabled && execute(command)}
    >
      <Icon icon={icon} size={15} />
      <span>{label}</span>
      {badge !== undefined ? (
        <>
          {/* Between flex items the space is not drawn, but it keeps the name and the text apart. */}{' '}
          <span className="kiln-appframe__badge">{badge}</span>
        </>
      ) : null}
    </button>
  );
}
