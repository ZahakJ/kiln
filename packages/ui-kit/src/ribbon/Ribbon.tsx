import { assignKeyTips, type CommandRegistry, normalizeKeyTip, TAB_KEYTIPS } from '@kiln/commands';
import { t } from '@kiln/i18n';
import { Check, ChevronDown, ChevronsRight, ChevronUp, Ellipsis } from 'lucide-react';
import {
  Fragment,
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { useCommands } from '../chrome/CommandContext.tsx';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import { Menu, MenuContent, MenuItem, MenuLabel, MenuSeparator, MenuTrigger } from '../primitives/Menu.tsx';
import { Popover, PopoverContent, PopoverTrigger } from '../primitives/Popover.tsx';
import { Toolbar } from '../primitives/Toolbar.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';
import { itemId, leafItems } from './builders.ts';
import { DialogLauncher } from './DialogLauncher.tsx';
import { RibbonIcon, RibbonItemView } from './RibbonItem.tsx';
import type {
  RibbonGroup,
  RibbonItem,
  RibbonItemSize,
  RibbonLayout,
  RibbonLeafItem,
  RibbonSpec,
  RibbonTab,
} from './types.ts';
import { visibleTab } from './visibility.ts';
import './ribbon.css';

const SHOW_TAB_EVENT = 'kiln:ribbon-show-tab';

/**
 * Makes a ribbon tab the active one, as Office does with Chart Design after a chart is inserted. A
 * contextual tab that is not showing yet is chosen once it shows (within a few seconds).
 */
export function showRibbonTab(tab: string): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent(SHOW_TAB_EVENT, { detail: { tab } }));
}

/** Width at which the automatic layout switches from Simplified to Classic. */
export const CLASSIC_MIN_WIDTH = 1100;

export type RibbonLayoutPreference = RibbonLayout | 'auto';

export interface RibbonProps {
  spec: RibbonSpec;
  /** Controlled active tab id. */
  activeTab?: string;
  defaultTab?: string;
  onActiveTabChange?: (id: string) => void;
  /** `auto` (default): Classic at ≥ 1100 px, Simplified below. */
  layout?: RibbonLayoutPreference;
  onLayoutChange?: (layout: RibbonLayoutPreference) => void;
  /** Tabs only (Ctrl+F1). A clicked tab then shows its commands in a floating panel. */
  collapsed?: boolean;
  onCollapsedChange?: (collapsed: boolean) => void;
  /** The File tab opens the Backstage. Omit to hide the File tab. */
  onFileClick?: () => void;
  /** Content of the File button (default: the word "File"). */
  fileButton?: ReactNode;
  /** Content at the right end of the tab strip (Comments, Editing mode…). */
  trailing?: ReactNode;
  className?: string;
}

function useElementWidth(ref: React.RefObject<HTMLElement | null>, fallback: number): number {
  const [width, setWidth] = useState(fallback);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const w = el.getBoundingClientRect().width;
      if (w > 0) setWidth(w);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return width;
}

/**
 * The Office ribbon, data-driven from a `RibbonSpec` and bound to the command registry: Classic
 * (3-row) or Simplified (1-row) layout, collapse to tabs only, contextual tab sets, groups that
 * collapse into drop-downs (Classic) or overflow into "More" (Simplified) when space runs out,
 * dialog launchers, Office super tooltips and Key Tips attributes. Keyboard: the tab strip is a
 * WAI-ARIA tablist (←/→, Home/End, ↓/Enter into the commands); the commands are one toolbar
 * (←/→ between all commands, Esc back to the tab).
 */
export function Ribbon({
  spec,
  activeTab: activeTabProp,
  defaultTab,
  onActiveTabChange,
  layout: layoutPref = 'auto',
  onLayoutChange,
  collapsed = false,
  onCollapsedChange,
  onFileClick,
  fileButton,
  trailing,
  className,
}: RibbonProps) {
  const { registry, app } = useCommands();
  const baseId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());
  const panelRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(rootRef, typeof window === 'undefined' ? 1280 : window.innerWidth);
  const layout: RibbonLayout =
    layoutPref === 'auto' ? (width >= CLASSIC_MIN_WIDTH ? 'classic' : 'simplified') : layoutPref;

  const tabs = useVisibleTabs(spec, registry);
  const [activeState, setActiveState] = useState(defaultTab ?? tabs[0]?.id ?? '');
  const requested = activeTabProp ?? activeState;
  const active = tabs.find((t) => t.id === requested) ?? tabs[0];
  const setActive = (id: string) => {
    if (activeTabProp === undefined) setActiveState(id);
    onActiveTabChange?.(id);
    // Editors follow the tab too (Deck shows its effect numbers on the Animations tab).
    if (typeof window !== 'undefined')
      window.dispatchEvent(new CustomEvent('kiln:ribbon-tab', { detail: { tab: id } }));
  };

  // A tab asked for by `showRibbonTab` (Chart Design once a chart is inserted): it is chosen as
  // soon as it shows (a contextual tab shows once its object is selected).
  const wanted = useRef<{ id: string; at: number } | null>(null);
  const [wantedRev, setWantedRev] = useState(0);
  useEffect(() => {
    const on = (e: Event) => {
      const id = (e as CustomEvent<{ tab: string }>).detail?.tab;
      if (!id) return;
      wanted.current = { id, at: performance.now() };
      setWantedRev((n) => n + 1);
    };
    window.addEventListener(SHOW_TAB_EVENT, on);
    return () => window.removeEventListener(SHOW_TAB_EVENT, on);
  }, []);
  const tabIds = tabs.map((t) => t.id).join('|');
  useEffect(() => {
    const w = wanted.current;
    if (!w) return;
    if (performance.now() - w.at > 3000) {
      wanted.current = null;
      return;
    }
    if (tabIds.split('|').includes(w.id)) {
      wanted.current = null;
      if (active?.id !== w.id) setActive(w.id);
    }
  }, [tabIds, wantedRev]);

  const [floating, setFloating] = useState(false);
  useEffect(() => {
    if (!collapsed) setFloating(false);
  }, [collapsed]);
  // A command run from the floating panel closes it (Office behaviour).
  useEffect(() => {
    if (!floating) return;
    return registry.subscribe((e) => {
      if (e.type === 'executed') setFloating(false);
    });
  }, [floating, registry]);
  useEffect(() => {
    if (!floating) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (rootRef.current?.contains(t) || t?.closest?.('[data-radix-popper-content-wrapper]')) return;
      setFloating(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
  }, [floating]);

  const regular = tabs.filter((t) => !t.contextual);
  const contextualSets = useMemo(() => {
    const sets = new Map<string, { label: string; tabs: RibbonTab[] }>();
    for (const t of tabs) {
      if (!t.contextual) continue;
      const s = sets.get(t.contextual.set) ?? { label: t.contextual.label, tabs: [] };
      s.tabs.push(t);
      sets.set(t.contextual.set, s);
    }
    return [...sets.entries()];
  }, [tabs]);
  const orderedTabs = [...regular, ...contextualSets.flatMap(([, s]) => s.tabs)];

  // Tabs that do not fit (phones) go into a More tabs menu at the end of the row, never cut off
  // mid-label; the active tab always shows.
  const listRef = useRef<HTMLDivElement>(null);
  const moreRef = useRef<HTMLButtonElement>(null);
  const tabWidths = useRef(new Map<string, number>());
  const [overflow, setOverflow] = useState<readonly string[]>([]);
  const tabKey = orderedTabs.map((t) => t.id).join('|');
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || typeof ResizeObserver === 'undefined') return;
    const ids = tabKey ? tabKey.split('|') : [];
    const measure = () => {
      for (const [id, el] of tabRefs.current) {
        if (el.offsetWidth > 0) tabWidths.current.set(id, el.offsetWidth);
      }
      // The row's width without the More tabs button (it takes 36 px when shown).
      const more = moreRef.current ? moreRef.current.offsetWidth + 4 : 0;
      const avail = list.clientWidth + more;
      if (avail <= 0) return;
      const gap = 2;
      // Contextual sets add their padding; the label above them takes no width of its own.
      const extra = contextualSets.length * 12;
      const w = (id: string) => (tabWidths.current.get(id) ?? 72) + gap;
      const total = ids.reduce((sum, id) => sum + w(id), extra);
      const next: string[] = [];
      if (total > avail) {
        const room = avail - 40 - extra;
        const activeId = active?.id ?? '';
        let used = activeId ? w(activeId) : 0;
        let full = false;
        for (const id of ids) {
          if (id === activeId) continue;
          if (!full && used + w(id) <= room) used += w(id);
          else {
            full = true;
            next.push(id);
          }
        }
      }
      setOverflow((prev) =>
        prev.length === next.length && prev.every((id, i) => id === next[i]) ? prev : next,
      );
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(list);
    return () => ro.disconnect();
  }, [tabKey, active?.id, contextualSets.length]);
  const hiddenTabs = new Set(overflow);

  // Key Tips: tabs (Office letters), then each tab's commands.
  const tabTips = useMemo(() => {
    const preferred = TAB_KEYTIPS[app] as Record<string, string>;
    return assignKeyTips(
      orderedTabs.map((t) => ({ id: t.id, label: t.label, preferred: t.keyTip ?? preferred[t.id] })),
      onFileClick ? ['F', 'Q'] : ['Q'],
    );
  }, [orderedTabs, app, onFileClick]);

  const panelId = `${baseId}-panel`;
  const tabDomId = (id: string) => `${baseId}-tab-${id}`;

  // A tab in the More tabs menu shows once it is active: focus it after that render.
  const focusTab = (id: string) => {
    const el = tabRefs.current.get(id);
    if (el?.classList.contains('is-overflow')) requestAnimationFrame(() => tabRefs.current.get(id)?.focus());
    else el?.focus();
  };
  const focusFirstCommand = () => {
    requestAnimationFrame(() => {
      const toolbar = panelRef.current?.querySelector<HTMLElement>('[role="toolbar"]');
      const first =
        toolbar?.querySelector<HTMLElement>('[tabindex="0"]') ??
        toolbar?.querySelector<HTMLElement>('button:not(:disabled), input:not(:disabled)');
      first?.focus();
    });
  };

  const onTabKeyDown = (e: KeyboardEvent<HTMLButtonElement>, id: string) => {
    const i = orderedTabs.findIndex((t) => t.id === id);
    const rtl = getComputedStyle(e.currentTarget).direction === 'rtl';
    let next: number | null = null;
    if (e.key === (rtl ? 'ArrowLeft' : 'ArrowRight')) next = (i + 1) % orderedTabs.length;
    else if (e.key === (rtl ? 'ArrowRight' : 'ArrowLeft'))
      next = (i - 1 + orderedTabs.length) % orderedTabs.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = orderedTabs.length - 1;
    else if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (collapsed) setFloating(true);
      focusFirstCommand();
      return;
    }
    if (next === null) return;
    e.preventDefault();
    const t = orderedTabs[next];
    if (t) {
      setActive(t.id);
      focusTab(t.id);
    }
  };

  const onTabClick = (id: string) => {
    if (collapsed) setFloating(!(floating && active?.id === id));
    setActive(id);
  };

  const renderTab = (t: RibbonTab) => {
    const selected = t.id === active?.id;
    return (
      <button
        key={t.id}
        ref={(el) => {
          if (el) tabRefs.current.set(t.id, el);
          else tabRefs.current.delete(t.id);
        }}
        type="button"
        role="tab"
        id={tabDomId(t.id)}
        aria-selected={selected}
        aria-controls={selected && (!collapsed || floating) ? panelId : undefined}
        tabIndex={selected ? 0 : -1}
        className={cx(
          'kiln-ribbon__tab',
          selected && 'is-active',
          t.contextual && 'kiln-ribbon__tab--contextual',
          hiddenTabs.has(t.id) && 'is-overflow',
        )}
        data-keytip={tabTips.get(t.id)}
        data-keytip-level="root"
        data-keytip-enter={`tab:${t.id}`}
        // A click on a tab leaves the keys where they were (in the document), as in Office;
        // the keyboard (F6, Key Tips, arrows) still moves focus to the tabs.
        onMouseDown={(e) => {
          if (e.button === 0) e.preventDefault();
        }}
        onClick={() => onTabClick(t.id)}
        onDoubleClick={() => onCollapsedChange?.(!collapsed)}
        onKeyDown={(e) => onTabKeyDown(e, t.id)}
      >
        <span className="kiln-ribbon__tab-label">{t.label}</span>
      </button>
    );
  };

  const showPanel = active && (!collapsed || floating);

  return (
    <div
      ref={rootRef}
      className={cx(
        'kiln-ribbon',
        `kiln-ribbon--${layout}`,
        collapsed && 'kiln-ribbon--collapsed',
        className,
      )}
      data-layout={layout}
    >
      <div className="kiln-ribbon__strip">
        {onFileClick ? (
          <button
            type="button"
            className="kiln-ribbon__file"
            onClick={onFileClick}
            data-keytip="F"
            data-keytip-level="root"
            aria-haspopup="dialog"
            aria-label={t('File')}
          >
            {fileButton ?? t('File')}
          </button>
        ) : null}
        <div ref={listRef} className="kiln-ribbon__tablist" role="tablist" aria-label={t('Ribbon tabs')}>
          {regular.map(renderTab)}
          {contextualSets.map(([set, s]) => (
            <div
              key={set}
              className={cx(
                'kiln-ribbon__ctxset',
                s.tabs.every((t) => hiddenTabs.has(t.id)) && 'is-overflow',
              )}
              role="presentation"
              data-set={set}
              title={s.label}
            >
              <span className="kiln-ribbon__ctxlabel" aria-hidden="true">
                {s.label}
              </span>
              <div className="kiln-ribbon__ctxtabs" role="presentation">
                {s.tabs.map(renderTab)}
              </div>
            </div>
          ))}
        </div>
        {overflow.length > 0 ? (
          <Menu modal={false}>
            <MenuTrigger asChild>
              <button
                ref={moreRef}
                type="button"
                className="kiln-ribbon__more-tabs"
                aria-label={t('More tabs')}
                onMouseDown={(e) => {
                  if (e.button === 0) e.preventDefault();
                }}
              >
                <Icon icon={ChevronsRight} size={16} />
              </button>
            </MenuTrigger>
            <MenuContent align="end">
              {orderedTabs
                .filter((t) => hiddenTabs.has(t.id))
                .map((t) => (
                  <MenuItem key={t.id} onSelect={() => onTabClick(t.id)}>
                    {t.contextual ? `${t.contextual.label}: ${t.label}` : t.label}
                  </MenuItem>
                ))}
            </MenuContent>
          </Menu>
        ) : null}
        <div className="kiln-ribbon__strip-end">
          {trailing}
          <RibbonDisplayMenu
            layout={layoutPref}
            collapsed={collapsed}
            onLayoutChange={onLayoutChange}
            onCollapsedChange={onCollapsedChange}
          />
        </div>
      </div>
      {showPanel ? (
        <div
          ref={panelRef}
          id={panelId}
          role="tabpanel"
          aria-labelledby={tabDomId(active.id)}
          className={cx('kiln-ribbon__panel', floating && 'kiln-ribbon__panel--floating')}
          onKeyDown={(e) => {
            if (e.key === 'Escape' && !e.defaultPrevented) {
              e.preventDefault();
              if (floating) setFloating(false);
              focusTab(active.id);
            }
          }}
        >
          <RibbonTabPanel key={`${active.id}-${layout}`} tab={active} layout={layout} />
          {!collapsed && onCollapsedChange ? (
            <Tooltip content={t('Collapse the Ribbon')} shortcut="Mod+F1">
              <button
                type="button"
                className="kiln-ribbon__collapse"
                aria-label={t('Collapse the Ribbon')}
                onClick={() => onCollapsedChange(true)}
              >
                <Icon icon={ChevronUp} size={14} />
              </button>
            </Tooltip>
          ) : null}
          {floating && onCollapsedChange ? (
            <Tooltip content={t('Pin the Ribbon')} shortcut="Mod+F1">
              <button
                type="button"
                className="kiln-ribbon__collapse"
                aria-label={t('Pin the Ribbon')}
                onClick={() => onCollapsedChange(false)}
              >
                <Icon icon={ChevronDown} size={14} />
              </button>
            </Tooltip>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function safeWhen(fn: NonNullable<RibbonTab['when']>, ctx: Parameters<NonNullable<RibbonTab['when']>>[0]) {
  try {
    return fn(ctx);
  } catch {
    return false;
  }
}

function RibbonDisplayMenu({
  layout,
  collapsed,
  onLayoutChange,
  onCollapsedChange,
}: {
  layout: RibbonLayoutPreference;
  collapsed: boolean;
  onLayoutChange?: (l: RibbonLayoutPreference) => void;
  onCollapsedChange?: (c: boolean) => void;
}) {
  if (!onLayoutChange && !onCollapsedChange) return null;
  const check = (on: boolean) => (on ? <Icon icon={Check} size={14} /> : null);
  return (
    <Menu modal={false}>
      <Tooltip content={t('Ribbon display options')}>
        <MenuTrigger asChild>
          <button
            type="button"
            className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-ribbon__display"
            aria-label={t('Ribbon display options')}
          >
            <Icon icon={ChevronDown} size={14} />
          </button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="end">
        {onCollapsedChange ? (
          <>
            <MenuLabel>{t('Show ribbon')}</MenuLabel>
            <MenuItem icon={check(!collapsed)} onSelect={() => onCollapsedChange(false)}>
              {t('Always show')}
            </MenuItem>
            <MenuItem icon={check(collapsed)} shortcut="Mod+F1" onSelect={() => onCollapsedChange(true)}>
              {t('Show tabs only')}
            </MenuItem>
          </>
        ) : null}
        {onLayoutChange ? (
          <>
            {onCollapsedChange ? <MenuSeparator /> : null}
            <MenuLabel>{t('Ribbon layout')}</MenuLabel>
            <MenuItem icon={check(layout === 'classic')} onSelect={() => onLayoutChange('classic')}>
              {t('Classic ribbon')}
            </MenuItem>
            <MenuItem icon={check(layout === 'simplified')} onSelect={() => onLayoutChange('simplified')}>
              {t('Simplified ribbon')}
            </MenuItem>
            <MenuItem icon={check(layout === 'auto')} onSelect={() => onLayoutChange('auto')}>
              {t('Automatic (fits the window)')}
            </MenuItem>
          </>
        ) : null}
      </MenuContent>
    </Menu>
  );
}

// ── Tab panel ────────────────────────────────────────────────────────────────────────────────────

const COLLAPSED_GROUP_WIDTH = 64;
const OVERFLOW_BUTTON_WIDTH = 40;

function defaultSize(it: RibbonLeafItem, layout: RibbonLayout, inRow: boolean): RibbonItemSize {
  const explicit = 'size' in it ? it.size : undefined;
  // Simplified ribbon (Office): one row, labels only on the former large commands (Paste ▾, Find).
  if (layout === 'simplified') return explicit === 'large' ? 'medium' : 'small';
  return explicit ?? (inRow ? 'small' : 'medium');
}

/** Key Tips for every item of a tab, Office letters first. */
function useItemKeyTips(tab: RibbonTab, layout: RibbonLayout): Map<string, string> {
  const { registry } = useCommands();
  return useMemo(() => {
    const candidates: { id: string; label: string; preferred?: string }[] = [];
    for (const g of tab.groups) {
      leafItems(g).forEach((it, i) => {
        if (it.kind === 'spacer') return;
        const id = itemId(it, i);
        const command = 'command' in it ? it.command : undefined;
        const meta = command ? registry.get(command) : undefined;
        const label = ('label' in it && it.label) || meta?.title || id;
        candidates.push({
          id,
          label,
          preferred: normalizeKeyTip(('keyTip' in it ? it.keyTip : undefined) ?? meta?.keyTip),
        });
      });
      if (g.launcher)
        candidates.push({
          id: `launcher:${g.id}`,
          label: t('{label} settings', { label: g.label }),
          preferred: normalizeKeyTip(g.launcher.keyTip),
        });
    }
    // 00 is the Simplified ribbon's "More commands" button; the Classic ribbon keeps Office's 0
    // (Excel's Increase Decimal).
    return assignKeyTips(candidates, layout === 'simplified' ? ['00'] : []);
  }, [tab, registry, layout]);
}

interface PanelProps {
  tab: RibbonTab;
  layout: RibbonLayout;
}

function RibbonTabPanel({ tab, layout }: PanelProps) {
  const tips = useItemKeyTips(tab, layout);
  // `tab` comes from `visibleTab`: only groups with something to show.
  const groups = tab.groups as RibbonGroup[];
  const level = `tab:${tab.id}`;
  return layout === 'classic' ? (
    <ClassicGroups tab={tab} groups={groups} tips={tips} level={level} />
  ) : (
    <SimplifiedGroups tab={tab} groups={groups} tips={tips} level={level} />
  );
}

interface GroupsProps {
  tab: RibbonTab;
  groups: RibbonGroup[];
  tips: Map<string, string>;
  level: string;
}

function ClassicGroups({ tab, groups, tips, level }: GroupsProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const widths = useRef(new Map<string, number>());
  const [collapsedIds, setCollapsedIds] = useState<ReadonlySet<string>>(new Set());
  const available = useElementWidth(containerRef, 0);

  useLayoutEffect(() => {
    const root = containerRef.current;
    if (!root) return;
    for (const el of root.querySelectorAll<HTMLElement>('[data-ribbon-group]')) {
      const id = el.dataset.ribbonGroup as string;
      if (el.dataset.collapsed !== 'true') widths.current.set(id, el.getBoundingClientRect().width);
    }
    if (available <= 0) return;
    const order = [...groups]
      .map((g, i) => ({ g, i }))
      .sort((a, b) => (a.g.priority ?? 0) - (b.g.priority ?? 0) || b.i - a.i);
    let total = groups.reduce((s, g) => s + (widths.current.get(g.id) ?? 0), 0);
    const next = new Set<string>();
    for (const { g } of order) {
      if (total <= available - 8) break;
      const w = widths.current.get(g.id) ?? 0;
      if (w <= COLLAPSED_GROUP_WIDTH) continue;
      next.add(g.id);
      total -= w - COLLAPSED_GROUP_WIDTH;
    }
    const same = next.size === collapsedIds.size && [...next].every((id) => collapsedIds.has(id));
    if (!same) setCollapsedIds(next);
  });

  return (
    <Toolbar
      ref={containerRef}
      aria-label={t('{label} commands', { label: tab.label })}
      className="kiln-ribbon__groups"
    >
      {groups.map((g) =>
        collapsedIds.has(g.id) ? (
          <CollapsedGroup key={g.id} group={g} tips={tips} level={level} />
        ) : (
          <ClassicGroup key={g.id} group={g} tips={tips} level={level} />
        ),
      )}
    </Toolbar>
  );
}

type Column =
  | { type: 'single'; item: RibbonLeafItem; index: number }
  | { type: 'stack'; rows: { item: RibbonItem; index: number }[] };

function columnsOf(group: RibbonGroup): Column[] {
  const rows = group.rows ?? 3;
  const cols: Column[] = [];
  let stack: { item: RibbonItem; index: number }[] = [];
  const flush = () => {
    if (stack.length) cols.push({ type: 'stack', rows: stack });
    stack = [];
  };
  group.items.forEach((it, index) => {
    const own = it.kind === 'gallery' || it.kind === 'spacer' || ('size' in it && it.size === 'large');
    if (own) {
      flush();
      cols.push({ type: 'single', item: it as RibbonLeafItem, index });
      return;
    }
    stack.push({ item: it, index });
    if (stack.length >= rows) flush();
  });
  flush();
  return cols;
}

function ClassicGroup({
  group,
  tips,
  level,
  inPopover,
}: {
  group: RibbonGroup;
  tips: Map<string, string>;
  level: string;
  inPopover?: boolean;
}) {
  const cols = columnsOf(group);
  const leafIndex = new Map(leafItems(group).map((it, i) => [it, i]));
  const tipFor = (it: RibbonLeafItem) => tips.get(itemId(it, leafIndex.get(it) ?? 0));
  const renderLeaf = (it: RibbonLeafItem, inRow: boolean) => (
    <RibbonItemView
      key={itemId(it, leafIndex.get(it) ?? 0)}
      item={it}
      size={defaultSize(it, 'classic', inRow)}
      layout="classic"
      keyTip={tipFor(it)}
      keyTipLevel={level}
    />
  );
  return (
    // biome-ignore lint/a11y/useSemanticElements: a WAI-ARIA group of ribbon commands, not a form fieldset
    <div
      className={cx('kiln-ribbon__group', `kiln-ribbon__group--rows-${group.rows ?? 3}`)}
      data-group={group.id}
      role="group"
      aria-label={group.label}
      data-ribbon-group={inPopover ? undefined : group.id}
    >
      <div className="kiln-ribbon__group-body">
        {cols.map((col, ci) =>
          col.type === 'single' ? (
            <div key={ci} className="kiln-ribbon__col kiln-ribbon__col--single">
              {renderLeaf(col.item, false)}
            </div>
          ) : (
            <div key={ci} className="kiln-ribbon__col kiln-ribbon__col--stack">
              {col.rows.map(({ item, index }) =>
                item.kind === 'row' ? (
                  <div key={item.id ?? `row-${index}`} className="kiln-ribbon__row">
                    {item.items.map((leaf) => renderLeaf(leaf, true))}
                  </div>
                ) : (
                  <div key={itemId(item, index)} className="kiln-ribbon__row">
                    {renderLeaf(item, false)}
                  </div>
                ),
              )}
            </div>
          ),
        )}
      </div>
      <div className="kiln-ribbon__group-label">
        <span>{group.label}</span>
        {group.launcher ? (
          <DialogLauncher
            command={group.launcher.command}
            label={group.launcher.label}
            groupLabel={group.label}
            keyTip={tips.get(`launcher:${group.id}`)}
            keyTipLevel={level}
          />
        ) : null}
      </div>
    </div>
  );
}

/** A Classic group squeezed into one button that opens the full group (Office does this when narrow). */
function CollapsedGroup({
  group,
  tips,
  level,
}: {
  group: RibbonGroup;
  tips: Map<string, string>;
  level: string;
}) {
  const { registry } = useCommands();
  const first = leafItems(group).find((it) => 'command' in it && it.command);
  const icon =
    group.icon ??
    (first && 'command' in first && first.command ? registry.get(first.command)?.icon : undefined);
  return (
    // biome-ignore lint/a11y/useSemanticElements: a WAI-ARIA group of ribbon commands, not a form fieldset
    <div
      className="kiln-ribbon__group kiln-ribbon__group--collapsed"
      role="group"
      aria-label={group.label}
      data-ribbon-group={group.id}
      data-collapsed="true"
    >
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="kiln-btn kiln-btn--toolbar kiln-btn--stacked kiln-ribbon__btn kiln-ribbon__btn--large"
            data-keytip-level={level}
          >
            <span className="kiln-btn__icon">
              <RibbonIcon name={icon} size={32} />
            </span>
            <span className="kiln-btn__label">
              {group.label}
              <Icon icon={ChevronDown} size={12} />
            </span>
          </button>
        </PopoverTrigger>
        <PopoverContent className="kiln-ribbon__group-popover" aria-label={group.label}>
          <Toolbar aria-label={group.label} className="kiln-ribbon__groups kiln-ribbon__groups--popover">
            <ClassicGroup group={group} tips={tips} level={level} inPopover />
          </Toolbar>
        </PopoverContent>
      </Popover>
    </div>
  );
}

function SimplifiedGroups({ tab, groups, tips, level }: GroupsProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const widths = useRef(new Map<string, number>());
  const available = useElementWidth(containerRef, 0);
  const [visibleCount, setVisibleCount] = useState(Number.POSITIVE_INFINITY);

  const entries = useMemo(() => {
    const out: { key: string; item: RibbonLeafItem; group: RibbonGroup; first: boolean }[] = [];
    for (const g of groups) {
      let first = true;
      leafItems(g).forEach((it, i) => {
        if ('hideInSimplified' in it && it.hideInSimplified) return;
        if (it.kind === 'spacer') return;
        out.push({ key: itemId(it, i), item: it, group: g, first });
        first = false;
      });
    }
    return out;
  }, [groups]);

  useLayoutEffect(() => {
    const root = containerRef.current;
    if (!root) return;
    for (const el of root.querySelectorAll<HTMLElement>('[data-ribbon-entry]')) {
      widths.current.set(el.dataset.ribbonEntry as string, el.getBoundingClientRect().width);
    }
    if (available <= 0) return;
    const widthOf = (e: (typeof entries)[number]) => (widths.current.get(e.key) ?? 0) + (e.first ? 9 : 2);
    const total = entries.reduce((sum, e) => sum + widthOf(e), 0);
    let fit = entries.length;
    if (total > available) {
      let used = 0;
      fit = 0;
      for (const e of entries) {
        if (used + widthOf(e) > available - OVERFLOW_BUTTON_WIDTH) break;
        used += widthOf(e);
        fit++;
      }
    }
    const next = fit >= entries.length ? Number.POSITIVE_INFINITY : fit;
    if (next !== visibleCount) setVisibleCount(next);
  });

  const shown = entries.slice(0, visibleCount);
  const overflow = entries.slice(shown.length);
  return (
    <Toolbar
      ref={containerRef}
      aria-label={t('{label} commands', { label: tab.label })}
      className="kiln-ribbon__groups kiln-ribbon__groups--simplified"
    >
      {shown.map((e) => (
        <Fragment key={`${e.group.id}:${e.key}`}>
          {e.first && e !== shown[0] ? <span className="kiln-ribbon__group-sep" aria-hidden="true" /> : null}
          <span className="kiln-ribbon__entry" data-ribbon-entry={e.key}>
            <RibbonItemView
              item={e.item}
              size={defaultSize(e.item, 'simplified', false)}
              layout="simplified"
              keyTip={tips.get(e.key)}
              keyTipLevel={level}
            />
          </span>
        </Fragment>
      ))}
      {overflow.length ? (
        <Popover>
          <Tooltip content={t('More commands')}>
            <PopoverTrigger asChild>
              <button
                type="button"
                className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only kiln-ribbon__more"
                aria-label={t('More commands')}
                data-keytip="00"
                data-keytip-level={level}
              >
                <span className="kiln-btn__icon">
                  <Icon icon={Ellipsis} size={20} />
                </span>
              </button>
            </PopoverTrigger>
          </Tooltip>
          <PopoverContent className="kiln-ribbon__overflow" align="end" aria-label={t('More commands')}>
            <Toolbar
              orientation="vertical"
              aria-label={t('More commands')}
              className="kiln-ribbon__overflow-list"
            >
              {overflow.map((e) => (
                <Fragment key={`${e.group.id}:${e.key}`}>
                  {e.first ? <div className="kiln-ribbon__overflow-heading">{e.group.label}</div> : null}
                  <RibbonItemView
                    item={e.item}
                    size="medium"
                    layout="simplified"
                    keyTip={tips.get(e.key)}
                    keyTipLevel={level}
                  />
                </Fragment>
              ))}
            </Toolbar>
          </PopoverContent>
        </Popover>
      ) : null}
    </Toolbar>
  );
}

/** What a visible tab list shows, as text: tab, group and item ids in order. */
function tabsSignature(tabs: readonly RibbonTab[]): string {
  return tabs
    .map(
      (t) =>
        `${t.id}[${t.groups
          .map(
            (g) =>
              `${g.id}${g.launcher ? '↘' : ''}(${g.items
                .map((it, i) => (it.kind === 'row' ? `{${it.items.map(itemId).join(',')}}` : itemId(it, i)))
                .join(',')})`,
          )
          .join(';')}]`,
    )
    .join('|');
}

/**
 * The tabs of `spec` this build shows for the current context, re-rendering only when that set
 * changes. Registry changes are frequent (the core's state after every keystroke, the selection's
 * formatting) and each item follows its own command state, so the ribbon itself must not rebuild
 * for them (typing latency).
 */
export function useVisibleTabs(spec: RibbonSpec, registry: CommandRegistry): RibbonTab[] {
  const cache = useRef<{ spec: RibbonSpec; sig: string; tabs: RibbonTab[] } | null>(null);
  const subscribe = useCallback(
    (cb: () => void) =>
      registry.subscribe((e) => {
        if (e.type !== 'executed') cb();
      }),
    [registry],
  );
  const get = useCallback(() => {
    const tabs = spec.tabs
      .filter((t) => !t.when || safeWhen(t.when, registry.context))
      // Hidden commands take their rows, groups and tabs with them (no empty slabs in release).
      .map((t) => visibleTab(registry, t))
      .filter((t): t is RibbonTab => t !== null);
    const sig = tabsSignature(tabs);
    const c = cache.current;
    if (c && c.spec === spec && c.sig === sig) return c.tabs;
    cache.current = { spec, sig, tabs };
    return tabs;
  }, [spec, registry]);
  return useSyncExternalStore(subscribe, get, get);
}
