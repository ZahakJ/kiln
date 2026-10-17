/**
 * Insert ▸ Table: Word's grid picker (hover or arrow keys to size, click or Enter to insert),
 * with Insert Table…, Convert Text to Table below. It replaces the ribbon's Table menu in Write.
 */
import type { RibbonSpec } from '@kiln/ui-kit';
import {
  CommandIcon,
  Icon,
  Popover,
  PopoverContent,
  PopoverTrigger,
  useCommandShortcut,
  useExecute,
} from '@kiln/ui-kit';
import { ChevronDown } from 'lucide-react';
import { type KeyboardEvent, useRef, useState } from 'react';

const COLS = 10;
const ROWS = 8;

export function TableGridPicker({ onInsert }: { onInsert: (rows: number, cols: number) => void }) {
  const [size, setSize] = useState({ rows: 0, cols: 0 });
  const cells = useRef<(HTMLButtonElement | null)[]>([]);
  // Roving focus: the cell at the current size takes Tab; arrows move it (Word's keyboard picker).
  const focusR = Math.max(0, size.rows - 1);
  const focusC = Math.max(0, size.cols - 1);
  const moveTo = (r: number, c: number) => {
    const rr = Math.min(ROWS - 1, Math.max(0, r));
    const cc = Math.min(COLS - 1, Math.max(0, c));
    setSize({ rows: rr + 1, cols: cc + 1 });
    cells.current[rr * COLS + cc]?.focus();
  };
  const onKey = (e: KeyboardEvent, r: number, c: number) => {
    if (e.key === 'ArrowRight') moveTo(r, c + 1);
    else if (e.key === 'ArrowLeft') moveTo(r, c - 1);
    else if (e.key === 'ArrowDown') moveTo(r + 1, c);
    else if (e.key === 'ArrowUp') moveTo(r - 1, c);
    else return;
    e.preventDefault();
  };
  const label = size.rows && size.cols ? `${size.cols}×${size.rows} Table` : 'Insert Table';
  return (
    <div className="kiln-write-gridpicker">
      <p className="kiln-write-gridpicker__label" aria-live="polite">
        {label}
      </p>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: leaving the grid with the mouse only clears the hover preview; the cells are the controls */}
      <div
        className="kiln-write-gridpicker__grid"
        data-testid="write-table-grid"
        onMouseLeave={() => setSize({ rows: 0, cols: 0 })}
      >
        {Array.from({ length: ROWS }, (_, r) => (
          <div key={r} className="kiln-write-gridpicker__row">
            {Array.from({ length: COLS }, (_, c) => {
              const on = r < size.rows && c < size.cols;
              return (
                <button
                  key={c}
                  ref={(el) => {
                    cells.current[r * COLS + c] = el;
                  }}
                  type="button"
                  tabIndex={r === focusR && c === focusC ? 0 : -1}
                  className="kiln-write-gridpicker__cell"
                  data-on={on || undefined}
                  aria-label={`Insert a table of ${c + 1} by ${r + 1}`}
                  onMouseEnter={() => setSize({ rows: r + 1, cols: c + 1 })}
                  onFocus={() => setSize({ rows: r + 1, cols: c + 1 })}
                  onKeyDown={(e) => onKey(e, r, c)}
                  onClick={() => onInsert(r + 1, c + 1)}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function TableButton({ size }: { size: 'large' | 'medium' | 'small' }) {
  const execute = useExecute();
  const shortcut = useCommandShortcut('suite.insert.table');
  const [open, setOpen] = useState(false);
  const run = (id: string, args?: unknown) => {
    setOpen(false);
    execute(id, args);
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={`kiln-btn kiln-btn--toolbar kiln-ribbon__btn kiln-ribbon__dropdown kiln-ribbon__btn--${size}${
            size === 'large' ? ' kiln-btn--stacked' : ''
          }`}
          aria-haspopup="dialog"
          aria-keyshortcuts={shortcut}
          data-command="suite.insert.table"
        >
          <span className="kiln-btn__icon">
            <CommandIcon name="InsertTable" size={size === 'large' ? 24 : 16} />
          </span>
          {size !== 'small' ? <span className="kiln-btn__label">Table</span> : null}
          <span className="kiln-btn__icon-after">
            <Icon icon={ChevronDown} size={12} />
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="kiln-write-gridpicker__popover" aria-label="Insert Table">
        <TableGridPicker
          onInsert={(rows, cols) => run('write.table.insert', { rows, cols, autofit: 'Fixed' })}
        />
        <div className="kiln-write-gridpicker__menu">
          <button type="button" className="kiln-write-menuitem" onClick={() => run('suite.insert.table')}>
            <CommandIcon name="InsertTable" size={16} />
            Insert Table…
          </button>
          <button
            type="button"
            className="kiln-write-menuitem"
            onClick={() => run('write.insert.convert_text_to_table', { separator: 'Tabs' })}
          >
            <CommandIcon name="ConvertTextToTable" size={16} />
            Convert Text to Table
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** Write's ribbon with the grid picker in Insert ▸ Tables. */
export function withTablePicker(spec: RibbonSpec): RibbonSpec {
  return {
    ...spec,
    tabs: spec.tabs.map((tab) =>
      tab.id !== 'insert'
        ? tab
        : {
            ...tab,
            groups: tab.groups.map((g) =>
              g.id !== 'tables'
                ? g
                : {
                    ...g,
                    items: g.items.map((item) =>
                      item.kind === 'dropdown' && item.command === 'suite.insert.table'
                        ? {
                            kind: 'custom' as const,
                            id: 'write.table.picker',
                            label: 'Table',
                            keyTip: 'T',
                            size: 'large' as const,
                            render: ({ layout }) => (
                              <TableButton size={layout === 'simplified' ? 'medium' : 'large'} />
                            ),
                          }
                        : item,
                    ),
                  },
            ),
          },
    ),
  };
}
