/**
 * The sheet's context menus (Excel's): cells, row headers and column headers. The surface asks
 * for one (right-click, Shift+F10, the Menu key) by setting `menu` on the model after selecting
 * what is under the pointer; every item is a command of the window's registry.
 */
import { CommandMenuItems, Menu, MenuContent, type MenuEntry, MenuTrigger } from '@kiln/ui-kit';
import { useGrid } from './hooks.ts';
import type { GridEditorModel, GridMenu } from './model.ts';

const CLIPBOARD: MenuEntry[] = [
  { command: 'suite.edit.cut' },
  { command: 'suite.edit.copy' },
  { kind: 'heading', label: 'Paste Options' },
  { command: 'suite.edit.paste' },
  { command: 'grid.clipboard.paste_values', label: 'Paste Values' },
  { command: 'grid.clipboard.paste_formulas', label: 'Paste Formulas' },
  { command: 'grid.clipboard.paste_formatting', label: 'Paste Formatting' },
  { command: 'grid.clipboard.paste_transpose', label: 'Transpose' },
  { command: 'suite.edit.paste_special', label: 'Paste Special…' },
  { kind: 'separator' },
];

/** The entries of a context menu of the given kind. */
export function gridMenuEntries(kind: GridMenu['kind']): MenuEntry[] {
  switch (kind) {
    case 'row':
      return [
        ...CLIPBOARD,
        { command: 'grid.cells.insert_rows', label: 'Insert' },
        { command: 'grid.cells.delete_rows', label: 'Delete' },
        { command: 'grid.editing.clear_contents', label: 'Clear Contents' },
        { kind: 'separator' },
        { command: 'grid.cells.format_cells', label: 'Format Cells…' },
        { command: 'ui.grid.row_height', label: 'Row Height…' },
        { command: 'grid.cells.hide_rows', label: 'Hide' },
        { command: 'grid.cells.unhide_rows', label: 'Unhide' },
      ];
    case 'column':
      return [
        ...CLIPBOARD,
        { command: 'grid.cells.insert_columns', label: 'Insert' },
        { command: 'grid.cells.delete_columns', label: 'Delete' },
        { command: 'grid.editing.clear_contents', label: 'Clear Contents' },
        { kind: 'separator' },
        { command: 'grid.cells.format_cells', label: 'Format Cells…' },
        { command: 'ui.grid.column_width', label: 'Column Width…' },
        { command: 'grid.cells.hide_columns', label: 'Hide' },
        { command: 'grid.cells.unhide_columns', label: 'Unhide' },
      ];
    default:
      return [
        ...CLIPBOARD,
        { command: 'ui.grid.insert_cells', label: 'Insert…' },
        { command: 'ui.grid.delete_cells', label: 'Delete…' },
        { command: 'grid.editing.clear_contents', label: 'Clear Contents' },
        { kind: 'separator' },
        { command: 'suite.edit.find', label: 'Find…' },
        { command: 'grid.cells.format_cells', label: 'Format Cells…' },
        { command: 'grid.alignment.merge_center', label: 'Merge & Center' },
        { command: 'grid.alignment.wrap_text', label: 'Wrap Text' },
      ];
  }
}

export function GridMenus({ model }: { model: GridEditorModel }) {
  const menu = useGrid(model, (s) => s.menu);
  if (!menu || menu.kind === 'tab') return null;
  return (
    <Menu
      open
      modal
      onOpenChange={(o) => {
        if (!o) {
          model.set({ menu: null });
          model.surface?.focus();
        }
      }}
    >
      <MenuTrigger asChild>
        <span
          aria-hidden="true"
          tabIndex={-1}
          style={{ position: 'fixed', left: menu.x, top: menu.y, width: 0, height: 0 }}
        />
      </MenuTrigger>
      <MenuContent
        aria-label="Cell"
        side="bottom"
        align="start"
        sideOffset={2}
        data-testid="grid-context-menu"
      >
        <CommandMenuItems entries={gridMenuEntries(menu.kind)} />
      </MenuContent>
    </Menu>
  );
}
