import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { Backstage, type BackstageProps } from './Backstage.tsx';

function Host(props: Partial<BackstageProps>) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        File
      </button>
      <Backstage
        open={open}
        onOpenChange={setOpen}
        appName="Kiln Write"
        documentTitle="Report"
        pages={{
          home: <p>Recent files</p>,
          info: <p>Properties</p>,
          print: <p>Print preview</p>,
          saveAs: <p>Formats</p>,
          saveCopy: <p>Copy formats</p>,
        }}
        {...props}
      />
    </>
  );
}

describe('Backstage', () => {
  it('opens full screen from File, lists only provided pages in Office order, and closes with Esc', async () => {
    const user = userEvent.setup();
    renderWithCommands(<Host />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    const dialog = screen.getByRole('dialog', { name: /Kiln Write File menu: Report/ });
    expect(dialog).toBeInTheDocument();
    const nav = screen.getByRole('navigation', { name: 'File menu' });
    const labels = [...nav.querySelectorAll('[data-backstage]')].map((b) => b.textContent);
    expect(labels).toEqual(['Home', 'Info', 'Save', 'Save As', 'Print', 'Close', 'Options']);
    expect(screen.getByRole('heading', { level: 1, name: 'Home' })).toBeInTheDocument();
    expect(screen.getByText('Recent files')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('button', { name: 'File' })).toHaveFocus();
  });

  it('switches pages and marks the current one', async () => {
    const user = userEvent.setup();
    renderWithCommands(<Host />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: 'Print' }));
    expect(screen.getByRole('heading', { level: 1, name: 'Print' })).toBeInTheDocument();
    expect(screen.getByText('Print preview')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Print' })).toHaveAttribute('aria-current', 'page');
  });

  it('shows Save a Copy instead of Save As while AutoSave is on', async () => {
    const user = userEvent.setup();
    renderWithCommands(<Host autoSave />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    expect(screen.getByRole('button', { name: 'Save a Copy' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save As' })).toBeNull();
  });

  it('runs Save / Close / Options through the registry and closes', async () => {
    const user = userEvent.setup();
    const h = createHarness();
    const save = vi.fn();
    h.registry.register({ id: 'file.save', title: 'Save', run: save });
    renderWithCommands(<Host />, h);
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(save).toHaveBeenCalledOnce();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('closes with the back arrow and reports custom actions', async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();
    renderWithCommands(<Host onAction={onAction} />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: 'Options' }));
    expect(onAction).toHaveBeenCalledWith('options');
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: 'Back to the document' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
