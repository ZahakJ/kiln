import { assignKeyTips, isTierEnabled, normalizeKeyTip, TAB_KEYTIPS } from '@kiln/commands';
import { Check, ChevronDown, ChevronUp, Ellipsis } from 'lucide-react';
import {
  Fragment,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useCommands, useRegistryVersion } from '../chrome/CommandContext.tsx';
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
import './ribbon.css';

/** Width at which the automatic layout switches from Simplified to Classic (PLAN.md §1.2). */
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
  const version = useRegistryVersion();
  const baseId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef(new Map<string, HTMLButtonElement>());
  const panelRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(rootRef, typeof window === 'undefined' ? 1280 : window.innerWidth);
  const layout: RibbonLayout =
    layoutPref === 'auto' ? (width >= CLASSIC_MIN_WIDTH ? 'classic' : 'simplified') : layoutPref;

  const tabs = useMemo(
    () =>
      spec.tabs.filter(
        (t) => isTierEnabled(t.tier, registry.tiers) && (!t.when || safeWhen(t.when, registry.context)),
      ),
    [spec, registry, version],
  );
  const [activeState, setActiveState] = useState(defaultTab ?? tabs[0]?.id ?? '');
  const requested = activeTabProp ?? activeState;
  const active = tabs.find((t) => t.id === requested) ?? tabs[0];
  const setActive = (id: string) => {
    if (activeTabProp === undefined) setActiveState(id);
    onActiveTabChange?.(id);
  };

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

  const focusTab = (id: string) => tabRefs.current.get(id)?.focus();
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
        )}
        data-keytip={tabTips.get(t.id)}
        data-keytip-level="root"
        data-keytip-enter={`tab:${t.id}`}
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
            aria-label="File"
          >
            {fileButton ?? 'File'}
          </button>
        ) : null}
        <div className="kiln-ribbon__tablist" role="tablist" aria-label="Ribbon tabs">
          {regular.map(renderTab)}
          {contextualSets.map(([set, s]) => (
            <div key={set} className="kiln-ribbon__ctxset" role="presentation" data-set={set} title={s.label}>
              <span className="kiln-ribbon__ctxlabel" aria-hidden="true">
                {s.label}
              </span>
              <div className="kiln-ribbon__ctxtabs" role="presentation">
                {s.tabs.map(renderTab)}
              </div>
            </div>
          ))}
        </div>
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
            <Tooltip content="Collapse the Ribbon" shortcut="Mod+F1">
              <button
                type="button"
                className="kiln-ribbon__collapse"
                aria-label="Collapse the Ribbon"
                onClick={() => onCollapsedChange(true)}
              >
                <Icon icon={ChevronUp} size={14} />
              </button>
            </Tooltip>
          ) : null}
          {floating && onCollapsedChange ? (
            <Tooltip content="Pin the Ribbon" shortcut="Mod+F1">
              <button
                type="button"
                className="kiln-ribbon__collapse"
                aria-label="Pin the Ribbon"
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
      <Tooltip content="Ribbon display options">
        <MenuTrigger asChild>
          <button
            type="button"
            className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-ribbon__display"
            aria-label="Ribbon display options"
          >
            <Icon icon={ChevronDown} size={14} />
          </button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="end">
        {onCollapsedChange ? (
          <>
            <MenuLabel>Show ribbon</MenuLabel>
            <MenuItem icon={check(!collapsed)} onSelect={() => onCollapsedChange(false)}>
              Always show
            </MenuItem>
            <MenuItem icon={check(collapsed)} shortcut="Mod+F1" onSelect={() => onCollapsedChange(true)}>
              Show tabs only
            </MenuItem>
          </>
        ) : null}
        {onLayoutChange ? (
          <>
            {onCollapsedChange ? <MenuSeparator /> : null}
            <MenuLabel>Ribbon layout</MenuLabel>
            <MenuItem icon={check(layout === 'classic')} onSelect={() => onLayoutChange('classic')}>
              Classic ribbon
            </MenuItem>
            <MenuItem icon={check(layout === 'simplified')} onSelect={() => onLayoutChange('simplified')}>
              Simplified ribbon
            </MenuItem>
            <MenuItem icon={check(layout === 'auto')} onSelect={() => onLayoutChange('auto')}>
              Automatic (fits the window)
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
function useItemKeyTips(tab: RibbonTab): Map<string, string> {
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
        candidates.push({ id: `launcher:${g.id}`, label: `${g.label} settings`, preferred: undefined });
    }
    // 00 is the Simplified ribbon's "More commands" button.
    return assignKeyTips(candidates, ['00']);
  }, [tab, registry]);
}

interface PanelProps {
  tab: RibbonTab;
  layout: RibbonLayout;
}

function RibbonTabPanel({ tab, layout }: PanelProps) {
  const { registry } = useCommands();
  const tips = useItemKeyTips(tab);
  const groups = tab.groups.filter((g) => isTierEnabled(g.tier, registry.tiers));
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
    <Toolbar ref={containerRef} aria-label={`${tab.label} commands`} className="kiln-ribbon__groups">
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
      aria-label={`${tab.label} commands`}
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
          <Tooltip content="More commands">
            <PopoverTrigger asChild>
              <button
                type="button"
                className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only kiln-ribbon__more"
                aria-label="More commands"
                data-keytip="00"
                data-keytip-level={level}
              >
                <span className="kiln-btn__icon">
                  <Icon icon={Ellipsis} size={20} />
                </span>
              </button>
            </PopoverTrigger>
          </Tooltip>
          <PopoverContent className="kiln-ribbon__overflow" align="end" aria-label="More commands">
            <Toolbar orientation="vertical" aria-label="More commands" className="kiln-ribbon__overflow-list">
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
