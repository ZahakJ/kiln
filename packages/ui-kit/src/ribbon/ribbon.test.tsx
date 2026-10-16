import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { button, combobox, dropdown, group, item, ribbon, row, split, tab, toggle } from './builders.ts';
import { Ribbon } from './Ribbon.tsx';

const spec = ribbon('write', [
  tab('home', 'Home', [
    group('clipboard', 'Clipboard', [
      split('edit.paste', [item('edit.pasteTextOnly')], { size: 'large' }),
      button('edit.cut', { size: 'medium' }),
    ]),
    group(
      'font',
      'Font',
      [
        row(combobox('format.fontFamily', [{ value: 'Calibri' }, { value: 'Arial' }], { width: 120 })),
        row(toggle('format.bold'), toggle('format.italic'), toggle('format.underline')),
      ],
      { rows: 2, launcher: { command: 'format.fontDialog' } },
    ),
  ]),
  tab('insert', 'Insert', [
    group('links', 'Links', [button('insert.link', { size: 'large' })]),
    group('tables', 'Tables', [
      dropdown('insert.table', [item('write.convertTextToTable')], { size: 'large' }),
    ]),
  ]),
  tab('view', 'View', [group('show', 'Show', [toggle('view.ruler', { size: 'medium' })])]),
  tab(
    'tableDesign',
    'Table Design',
    [group('styles', 'Table Styles', [button('format.shading', { size: 'large' })])],
    {
      contextual: { set: 'table', label: 'Table Tools' },
      when: (ctx) => ctx.inTable === true,
    },
  ),
]);

function setup(props: Partial<Parameters<typeof Ribbon>[0]> = {}) {
  const h = createHarness();
  let bold = false;
  const boldRun = vi.fn(() => {
    bold = !bold;
  });
  h.registry.register({ id: 'format.bold', title: 'Bold', run: boldRun, isChecked: () => bold });
  h.registry.register({ id: 'format.italic', title: 'Italic', run: () => {}, isChecked: () => false });
  h.registry.register({ id: 'edit.cut', title: 'Cut', run: () => {} });
  h.registry.register({ id: 'format.fontFamily', title: 'Font', run: vi.fn(), value: () => 'Calibri' });
  h.registry.register({ id: 'insert.link', title: 'Link', run: () => {} });
  const r = renderWithCommands(<Ribbon spec={spec} layout="classic" onFileClick={() => {}} {...props} />, h);
  return { ...r, boldRun };
}

describe('Ribbon', () => {
  it('renders tabs as a WAI-ARIA tablist and the active tab’s commands as a toolbar', () => {
    setup();
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(['Home', 'Insert', 'View']);
    expect(tabs[0]).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', tabs[0]?.id);
    expect(screen.getByRole('toolbar', { name: 'Home commands' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Font' })).toBeInTheDocument();
  });

  it('moves between tabs with the arrow keys, Home and End (automatic activation)', async () => {
    const user = userEvent.setup();
    setup();
    const home = screen.getByRole('tab', { name: 'Home' });
    home.focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Insert' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Insert' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('toolbar', { name: 'Insert commands' })).toBeInTheDocument();
    await user.keyboard('{End}');
    expect(screen.getByRole('tab', { name: 'View' })).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Home' })).toHaveFocus();
    await user.keyboard('{ArrowLeft}');
    expect(screen.getByRole('tab', { name: 'View' })).toHaveFocus();
    await user.keyboard('{Home}');
    expect(screen.getByRole('tab', { name: 'Home' })).toHaveFocus();
  });

  it('enters the commands with ↓, moves with ←/→ and returns to the tab with Esc', async () => {
    const user = userEvent.setup();
    setup();
    screen.getByRole('tab', { name: 'Home' }).focus();
    await user.keyboard('{ArrowDown}');
    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Home commands' })).toContainElement(
        document.activeElement as HTMLElement,
      ),
    );
    const first = document.activeElement;
    expect(first).toBe(screen.getByRole('button', { name: 'Cut' })); // Paste is disabled, so skipped
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('combobox', { name: 'Font' })).toHaveFocus();
    await user.keyboard('{ArrowLeft}{ArrowLeft}'); // the font box keeps ←; Cut was left of it
    (first as HTMLElement).focus();
    await user.keyboard('{ArrowLeft}'); // wraps to the last command
    expect(document.activeElement).not.toBe(first);
    expect(screen.getByRole('toolbar', { name: 'Home commands' })).toContainElement(document.activeElement);
    // The first Esc closes the focused command's ScreenTip, the next returns to the tab.
    await user.keyboard('{Escape}{Escape}');
    expect(screen.getByRole('tab', { name: 'Home' })).toHaveFocus();
  });

  it('runs commands, reflects toggle state and shows the shortcut in the accessible tooltip', async () => {
    const user = userEvent.setup();
    const { boldRun } = setup();
    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(bold).toHaveAttribute('aria-pressed', 'false');
    await user.click(bold);
    expect(boldRun).toHaveBeenCalledOnce();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'true'),
    );
    await user.hover(screen.getByRole('button', { name: 'Italic' }));
    expect(await screen.findByRole('tooltip')).toHaveTextContent('Italic (Ctrl+I)');
  });

  it('marks pending commands disabled (tooltip still explains) and does not run them', async () => {
    const user = userEvent.setup();
    const { registry } = setup();
    const exec = vi.spyOn(registry, 'execute');
    const underline = screen.getByRole('button', { name: 'Underline' });
    expect(underline).toHaveAttribute('aria-disabled', 'true');
    await user.click(underline);
    expect(exec).not.toHaveBeenCalled();
  });

  it('binds combobox values to the command value and runs it with the new value', async () => {
    const { registry } = setup();
    const font = screen.getByRole('combobox', { name: 'Font' });
    expect(font).toHaveValue('Calibri');
    const run = registry.get('format.fontFamily')?.run;
    fireEvent.change(font, { target: { value: 'Arial' } });
    fireEvent.keyDown(font, { key: 'Enter' });
    await waitFor(() => expect(run).toHaveBeenCalledWith(expect.anything(), { value: 'Arial' }));
  });

  it('shows contextual tabs with their set label only while the context applies', () => {
    const { registry } = setup();
    expect(screen.queryByRole('tab', { name: 'Table Design' })).toBeNull();
    act(() => registry.setContext({ inTable: true }));
    const ctxTab = screen.getByRole('tab', { name: 'Table Design' });
    expect(ctxTab.closest('[data-set="table"]')).toHaveTextContent('Table Tools');
    act(() => registry.setContext({ inTable: false }));
    expect(screen.queryByRole('tab', { name: 'Table Design' })).toBeNull();
  });

  it('collapses to tabs only and shows a floating panel when a tab is clicked', async () => {
    const user = userEvent.setup();
    const onCollapsedChange = vi.fn();
    setup({ collapsed: true, onCollapsedChange });
    expect(screen.queryByRole('tabpanel')).toBeNull();
    await user.click(screen.getByRole('tab', { name: 'Insert' }));
    expect(screen.getByRole('tabpanel')).toHaveClass('kiln-ribbon__panel--floating');
    await user.click(screen.getByRole('button', { name: 'Pin the Ribbon' }));
    expect(onCollapsedChange).toHaveBeenCalledWith(false);
  });

  it('renders Key Tip attributes for tabs (Office letters) and commands', () => {
    setup();
    expect(screen.getByRole('tab', { name: 'Home' })).toHaveAttribute('data-keytip', 'H');
    expect(screen.getByRole('tab', { name: 'Insert' })).toHaveAttribute('data-keytip', 'N');
    expect(screen.getByRole('tab', { name: 'View' })).toHaveAttribute('data-keytip', 'W');
    expect(screen.getByRole('button', { name: 'Bold' })).toHaveAttribute('data-keytip', '1');
    expect(screen.getByRole('button', { name: 'Font Settings' })).toHaveAttribute('data-keytip');
  });

  it('switches to the Simplified single-row layout', () => {
    setup({ layout: 'simplified' });
    const toolbar = screen.getByRole('toolbar', { name: 'Home commands' });
    expect(toolbar).toHaveClass('kiln-ribbon__groups--simplified');
    // Large commands keep a label; the rest are icon-only.
    expect(screen.getAllByText('Paste').length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'Cut' })).toHaveClass('kiln-btn--icon-only');
  });

  it('opens the display options menu to change layout and collapse', async () => {
    const user = userEvent.setup();
    const onLayoutChange = vi.fn();
    const onCollapsedChange = vi.fn();
    setup({ onLayoutChange, onCollapsedChange });
    await user.click(screen.getByRole('button', { name: 'Ribbon display options' }));
    await user.click(await screen.findByRole('menuitem', { name: /Simplified ribbon/ }));
    expect(onLayoutChange).toHaveBeenCalledWith('simplified');
    await user.click(screen.getByRole('button', { name: 'Ribbon display options' }));
    await user.click(await screen.findByRole('menuitem', { name: /Show tabs only/ }));
    expect(onCollapsedChange).toHaveBeenCalledWith(true);
  });

  it('opens the File menu from the File tab', async () => {
    const user = userEvent.setup();
    const onFileClick = vi.fn();
    setup({ onFileClick });
    await user.click(screen.getByRole('button', { name: 'File' }));
    expect(onFileClick).toHaveBeenCalledOnce();
  });
});
