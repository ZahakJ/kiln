import * as DM from '@radix-ui/react-dropdown-menu';
import { type ReactNode, useEffect, useId, useRef, useState } from 'react';
import { CommandIcon } from '../chrome/commandIcons.tsx';
import { cx } from '../primitives/cx.ts';
import { MenuCheckboxItem, MenuContent, MenuLabel } from '../primitives/Menu.tsx';
import { NumberInput } from '../primitives/NumberInput.tsx';
import { Popover, PopoverAnchor, PopoverContent } from '../primitives/Popover.tsx';
import { ZoomSlider } from '../primitives/Slider.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';
// The primitives' base styles. Their barrel (primitives/index.ts) imports them too, but a bundler
// skips a pure re-export barrel, and its CSS with it, in production builds. Importing them here,
// at the barrel's place in the module order (ui-kit index: primitives, ribbon, then statusbar),
// keeps the cascade of development and release builds identical: component rules first, then the
// primitives, then the status bar. chrome.spec's release-build test fails if they go missing.
import '../primitives/primitives.css';
import './statusbar.css';

export interface StatusBarItem {
  id: string;
  /** Visible content ("Page 3 of 12", "1,284 words"). */
  content: ReactNode;
  /** Name in the "Customize Status Bar" menu and accessible label when `content` is not text. */
  label: string;
  /** Makes the item a button (Word count → Word Count dialog, Language → language picker). */
  onClick?: () => void;
  /** Tooltip. */
  title?: string;
  /** Hidden until the user turns it on in the Customize menu. */
  defaultHidden?: boolean;
}

export interface StatusBarView {
  id: string;
  label: string;
  /** Icon name (glyph or Lucide). */
  icon: string;
}

export interface StatusBarZoom {
  /** Percent. */
  value: number;
  onChange: (value: number) => void;
  /** Presets in the zoom popover. Default 200, 150, 100, 75, 50. */
  presets?: readonly number[];
  /** App-specific fits ("Page width", "Whole page", "Fit slide to window"). */
  fits?: readonly { id: string; label: string; onSelect: () => void }[];
}

export interface StatusBarProps {
  left?: readonly StatusBarItem[];
  right?: readonly StatusBarItem[];
  /** View switcher (Print Layout / Web Layout / Read Mode…). */
  views?: { value: string; options: readonly StatusBarView[]; onChange: (id: string) => void };
  zoom?: StatusBarZoom;
  /** Remember which items the user hid (localStorage key). Default: not persisted. */
  storageKey?: string;
  className?: string;
}

function loadHidden(key: string | undefined, items: readonly StatusBarItem[]): Set<string> {
  const defaults = new Set(items.filter((i) => i.defaultHidden).map((i) => i.id));
  if (!key) return defaults;
  try {
    const raw = localStorage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return new Set(parsed.filter((x): x is string => typeof x === 'string'));
    }
  } catch {
    // ignore
  }
  return defaults;
}

/**
 * Office status bar: document facts on the left, view buttons and zoom on the right. Right-click
 * opens "Customize Status Bar" to show or hide items; the zoom percentage opens presets.
 */
export function StatusBar({ left = [], right = [], views, zoom, storageKey, className }: StatusBarProps) {
  const all = [...left, ...right];
  const [hidden, setHidden] = useState<Set<string>>(() => loadHidden(storageKey, all));
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);

  const toggle = (id: string) => {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setHidden(next);
    if (storageKey) {
      try {
        localStorage.setItem(storageKey, JSON.stringify([...next]));
      } catch {
        // ignore
      }
    }
  };

  const renderItem = (item: StatusBarItem) => {
    if (hidden.has(item.id)) return null;
    const body = item.onClick ? (
      <button
        type="button"
        className="kiln-statusbar__item kiln-statusbar__item--button"
        onClick={item.onClick}
        data-status={item.id}
      >
        {item.content}
      </button>
    ) : (
      <span className="kiln-statusbar__item" data-status={item.id}>
        {item.content}
      </span>
    );
    return item.title ? (
      <Tooltip key={item.id} content={item.title} side="top">
        {body}
      </Tooltip>
    ) : (
      <span key={item.id} className="kiln-statusbar__cell">
        {body}
      </span>
    );
  };

  return (
    <section
      className={cx('kiln-statusbar', className)}
      aria-label="Status bar"
      onContextMenu={(e) => {
        if (all.length === 0) return;
        e.preventDefault();
        setMenuAt({ x: e.clientX, y: e.clientY });
      }}
    >
      <div className="kiln-statusbar__left" role="status" aria-live="polite">
        {left.map(renderItem)}
      </div>
      <div className="kiln-statusbar__right">
        {right.map(renderItem)}
        {views ? (
          <div className="kiln-statusbar__views" role="radiogroup" aria-label="Views">
            {views.options.map((v) => (
              <Tooltip key={v.id} content={v.label} side="top">
                {/* biome-ignore lint/a11y/useSemanticElements: icon toggle buttons acting as radios (Office view switcher) */}
                <button
                  type="button"
                  role="radio"
                  aria-checked={views.value === v.id}
                  aria-label={v.label}
                  className={cx('kiln-statusbar__view', views.value === v.id && 'is-active')}
                  onClick={() => views.onChange(v.id)}
                  onKeyDown={(e) => {
                    const i = views.options.findIndex((o) => o.id === v.id);
                    const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
                    if (!d) return;
                    e.preventDefault();
                    const next = views.options[(i + d + views.options.length) % views.options.length];
                    if (next) {
                      views.onChange(next.id);
                      (
                        e.currentTarget.parentElement?.querySelector(
                          `[aria-label="${CSS.escape(next.label)}"]`,
                        ) as HTMLElement | null
                      )?.focus();
                    }
                  }}
                  tabIndex={views.value === v.id ? 0 : -1}
                >
                  <CommandIcon name={v.icon} size={16} />
                </button>
              </Tooltip>
            ))}
          </div>
        ) : null}
        {zoom ? <ZoomControl {...zoom} /> : null}
      </div>
      {menuAt ? (
        <DM.Root open onOpenChange={(o) => !o && setMenuAt(null)}>
          <DM.Trigger asChild>
            <span
              aria-hidden="true"
              style={{ position: 'fixed', left: menuAt.x, top: menuAt.y, width: 0, height: 0 }}
            />
          </DM.Trigger>
          <MenuContent side="top" align="start" aria-label="Customize Status Bar">
            <MenuLabel>Customize Status Bar</MenuLabel>
            {all.map((item) => (
              <MenuCheckboxItem
                key={item.id}
                checked={!hidden.has(item.id)}
                onSelect={(e) => {
                  e.preventDefault(); // keep the menu open, as Office does
                  toggle(item.id);
                }}
              >
                {item.label}
              </MenuCheckboxItem>
            ))}
          </MenuContent>
        </DM.Root>
      ) : null}
    </section>
  );
}

const DEFAULT_PRESETS = [200, 150, 100, 75, 50] as const;

/** − slider + and the percentage, which opens a popover with presets, fits and a custom value. */
export function ZoomControl({ value, onChange, presets = DEFAULT_PRESETS, fits = [] }: StatusBarZoom) {
  const [open, setOpen] = useState(false);
  const [custom, setCustomState] = useState(value);
  // Enter in the field commits and submits in the same event: read the latest value from a ref.
  const customRef = useRef(value);
  const setCustom = (v: number) => {
    customRef.current = v;
    setCustomState(v);
  };
  const firstRef = useRef<HTMLButtonElement>(null);
  const labelId = useId();
  useEffect(() => {
    if (open) {
      customRef.current = value;
      setCustomState(value);
    }
  }, [open, value]);
  const choose = (v: number) => {
    onChange(Math.round(Math.min(500, Math.max(10, v))));
    setOpen(false);
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverAnchor asChild>
        <div className="kiln-statusbar__zoom">
          <ZoomSlider value={value} onValueChange={onChange} onLabelClick={() => setOpen(true)} />
        </div>
      </PopoverAnchor>
      <PopoverContent
        side="top"
        align="end"
        className="kiln-statusbar__zoom-popover"
        aria-label="Zoom"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          firstRef.current?.focus();
        }}
      >
        <div className="kiln-statusbar__zoom-title">Zoom</div>
        {/* biome-ignore lint/a11y/useSemanticElements: a group of buttons, not form fields */}
        <div className="kiln-statusbar__zoom-presets" role="group" aria-label="Zoom presets">
          {presets.map((p, i) => (
            <button
              key={p}
              ref={i === 0 ? firstRef : undefined}
              type="button"
              className={cx(
                'kiln-btn kiln-btn--ghost kiln-btn--sm kiln-statusbar__preset',
                p === value && 'is-active',
              )}
              aria-pressed={p === value}
              onClick={() => choose(p)}
            >
              {p}%
            </button>
          ))}
        </div>
        {fits.length ? (
          // biome-ignore lint/a11y/useSemanticElements: a group of buttons, not form fields
          <div className="kiln-statusbar__zoom-fits" role="group" aria-label="Fit">
            {fits.map((f) => (
              <button
                key={f.id}
                type="button"
                className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-statusbar__preset"
                onClick={() => {
                  f.onSelect();
                  setOpen(false);
                }}
              >
                {f.label}
              </button>
            ))}
          </div>
        ) : null}
        <form
          className="kiln-statusbar__zoom-custom"
          onSubmit={(e) => {
            e.preventDefault();
            choose(customRef.current);
          }}
        >
          <span id={labelId}>Percent</span>
          <NumberInput
            aria-labelledby={labelId}
            value={custom}
            onValueChange={setCustom}
            unit="%"
            min={10}
            max={500}
            step={10}
            precision={0}
            width={92}
            size="sm"
          />
          <button type="submit" className="kiln-btn kiln-btn--primary kiln-btn--sm">
            OK
          </button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
