/**
 * The ribbon sheet: the phone's whole ribbon in a bottom sheet. The tabs are a row
 * of chips (Home, Insert, Layout…, contextual tabs while their object is selected) and each group
 * is a tile under its desktop name, holding the ribbon's own controls, so every command of the
 * desktop ribbon is here, from the same registry, with the same menus, galleries and state.
 */
import { t } from '@kiln/i18n';
import { type KeyboardEvent, useEffect, useMemo, useRef } from 'react';
import { useCommands } from '../chrome/CommandContext.tsx';
import { resolveCommandIcon } from '../chrome/commandIcons.tsx';
import { Icon } from '../icons/Icon.tsx';
import { cx } from '../primitives/cx.ts';
import { Toolbar } from '../primitives/Toolbar.tsx';
import { itemId, leafItems } from '../ribbon/builders.ts';
import { DialogLauncher } from '../ribbon/DialogLauncher.tsx';
import { useVisibleTabs } from '../ribbon/Ribbon.tsx';
import { RibbonItemView } from '../ribbon/RibbonItem.tsx';
import type { RibbonGroup, RibbonItemSize, RibbonLeafItem, RibbonSpec, RibbonTab } from '../ribbon/types.ts';
import { BottomSheet } from './BottomSheet.tsx';

export interface RibbonSheetProps {
  spec: RibbonSpec;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The tab shown (controlled: the bottom toolbar's first button names it). */
  tab: string;
  onTabChange: (tab: string) => void;
}

/** The tabs a ribbon sheet shows now: regular tabs, then the contextual ones. */
export function useSheetTabs(spec: RibbonSpec): RibbonTab[] {
  const { registry } = useCommands();
  const tabs = useVisibleTabs(spec, registry);
  return useMemo(() => [...tabs.filter((x) => !x.contextual), ...tabs.filter((x) => x.contextual)], [tabs]);
}

/** A tile's controls: labelled where the desktop shows a big button, icons elsewhere. */
function sizeOf(item: RibbonLeafItem): RibbonItemSize {
  if (item.kind === 'combobox' || item.kind === 'gallery' || item.kind === 'numberinput') return 'medium';
  if ('checkbox' in item && item.checkbox) return 'medium';
  return 'size' in item && item.size === 'large' ? 'medium' : 'small';
}

export function RibbonSheet({ spec, open, onOpenChange, tab, onTabChange }: RibbonSheetProps) {
  const tabs = useSheetTabs(spec);
  const active = tabs.find((x) => x.id === tab) ?? tabs[0];
  const chips = useRef(new Map<string, HTMLButtonElement>());
  const body = useRef<HTMLDivElement>(null);

  // The chosen chip scrolls into view, no chip is left cut at the strip's start, and a new tab
  // starts at its first group.
  useEffect(() => {
    if (!open || !active) return;
    const id = requestAnimationFrame(() => {
      const chip = chips.current.get(active.id);
      const strip = chip?.parentElement;
      if (strip && chip) {
        const rtl = getComputedStyle(strip).direction === 'rtl';
        // Distance along the reading direction from the strip's start edge.
        const s = strip.getBoundingClientRect();
        const from = (r: DOMRect) => (rtl ? s.right - r.right : r.left - s.left);
        const to = (r: DOMRect) => (rtl ? s.right - r.left : r.right - s.left);
        const step = (d: number) => {
          strip.scrollLeft += rtl ? -d : d;
        };
        // The chosen chip, whole.
        const c = chip.getBoundingClientRect();
        if (from(c) < 0) step(from(c) - 4);
        else if (to(c) > s.width) step(to(c) - s.width + 24);
        // A chip cut at the start edge: shown whole, or (if that hides the chosen one) not at all.
        for (const el of strip.querySelectorAll<HTMLElement>('[role="tab"]')) {
          const r = el.getBoundingClientRect();
          if (from(r) < -0.5 && to(r) > 0.5) {
            const back = from(r) - 2;
            const chosen = chip.getBoundingClientRect();
            if (to(chosen) - back <= s.width) step(back);
            else step(to(r) + 2);
            break;
          }
        }
        onChipsScroll(strip);
      }
      body.current?.scrollTo?.({ top: 0 });
    });
    return () => cancelAnimationFrame(id);
  }, [open, active, tabs]);

  // The strip fades at an edge it can scroll past: a cut chip reads as more to come.
  const onChipsScroll = (el: HTMLElement) => {
    const max = el.scrollWidth - el.clientWidth;
    const pos = Math.abs(el.scrollLeft);
    if (pos > 1) el.dataset.moreStart = '';
    else delete el.dataset.moreStart;
    if (max - pos > 1) el.dataset.moreEnd = '';
    else delete el.dataset.moreEnd;
  };

  const onChipKey = (e: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const rtl = getComputedStyle(e.currentTarget).direction === 'rtl';
    let next: number | null = null;
    if (e.key === (rtl ? 'ArrowLeft' : 'ArrowRight')) next = (index + 1) % tabs.length;
    else if (e.key === (rtl ? 'ArrowRight' : 'ArrowLeft')) next = (index - 1 + tabs.length) % tabs.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = tabs.length - 1;
    if (next === null) return;
    e.preventDefault();
    const target = tabs[next];
    if (!target) return;
    onTabChange(target.id);
    chips.current.get(target.id)?.focus();
  };

  const panelId = 'kiln-ribbon-sheet-panel';
  return (
    <BottomSheet
      open={open}
      onOpenChange={onOpenChange}
      title={t('Ribbon')}
      titleHidden
      className="kiln-ribbon-sheet"
      testId="ribbon-sheet"
      header={
        <div
          className="kiln-ribbon-sheet__chips"
          role="tablist"
          aria-label={t('Ribbon tabs')}
          ref={(el) => {
            if (el) requestAnimationFrame(() => onChipsScroll(el));
          }}
          onScroll={(e) => onChipsScroll(e.currentTarget)}
        >
          {tabs.map((x, i) => {
            const selected = x.id === active?.id;
            return (
              <button
                key={x.id}
                ref={(el) => {
                  if (el) chips.current.set(x.id, el);
                  else chips.current.delete(x.id);
                }}
                type="button"
                role="tab"
                id={`kiln-ribbon-sheet-tab-${x.id}`}
                aria-selected={selected}
                aria-controls={selected ? panelId : undefined}
                tabIndex={selected ? 0 : -1}
                className={cx(
                  'kiln-ribbon-sheet__chip',
                  x.contextual && 'kiln-ribbon-sheet__chip--contextual',
                )}
                onClick={() => onTabChange(x.id)}
                onKeyDown={(e) => onChipKey(e, i)}
              >
                {x.contextual ? (
                  <span className="kiln-ribbon-sheet__chip-set">{x.contextual.label}</span>
                ) : null}
                <span>{x.label}</span>
              </button>
            );
          })}
        </div>
      }
    >
      {active ? (
        <div
          ref={body}
          id={panelId}
          role="tabpanel"
          aria-labelledby={`kiln-ribbon-sheet-tab-${active.id}`}
          className="kiln-ribbon-sheet__panel"
        >
          <Toolbar
            key={active.id}
            aria-label={t('{label} commands', { label: active.label })}
            orientation="horizontal"
            className="kiln-ribbon-sheet__tiles"
          >
            {(active.groups as RibbonGroup[]).map((g) => (
              <GroupTile key={g.id} group={g} />
            ))}
          </Toolbar>
        </div>
      ) : null}
    </BottomSheet>
  );
}

function GroupTile({ group }: { group: RibbonGroup }) {
  const items = leafItems(group);
  const icon = resolveCommandIcon(group.icon);
  return (
    <section className="kiln-ribbon-sheet__tile" aria-label={group.label} data-ribbon-group={group.id}>
      <div className="kiln-ribbon-sheet__tile-head">
        {icon ? <Icon icon={icon} size={14} /> : null}
        <h3 className="kiln-ribbon-sheet__tile-title">{group.label}</h3>
        {group.launcher ? (
          <DialogLauncher
            command={group.launcher.command}
            label={group.launcher.label}
            groupLabel={group.label}
          />
        ) : null}
      </div>
      <div className="kiln-ribbon-sheet__items">
        {items.map((it, i) =>
          it.kind === 'spacer' ? null : (
            <RibbonItemView key={itemId(it, i)} item={it} size={sizeOf(it)} layout="simplified" />
          ),
        )}
      </div>
    </section>
  );
}
