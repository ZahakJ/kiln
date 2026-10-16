import * as RP from '@radix-ui/react-popover';
import { ChevronDown } from 'lucide-react';
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

export interface ComboboxOption {
  value: string;
  /** Display text; defaults to `value`. */
  label?: string;
  /** Section header the option is listed under ("Theme Fonts", "All Fonts"…). */
  group?: string;
  /** Inline style for the option label, e.g. `{ fontFamily }` for a font preview. */
  style?: CSSProperties;
  /** Right-aligned hint (e.g. "(Headings)" in Office's font list). */
  hint?: ReactNode;
}

export interface ComboboxProps {
  /** Committed value. */
  value: string;
  onValueChange: (value: string) => void;
  options: readonly ComboboxOption[];
  /** Accessible name (required: the ribbon has no visible labels). */
  'aria-label': string;
  /** Accept text that is not in the list (Office allows any font name and sizes like 10.5). Default true. */
  allowCustomValue?: boolean;
  /**
   * Normalises typed text before committing; return `null` to reject (the field reverts), e.g.
   * a font size validator that accepts 1–1638 in 0.5 steps.
   */
  parse?: (text: string) => string | null;
  /** `contains` filters the list while typing; `none` keeps the full list and only highlights. */
  filter?: 'contains' | 'startsWith' | 'none';
  /** Inline autocompletion of the first prefix match (like Word's font box). Default true. */
  autocomplete?: boolean;
  placeholder?: string;
  disabled?: boolean;
  /** CSS width of the field. */
  width?: number | string;
  /** Max list height in px. Default 320. */
  listHeight?: number;
  size?: 'sm' | 'md';
  className?: string;
  id?: string;
  /** Shown when nothing matches. */
  emptyText?: ReactNode;
}

const labelOf = (o: ComboboxOption) => o.label ?? o.value;
const norm = (s: string) => s.trim().toLocaleLowerCase();

/**
 * Editable combobox (WAI-ARIA 1.2 combobox + listbox with `aria-activedescendant`) — the Office font
 * family / font size box. Type to filter with inline completion, ↓/↑ to move, Enter or Tab to commit,
 * Esc to close (and Esc again to revert), Alt+↓ to open without typing.
 */
export function Combobox({
  value,
  onValueChange,
  options,
  allowCustomValue = true,
  parse,
  filter = 'contains',
  autocomplete = true,
  placeholder,
  disabled,
  width,
  listHeight = 320,
  size = 'md',
  className,
  id,
  emptyText = 'No matches',
  ...aria
}: ComboboxProps) {
  const autoId = useId();
  const listId = `${autoId}-list`;
  const optionId = (i: number) => `${autoId}-opt-${i}`;
  const inputRef = useRef<HTMLInputElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const selected = options.find((o) => o.value === value);
  const display = selected ? labelOf(selected) : value;

  const [text, setText] = useState(display);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState<string | null>(null); // null = not filtering
  const [active, setActive] = useState(-1);
  const editing = useRef(false);
  /** True after ↑/↓ moved the active option since the last keystroke (Enter then picks it). */
  const navigated = useRef(false);
  /** Selection range to apply after the next render (inline completion highlights the completed tail). */
  const pendingSelection = useRef<[number, number] | null>(null);
  useLayoutEffect(() => {
    const range = pendingSelection.current;
    if (range && inputRef.current) {
      pendingSelection.current = null;
      inputRef.current.setSelectionRange(range[0], range[1]);
    }
  });

  // Follow external value changes while not editing.
  useEffect(() => {
    if (!editing.current) setText(display);
  }, [display]);

  const visible = useMemo(() => {
    if (query === null || filter === 'none' || query === '') return options;
    const q = norm(query);
    return options.filter((o) => {
      const l = norm(labelOf(o));
      return filter === 'startsWith' ? l.startsWith(q) : l.includes(q);
    });
  }, [options, query, filter]);

  const indexOfValue = (list: readonly ComboboxOption[], v: string) => list.findIndex((o) => o.value === v);

  const openList = (activeIndex?: number) => {
    if (disabled) return;
    setOpen(true);
    setActive(activeIndex ?? Math.max(0, indexOfValue(visible, value)));
  };

  // Keep the active option scrolled into view.
  useEffect(() => {
    if (!open || active < 0) return;
    const el = listRef.current?.querySelector<HTMLElement>(`#${CSS.escape(optionId(active))}`);
    el?.scrollIntoView?.({ block: 'nearest' });
  }, [open, active]);

  const commit = (raw: string) => {
    editing.current = false;
    setQuery(null);
    setOpen(false);
    const exact = options.find((o) => norm(labelOf(o)) === norm(raw) || norm(o.value) === norm(raw));
    let next: string | null = exact ? exact.value : null;
    if (next === null && allowCustomValue && raw.trim() !== '') next = parse ? parse(raw.trim()) : raw.trim();
    if (next === null) {
      setText(display);
      return;
    }
    const nextOption = options.find((o) => o.value === next);
    setText(nextOption ? labelOf(nextOption) : next);
    if (next !== value) onValueChange(next);
  };

  const commitOption = (o: ComboboxOption) => {
    commit(labelOf(o));
    inputRef.current?.focus();
  };

  const onInput = (raw: string, isDeletion: boolean) => {
    editing.current = true;
    navigated.current = false;
    setQuery(raw);
    let completed = raw;
    const candidates =
      filter === 'none' || raw === ''
        ? options
        : options.filter((o) =>
            filter === 'startsWith'
              ? norm(labelOf(o)).startsWith(norm(raw))
              : norm(labelOf(o)).includes(norm(raw)),
          );
    const prefix = raw === '' ? -1 : candidates.findIndex((o) => norm(labelOf(o)).startsWith(norm(raw)));
    if (autocomplete && !isDeletion && prefix >= 0 && raw.length > 0) {
      const full = labelOf(candidates[prefix] as ComboboxOption);
      completed = raw + full.slice(raw.length);
      pendingSelection.current = [raw.length, completed.length];
      setText(completed);
    } else {
      setText(raw);
    }
    setOpen(true);
    setActive(prefix >= 0 ? prefix : candidates.length > 0 && raw !== '' ? 0 : -1);
  };

  const move = (delta: number) => {
    if (!open) {
      openList();
      return;
    }
    if (visible.length === 0) return;
    navigated.current = true;
    const next = Math.min(visible.length - 1, Math.max(0, (active < 0 ? -1 : active) + delta));
    setActive(next);
    const o = visible[next];
    if (o) {
      editing.current = true;
      setText(labelOf(o));
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        if (e.altKey) openList();
        else move(1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        if (e.altKey) setOpen(false);
        else move(-1);
        break;
      case 'PageDown':
        if (open) {
          e.preventDefault();
          move(10);
        }
        break;
      case 'PageUp':
        if (open) {
          e.preventDefault();
          move(-10);
        }
        break;
      case 'Enter': {
        e.preventDefault();
        const o = open && navigated.current && active >= 0 ? visible[active] : undefined;
        navigated.current = false;
        commit(o ? labelOf(o) : text);
        inputRef.current?.select();
        break;
      }
      case 'Escape':
        if (open) {
          e.preventDefault();
          e.stopPropagation();
          setOpen(false);
          setQuery(null);
        } else if (editing.current || text !== display) {
          e.preventDefault();
          editing.current = false;
          setText(display);
        }
        break;
      case 'Tab':
        if (editing.current) commit(text);
        else setOpen(false);
        break;
      default:
    }
  };

  // Group headers are rendered when the visible options carry groups.
  const groups: Array<{ name: string | undefined; items: Array<{ o: ComboboxOption; i: number }> }> = [];
  visible.forEach((o, i) => {
    const last = groups[groups.length - 1];
    if (last && last.name === o.group) last.items.push({ o, i });
    else groups.push({ name: o.group, items: [{ o, i }] });
  });

  const renderOption = ({ o, i }: { o: ComboboxOption; i: number }) => (
    // Options stay unfocusable by design: focus remains in the input (aria-activedescendant pattern),
    // and the keyboard path is ↑/↓ + Enter on the input.
    // biome-ignore lint/a11y/useKeyWithClickEvents: keyboard handled by the combobox input
    // biome-ignore lint/a11y/useFocusableInteractive: aria-activedescendant pattern
    <div
      key={`${o.value}-${i}`}
      id={optionId(i)}
      role="option"
      aria-selected={o.value === value}
      data-active={i === active || undefined}
      className="kiln-combobox__option"
      onMouseDown={(e) => e.preventDefault()}
      onMouseMove={() => setActive(i)}
      onClick={() => commitOption(o)}
    >
      <span className="kiln-combobox__option-label" style={o.style}>
        {labelOf(o)}
      </span>
      {o.hint ? <span className="kiln-combobox__hint">{o.hint}</span> : null}
    </div>
  );

  return (
    <RP.Root open={open && !disabled} onOpenChange={(o) => !o && setOpen(false)}>
      <RP.Anchor asChild>
        <div
          ref={wrapperRef}
          className={cx(
            'kiln-field',
            'kiln-combobox',
            `kiln-field--${size}`,
            disabled && 'is-disabled',
            className,
          )}
          style={width !== undefined ? { width } : undefined}
        >
          <input
            ref={inputRef}
            id={id}
            className="kiln-combobox__input"
            role="combobox"
            aria-label={aria['aria-label']}
            aria-expanded={open}
            aria-controls={listId}
            aria-autocomplete={autocomplete ? 'both' : 'list'}
            aria-activedescendant={open && active >= 0 ? optionId(active) : undefined}
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            placeholder={placeholder}
            value={text}
            onChange={(e) => {
              const native = e.nativeEvent as InputEvent;
              onInput(
                e.target.value,
                typeof native.inputType === 'string' && native.inputType.startsWith('delete'),
              );
            }}
            onKeyDown={onKeyDown}
            onFocus={(e) => e.target.select()}
            onBlur={(e) => {
              if (wrapperRef.current?.contains(e.relatedTarget as Node | null)) return;
              if (listRef.current?.contains(e.relatedTarget as Node | null)) return;
              if (editing.current) commit(text);
              else setOpen(false);
            }}
          />
          <button
            type="button"
            className="kiln-combobox__toggle"
            tabIndex={-1}
            aria-label={`Show ${aria['aria-label']} list`}
            aria-controls={listId}
            aria-expanded={open}
            disabled={disabled}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              if (open) setOpen(false);
              else {
                setQuery(null);
                openList(Math.max(0, indexOfValue(options, value)));
              }
              inputRef.current?.focus();
            }}
          >
            <Icon icon={ChevronDown} size={14} />
          </button>
        </div>
      </RP.Anchor>
      <RP.Portal>
        <RP.Content
          className="kiln-menu kiln-combobox__popup"
          align="start"
          sideOffset={4}
          collisionPadding={8}
          onOpenAutoFocus={(e) => e.preventDefault()}
          onCloseAutoFocus={(e) => e.preventDefault()}
          onInteractOutside={(e) => {
            if (wrapperRef.current?.contains(e.target as Node)) e.preventDefault();
          }}
          style={{ minWidth: 'var(--radix-popover-trigger-width)' }}
        >
          <div
            ref={listRef}
            id={listId}
            role="listbox"
            aria-label={aria['aria-label']}
            className="kiln-combobox__list"
            style={{ maxHeight: listHeight }}
          >
            {visible.length === 0 ? <div className="kiln-combobox__empty">{emptyText}</div> : null}
            {groups.map((g, gi) =>
              g.name ? (
                // biome-ignore lint/a11y/useSemanticElements: role=group inside a listbox is the ARIA pattern
                <div
                  key={`${g.name}-${gi}`}
                  role="group"
                  aria-label={g.name}
                  className="kiln-combobox__group"
                >
                  <div className="kiln-menu__group-label" aria-hidden="true">
                    {g.name}
                  </div>
                  {g.items.map(renderOption)}
                </div>
              ) : (
                g.items.map(renderOption)
              ),
            )}
          </div>
        </RP.Content>
      </RP.Portal>
    </RP.Root>
  );
}
