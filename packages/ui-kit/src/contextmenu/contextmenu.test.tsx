import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { ContextMenu } from './ContextMenu.tsx';

describe('ContextMenu', () => {
  it('opens at the pointer with command items, shortcuts and state, and runs the choice', async () => {
    const user = userEvent.setup();
    const h = createHarness();
    const copy = vi.fn();
    h.registry.register({ id: 'edit.copy', title: 'Copy', run: copy });
    h.registry.register({ id: 'view.ruler', title: 'Ruler', run: () => {}, isChecked: () => true });
    renderWithCommands(
      <ContextMenu
        entries={[
          { command: 'edit.copy' },
          { command: 'edit.cut' },
          { kind: 'separator' },
          { command: 'view.ruler' },
        ]}
      >
        <div>Document text</div>
      </ContextMenu>,
      h,
    );
    fireEvent.contextMenu(screen.getByText('Document text'), { clientX: 100, clientY: 120 });
    const copyItem = await screen.findByRole('menuitem', { name: /Copy/ });
    expect(copyItem).toHaveTextContent('Ctrl+C');
    expect(screen.getByRole('menuitem', { name: /Cut/ })).toHaveAttribute('aria-disabled', 'true'); // pending
    expect(screen.getByRole('menuitemcheckbox', { name: /Ruler/ })).toHaveAttribute('aria-checked', 'true');
    await user.click(copyItem);
    await waitFor(() => expect(copy).toHaveBeenCalledOnce());
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('opens from the keyboard (Shift+F10) and shows the mini toolbar above it', async () => {
    renderWithCommands(
      <ContextMenu entries={[{ command: 'edit.copy' }]} miniToolbar={<button type="button">B</button>}>
        <button type="button">Focusable text</button>
      </ContextMenu>,
    );
    const target = screen.getByText('Focusable text');
    target.focus();
    fireEvent.keyDown(target, { key: 'F10', shiftKey: true });
    expect(await screen.findByRole('menu')).toBeInTheDocument();
    expect(screen.getByRole('toolbar', { name: 'Formatting' })).toBeInTheDocument();
  });

  it('can leave the browser menu alone', () => {
    renderWithCommands(
      <ContextMenu entries={[{ command: 'edit.copy' }]} onBeforeOpen={() => false}>
        <div>Plain</div>
      </ContextMenu>,
    );
    const ev = fireEvent.contextMenu(screen.getByText('Plain'));
    expect(ev).toBe(true); // not prevented
    expect(screen.queryByRole('menu')).toBeNull();
  });
});
