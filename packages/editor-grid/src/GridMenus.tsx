/**
 * The sheet's context menus (Excel's): cells, row headers, column headers, charts and other
 * objects, and the error indicator's menu. The surface asks for one (right-click, Shift+F10, the
 * Menu key, the error button) by setting `menu` on the model after selecting what is under the
 * pointer; every item is a command of the window's registry. A right-click on cells also shows
 * Excel's mini toolbar above the menu: the ribbon's own Home controls.
 */
import { msg, t } from '@kiln/i18n';
import type { GridErrorCheck } from '@kiln/protocol';
import {
  COMBOBOX_COMMIT,
  type ComboboxCommit,
  CommandMenuItems,
  Menu,
  MenuContent,
  type MenuEntry,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
  type RibbonItem,
  RibbonItemView,
  type RibbonLeafItem,
  type RibbonSpec,
  Toolbar,
  useCommands,
  useExecute,
} from '@kiln/ui-kit';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel, GridMenu } from './model.ts';

const CLIPBOARD: MenuEntry[] = [
  { command: 'suite.edit.cut' },
  { command: 'suite.edit.copy' },
  { kind: 'heading', label: msg('Paste Options') },
  { command: 'suite.edit.paste' },
  { command: 'grid.clipboard.paste_values', label: msg('Paste Values') },
  { command: 'grid.clipboard.paste_formulas', label: msg('Paste Formulas') },
  { command: 'grid.clipboard.paste_formatting', label: msg('Paste Formatting') },
  { command: 'grid.clipboard.paste_transpose', label: msg('Transpose') },
  { command: 'suite.edit.paste_special', label: msg('Paste Special…') },
  { kind: 'separator' },
];

const FILTER: MenuEntry = {
  kind: 'submenu',
  label: msg('Filter'),
  icon: 'FilterToggle',
  items: [
    { command: 'grid.data.reapply_filter', label: msg('Reapply') },
    { kind: 'separator' },
    {
      command: 'grid.data.filter_by_selection',
      label: msg("Filter by Selected Cell's Value"),
      args: { by: 'Value' },
    },
    {
      command: 'grid.data.filter_by_selection',
      label: msg("Filter by Selected Cell's Color"),
      args: { by: 'FillColor' },
    },
    {
      command: 'grid.data.filter_by_selection',
      label: msg("Filter by Selected Cell's Font Color"),
      args: { by: 'FontColor' },
    },
    { kind: 'separator' },
    { command: 'grid.data.clear_filter', label: msg('Clear Filter') },
  ],
};

const SORT: MenuEntry = {
  kind: 'submenu',
  label: msg('Sort'),
  icon: 'SortAscending',
  items: [
    { command: 'grid.data.sort_ascending', label: msg('Sort A to Z') },
    { command: 'grid.data.sort_descending', label: msg('Sort Z to A') },
    { kind: 'separator' },
    {
      command: 'grid.data.sort_by_selection_color',
      label: msg('Put Selected Cell Color On Top'),
      args: { font: false },
    },
    {
      command: 'grid.data.sort_by_selection_color',
      label: msg('Put Selected Font Color On Top'),
      args: { font: true },
    },
    { kind: 'separator' },
    { command: 'grid.data.sort', label: msg('Custom Sort…') },
  ],
};

const OBJECT_ORDER: MenuEntry[] = [
  { command: 'suite.edit.cut' },
  { command: 'suite.edit.copy' },
  { command: 'suite.edit.paste' },
  { kind: 'separator' },
  { command: 'grid.drawing.delete', label: msg('Delete') },
  { command: 'ui.grid.bring_to_front', label: msg('Bring to Front') },
  { command: 'ui.grid.send_to_back', label: msg('Send to Back') },
  { kind: 'separator' },
];

/** The entries of a context menu of the given kind. */
export function gridMenuEntries(kind: GridMenu['kind']): MenuEntry[] {
  switch (kind) {
    case 'row':
      return [
        ...CLIPBOARD,
        { command: 'grid.cells.insert_rows', label: t('Insert') },
        { command: 'grid.cells.delete_rows', label: t('Delete') },
        { command: 'grid.editing.clear_contents', label: t('Clear Contents') },
        { kind: 'separator' },
        { command: 'grid.cells.format_cells', label: t('Format Cells…') },
        { command: 'ui.grid.row_height', label: t('Row Height…') },
        { command: 'grid.cells.hide_rows', label: t('Hide') },
        { command: 'grid.cells.unhide_rows', label: t('Unhide') },
      ];
    case 'column':
      return [
        ...CLIPBOARD,
        { command: 'grid.cells.insert_columns', label: t('Insert') },
        { command: 'grid.cells.delete_columns', label: t('Delete') },
        { command: 'grid.editing.clear_contents', label: t('Clear Contents') },
        { kind: 'separator' },
        { command: 'grid.cells.format_cells', label: t('Format Cells…') },
        { command: 'ui.grid.column_width', label: t('Column Width…') },
        { command: 'grid.cells.hide_columns', label: t('Hide') },
        { command: 'grid.cells.unhide_columns', label: t('Unhide') },
      ];
    case 'object':
      return [...OBJECT_ORDER, { command: 'ui.grid.format_object', label: t('Format Object…') }];
    case 'chart':
      return [...OBJECT_ORDER, { command: 'ui.grid.format_chart', label: t('Format Chart Area…') }];
    default:
      return [
        ...CLIPBOARD,
        { command: 'ui.grid.insert_cells', label: t('Insert…') },
        { command: 'ui.grid.delete_cells', label: t('Delete…') },
        { command: 'grid.editing.clear_contents', label: t('Clear Contents') },
        { kind: 'separator' },
        FILTER,
        SORT,
        { kind: 'separator' },
        { command: 'suite.review.new_comment', label: t('New Comment') },
        { command: 'grid.review.new_note', label: t('New Note') },
        { kind: 'separator' },
        { command: 'grid.cells.format_cells', label: t('Format Cells…') },
        { command: 'grid.editing.open_dropdown', label: t('Pick From Drop-down List…') },
        { command: 'grid.formulas.define_name', label: t('Define Name…') },
        { command: 'suite.insert.link', label: t('Link') },
      ];
  }
}

/** Command ids of the mini toolbar, Excel's two rows. */
export const MINI_TOOLBAR_ROWS: readonly (readonly string[])[] = [
  [
    'suite.format.font_family',
    'suite.format.font_size',
    'suite.format.grow_font',
    'suite.format.shrink_font',
    'grid.number.accounting',
    'grid.number.percent',
    'grid.number.comma',
    'suite.edit.format_painter',
  ],
  [
    'suite.format.bold',
    'suite.format.italic',
    'suite.format.align_center',
    'grid.font.fill_color',
    'suite.format.font_color',
    'grid.font.borders',
    'grid.alignment.merge_center',
    'grid.number.increase_decimal',
    'grid.number.decrease_decimal',
  ],
];

function flatten(items: readonly RibbonItem[]): RibbonLeafItem[] {
  return items.flatMap((i) => (i.kind === 'row' ? flatten(i.items) : [i]));
}

/** The mini toolbar's controls: the Home tab's own items for those commands, row by row. */
export function miniToolbarRows(ribbon: RibbonSpec | undefined): RibbonLeafItem[][] {
  const home = ribbon?.tabs.find((t) => t.id === 'home');
  if (!home) return [];
  const all = home.groups.flatMap((g) => flatten(g.items));
  return MINI_TOOLBAR_ROWS.map((row) =>
    row.flatMap((id): RibbonLeafItem[] => {
      const item = all.find((i) => 'command' in i && i.command === id);
      if (!item) return [];
      if (item.kind === 'combobox') return [{ ...item, width: id.endsWith('size') ? 52 : 116 }];
      return [item];
    }),
  ).filter((r) => r.length > 0);
}

const insideMini = (t: EventTarget | null) => t instanceof Element && !!t.closest('.kiln-contextmenu__mini');

/** How long after the user last pressed or clicked in the mini toolbar a command counts as its. */
const MINI_COMMAND_MS = 1500;

/**
 * The mini toolbar closes with the menu after one of its controls has formatted the selection, and
 * the keys go back to the sheet (Excel): a button, a pick from one of its lists, palettes or menus,
 * or a font or size typed or picked in its boxes (Enter or a click, even on the value already set).
 * Its lists and palettes are its own while they are open: a press in one (they are drawn outside
 * the toolbar) does not close the menu. Returns the handlers for the toolbar's element.
 */
function useMiniToolbar(model: GridEditorModel, active: boolean) {
  const { registry } = useCommands();
  // When the user last pressed a key or the pointer in the toolbar or a list it opened.
  const lastUse = useRef(Number.NEGATIVE_INFINITY);
  // The current press started in the toolbar's tree (React events reach it through portals).
  const pressInTree = useRef(false);
  const el = useRef<HTMLDivElement | null>(null);
  const close = useMemo(
    () => () =>
      setTimeout(() => {
        if (!model.state.menu) return;
        model.set({ menu: null });
        model.surface?.focus();
      }, 0),
    [model],
  );
  useEffect(() => {
    if (!active) return;
    const off = registry.subscribe((e) => {
      if (e.type === 'executed' && performance.now() - lastUse.current < MINI_COMMAND_MS) close();
    });
    const node = el.current;
    const onCommit = (e: Event) => {
      const via = (e as CustomEvent<ComboboxCommit>).detail?.via;
      if (via === 'enter' || via === 'pick') close();
    };
    node?.addEventListener(COMBOBOX_COMMIT, onCommit);
    return () => {
      off();
      node?.removeEventListener(COMBOBOX_COMMIT, onCommit);
    };
  }, [active, registry, close]);
  const used = () => {
    lastUse.current = performance.now();
  };
  return {
    ref: el,
    /** True (once) when the press or focus that a menu calls outside came from the toolbar. */
    ownsOutside: (target: EventTarget | null) => {
      const own = pressInTree.current || insideMini(target);
      pressInTree.current = false;
      return own;
    },
    handlers: {
      onPointerDownCapture: () => {
        pressInTree.current = true;
        used();
        // A press that no menu hears as outside must not count for the next one.
        setTimeout(() => {
          pressInTree.current = false;
        }, 0);
      },
      onKeyDownCapture: used,
      onFocusCapture: () => {
        pressInTree.current = true;
        setTimeout(() => {
          pressInTree.current = false;
        }, 0);
      },
    },
  };
}

/** Puts the caret at the end of the Formula Bar, which starts editing the active cell there. */
function editInFormulaBar(): void {
  const bar = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Formula Bar"]');
  if (!bar) return;
  bar.focus();
  bar.setSelectionRange(bar.value.length, bar.value.length);
}

/** The error indicator's menu: the error's name, then Excel's choices. */
function ErrorMenuItems({
  error,
  model,
  onFormulaBar,
}: {
  error: GridErrorCheck;
  model: GridEditorModel;
  onFormulaBar: () => void;
}) {
  const execute = useExecute();
  const text = error.rule === 'NumberAsText';
  return (
    <>
      <MenuLabel>{error.title}</MenuLabel>
      {text ? (
        <MenuItem onSelect={() => execute('grid.formulas.convert_to_number')}>
          {t('Convert to Number')}
        </MenuItem>
      ) : null}
      <MenuItem onSelect={() => model.openDialog('errorHelp')}>{t('Help on this Error')}</MenuItem>
      {text ? null : (
        <MenuItem onSelect={() => execute('grid.formulas.evaluate_formula')}>
          {t('Show Calculation Steps…')}
        </MenuItem>
      )}
      <MenuItem onSelect={() => execute('grid.formulas.ignore_error')}>{t('Ignore Error')}</MenuItem>
      <MenuItem onSelect={onFormulaBar}>{t('Edit in Formula Bar')}</MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={() => execute('grid.formulas.error_checking')}>{t('Error Checking…')}</MenuItem>
    </>
  );
}

/** Height of one row of the mini toolbar, px. */
const MINI_ROW_H = 34;

/**
 * Where a context menu and its mini toolbar go once the menu is drawn: a menu taller than the room
 * both below and above the point moves up until it fits the window (Excel's menus never run off
 * the screen), and the mini toolbar sits on the side of the point the menu leaves free (above the
 * menu when it covers the point).
 */
function useMenuPlace(
  menu: GridMenu | null,
  content: React.RefObject<HTMLDivElement | null>,
  rows: number,
): { top: number | null; mini: number | null; left: number | null } {
  const [place, setPlace] = useState<{ top: number | null; mini: number | null; left: number | null }>({
    top: null,
    mini: null,
    left: null,
  });
  useEffect(() => {
    setPlace({ top: null, mini: null, left: null });
    if (!menu || menu.kind === 'tab') return;
    let top: number | null = null;
    let mini: number | null = null;
    let left: number | null = null;
    let raf = 0;
    const tick = () => {
      const el = content.current;
      const r = el?.getBoundingClientRect();
      if (el && r && r.height > 0) {
        const vh = window.innerHeight;
        const room = 8;
        // The menu's own height (a menu squeezed into the room it was given scrolls).
        const full = Math.max(r.height, el.scrollHeight);
        if (top === null && menu.y + full + room > vh && menu.y - full - room < 0) {
          // The room below it is the window less the menu's offset and collision padding.
          top = Math.max(room, vh - full - 2 * room);
        }
        // A menu opened near the window's end edge (a long press on a phone) stays inside it:
        // with its place fixed, the menu does not shift by itself.
        // (Either way round: the menu's offset from its anchor is kept, its edges come inside.)
        const anchor = left ?? menu.x;
        const offset = r.left - anchor;
        const inside = Math.min(Math.max(r.left, room), Math.max(room, window.innerWidth - r.width - room));
        const nextLeft = Math.abs(inside - r.left) > 0.5 ? inside - offset : left;
        const h = MINI_ROW_H * rows;
        let next: number;
        if (r.top >= menu.y - 2 && top === null) next = Math.max(room, menu.y - room - h);
        else if (r.bottom <= menu.y + 2) next = menu.y + room;
        else next = r.top - room - h >= room ? r.top - room - h : Math.min(vh - h - room, r.bottom + room);
        if (next !== mini || top !== null || nextLeft !== left) {
          mini = next;
          left = nextLeft;
          setPlace((p) => (p.top === top && p.mini === mini && p.left === left ? p : { top, mini, left }));
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [menu, content, rows]);
  return place;
}

export function GridMenus({ model, ribbon }: { model: GridEditorModel; ribbon?: RibbonSpec }) {
  const menu = useGrid(model, (s) => s.menu);
  const error = useGrid(model, (s) => s.grid?.surface?.error ?? null);
  const rows = useMemo(() => miniToolbarRows(ribbon), [ribbon]);
  // Edit in Formula Bar: the menu closes to the Formula Bar instead of the sheet.
  const toFormulaBar = useRef(false);
  const content = useRef<HTMLDivElement>(null);
  const place = useMenuPlace(menu, content, rows.length);
  const mini =
    !!menu && rows.length > 0 && (menu.kind === 'cell' || menu.kind === 'row' || menu.kind === 'column');
  const toolbar = useMiniToolbar(model, mini);
  if (!menu || menu.kind === 'tab') return null;
  return (
    <Menu
      open
      modal={!mini}
      onOpenChange={(o) => {
        if (o) return;
        model.set({ menu: null });
        model.surface?.focus();
      }}
    >
      <MenuTrigger asChild>
        <span
          aria-hidden="true"
          tabIndex={-1}
          style={{
            position: 'fixed',
            left: place.left ?? menu.x,
            top: place.top ?? menu.y,
            width: 0,
            height: 0,
          }}
        />
      </MenuTrigger>
      <MenuContent
        ref={content}
        // Moved up to fit the window (no room below or above the point): it stays where it fits.
        avoidCollisions={place.top === null}
        updatePositionStrategy="always"
        aria-label={menu.kind === 'error' ? t('Error') : t('Cell')}
        side="bottom"
        align="start"
        sideOffset={2}
        data-testid={menu.kind === 'error' ? 'grid-error-menu' : 'grid-context-menu'}
        data-kiln-shortcuts="off"
        onCloseAutoFocus={(e) => {
          if (!toFormulaBar.current) return;
          toFormulaBar.current = false;
          e.preventDefault();
          editInFormulaBar();
        }}
        onPointerDownOutside={(e) => {
          if (mini && toolbar.ownsOutside(e.target)) e.preventDefault();
        }}
        onFocusOutside={(e) => {
          if (mini && toolbar.ownsOutside(e.target)) e.preventDefault();
        }}
      >
        {menu.kind === 'error' ? (
          error ? (
            <ErrorMenuItems
              error={error}
              model={model}
              onFormulaBar={() => {
                toFormulaBar.current = true;
              }}
            />
          ) : (
            <MenuLabel>{t('No error')}</MenuLabel>
          )
        ) : (
          <CommandMenuItems entries={gridMenuEntries(menu.kind)} />
        )}
      </MenuContent>
      {mini ? (
        <div
          ref={toolbar.ref}
          className="kiln-contextmenu__mini kiln-grid-mini"
          style={{ left: menu.x, top: place.mini ?? Math.max(8, menu.y - 8 - 34 * rows.length) }}
          data-toolbar-skip
          data-testid="grid-mini-toolbar"
          {...toolbar.handlers}
        >
          <Toolbar aria-label={t('Formatting')} className="kiln-minitoolbar__bar kiln-grid-mini__bar">
            {rows.map((row, i) => (
              <div key={i} className="kiln-grid-mini__row">
                {row.map((item) => (
                  <RibbonItemView
                    key={'command' in item ? item.command : item.kind}
                    item={item}
                    size="small"
                    layout="simplified"
                  />
                ))}
              </div>
            ))}
          </Toolbar>
        </div>
      ) : null}
    </Menu>
  );
}
