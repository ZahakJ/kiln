import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { button, group, ribbon, tab, toggle } from '../ribbon/builders.ts';
import { AppFrame } from './AppFrame.tsx';
import { resetOptionsCache } from './options.ts';
import { createHarness } from './test-helpers.tsx';

// User-event flows open Radix overlays; on a loaded 4-CPU machine they need more than Vitest's 5 s.
vi.setConfig({ testTimeout: 20_000 });

const spec = ribbon('write', [
  tab('home', 'Home', [group('font', 'Font', [toggle('suite.format.bold'), button('suite.format.italic')])]),
  tab('view', 'View', [group('ribbon', 'Ribbon', [toggle('ui.ribbon.collapse', { size: 'medium' })])]),
]);

function setup(extra: Partial<Parameters<typeof AppFrame>[0]> = {}) {
  resetOptionsCache();
  const h = createHarness();
  let bold = false;
  const boldRun = vi.fn(() => {
    bold = !bold;
  });
  h.registry.register({ id: 'suite.format.bold', title: 'Bold', run: boldRun, isChecked: () => bold });
  const onRename = vi.fn();
  const utils = render(
    <AppFrame
      app="write"
      registry={h.registry}
      keymap={h.keymap}
      title="Report"
      onRename={onRename}
      saveStatus="Saved"
      ribbon={spec}
      backstage={{ info: <p>Info page</p> }}
      statusBar={<div>status</div>}
      {...extra}
    >
      <textarea aria-label="Document text" defaultValue="Document" />
    </AppFrame>,
  );
  return { ...utils, ...h, boldRun, onRename };
}

describe('AppFrame', () => {
  it('lays out the title bar, ribbon, document and status bar', () => {
    setup();
    expect(screen.getByRole('img', { name: 'Kiln Write' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Search/ })).toHaveTextContent('Alt+Q');
    expect(screen.getByRole('tablist', { name: 'Ribbon tabs' })).toBeInTheDocument();
    expect(screen.getByRole('main', { name: 'Kiln Write document' })).toContainElement(
      screen.getByRole('textbox', { name: 'Document text' }),
    );
    expect(screen.getByText('status')).toBeInTheDocument();
    expect(screen.getByTestId('save-status')).toHaveTextContent('Saved');
    expect(screen.getByRole('button', { name: 'File' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Undo' })).toHaveAttribute('aria-keyshortcuts', 'Control+Z');
  });

  it('dispatches keyboard shortcuts to implemented commands only', async () => {
    const user = userEvent.setup();
    const { boldRun } = setup();
    screen.getByRole('textbox', { name: 'Document text' }).focus();
    await user.keyboard('{Control>}b{/Control}');
    expect(boldRun).toHaveBeenCalledOnce();
    // Ctrl+I is bound but not implemented: the key falls through untouched.
    const ev = new KeyboardEvent('keydown', { key: 'i', ctrlKey: true, bubbles: true, cancelable: true });
    act(() => void screen.getByRole('textbox', { name: 'Document text' }).dispatchEvent(ev));
    expect(ev.defaultPrevented).toBe(false);
  });

  it('opens the command palette with Ctrl+Shift+P and Alt+Q, and the search box', async () => {
    const user = userEvent.setup();
    setup();
    await user.keyboard('{Control>}{Shift>}p{/Shift}{/Control}');
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await user.keyboard('{Alt>}q{/Alt}');
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    await user.click(screen.getByRole('button', { name: /^Search/ }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });

  it('collapses the ribbon with Ctrl+F1 and remembers it', async () => {
    const user = userEvent.setup();
    const { unmount } = setup();
    expect(screen.getByRole('tabpanel')).toBeInTheDocument();
    await user.keyboard('{Control>}{F1}{/Control}');
    expect(screen.queryByRole('tabpanel')).toBeNull();
    unmount();
    setup();
    expect(screen.queryByRole('tabpanel')).toBeNull();
    await user.keyboard('{Control>}{F1}{/Control}');
    expect(screen.getByRole('tabpanel')).toBeInTheDocument();
  });

  it('shows Key Tips on Alt release and runs Alt, H, 1', async () => {
    const user = userEvent.setup();
    const { boldRun } = setup();
    screen.getByRole('textbox', { name: 'Document text' }).focus();
    await user.keyboard('{Alt}');
    await waitFor(() => expect(screen.getByTestId('keytips')).toBeInTheDocument());
    await user.keyboard('h');
    await act(() => new Promise((r) => requestAnimationFrame(() => r(undefined))));
    await user.keyboard('1');
    expect(boldRun).toHaveBeenCalledOnce();
    expect(screen.queryByTestId('keytips')).toBeNull();
  });

  it('opens the Backstage from File and the Options dialog from the palette', async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole('button', { name: 'File' }));
    expect(screen.getByRole('dialog', { name: /Kiln Write File menu/ })).toHaveTextContent('Info page');
    await user.click(screen.getByRole('button', { name: 'Options' }));
    expect(await screen.findByRole('dialog', { name: 'Kiln Write Options' })).toBeInTheDocument();
  });

  it('renames the document inline', async () => {
    const user = userEvent.setup();
    const { onRename } = setup();
    await user.click(screen.getByRole('button', { name: /Report/ }));
    const input = screen.getByRole('textbox', { name: 'Document name' });
    await user.clear(input);
    await user.type(input, 'Budget{Enter}');
    expect(onRename).toHaveBeenCalledWith('Budget');
  });

  it('toggles AutoSave as a switch', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    setup({ autoSave: { enabled: true, onChange } });
    const sw = screen.getByRole('switch', { name: /AutoSave/ });
    expect(sw).toHaveAttribute('aria-checked', 'true');
    await user.click(sw);
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it('implements the theme and ribbon commands, reflected as checked toggles', async () => {
    const user = userEvent.setup();
    const { registry } = setup();
    await act(() => registry.execute('ui.theme.dark'));
    expect(registry.getState('ui.theme.dark').checked).toBe(true);
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    await act(() => registry.execute('ui.theme.system'));
    await user.click(screen.getByRole('tab', { name: 'View' }));
    const collapse = within(screen.getByRole('tabpanel'))
      .getAllByRole('button', { name: 'Collapse the Ribbon' })
      .find((b) => b.hasAttribute('aria-pressed')) as HTMLElement;
    expect(collapse).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(collapse);
    expect(registry.getState('ui.ribbon.collapse').checked).toBe(true);
  });
});
