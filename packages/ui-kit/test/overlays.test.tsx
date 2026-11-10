import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
  Menu,
  MenuCheckboxItem,
  MenuContent,
  MenuItem,
  MenuTrigger,
  Popover,
  PopoverContent,
  PopoverTrigger,
  SplitButton,
  Submenu,
  Tooltip,
} from '../src/index.ts';

describe('Menu / MenuItem / Submenu', () => {
  it('opens from the keyboard, navigates with arrows, runs items with Enter and opens submenus with →', async () => {
    const onCopy = vi.fn();
    const onSpecial = vi.fn();
    render(
      <Menu>
        <MenuTrigger asChild>
          <Button>Edit</Button>
        </MenuTrigger>
        <MenuContent>
          <MenuItem shortcut="Mod+X">Cut</MenuItem>
          <MenuItem shortcut="Mod+C" onSelect={onCopy}>
            Copy
          </MenuItem>
          <Submenu label="Paste Options">
            <MenuItem onSelect={onSpecial}>Paste Special…</MenuItem>
          </Submenu>
        </MenuContent>
      </Menu>,
    );
    screen.getByRole('button', { name: 'Edit' }).focus();
    await userEvent.keyboard('{Enter}');
    const menu = await screen.findByRole('menu');
    expect(menu).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: /Cut/ }).getAttribute('aria-keyshortcuts')).toMatch(/\+X$/);
    await userEvent.keyboard('{ArrowDown}');
    expect(document.activeElement?.textContent).toContain('Copy');
    await userEvent.keyboard('{Enter}');
    expect(onCopy).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).toBeNull();

    screen.getByRole('button', { name: 'Edit' }).focus();
    await userEvent.keyboard('{Enter}');
    await screen.findByRole('menu');
    await userEvent.keyboard('{End}');
    expect(document.activeElement?.textContent).toContain('Paste Options');
    await userEvent.keyboard('{ArrowRight}');
    const special = await screen.findByRole('menuitem', { name: 'Paste Special…' });
    expect(special).toBeTruthy();
    await userEvent.keyboard('{Enter}');
    expect(onSpecial).toHaveBeenCalledTimes(1);
  });

  it('leaves the keys with a dialog when the pointer leaves an item of the menu that opened it', async () => {
    // The ribbon's menus are not modal: a dialog can take the keys while one is still open.
    render(
      <Menu open modal={false}>
        <MenuTrigger asChild>
          <Button>Data</Button>
        </MenuTrigger>
        <MenuContent>
          <MenuItem>Goal Seek…</MenuItem>
        </MenuContent>
      </Menu>,
    );
    const item = await screen.findByRole('menuitem', { name: 'Goal Seek…' });
    // The dialog the item opened took the keys while the menu is still there.
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    const field = document.createElement('input');
    dialog.append(field);
    document.body.append(dialog);
    field.focus();
    fireEvent.pointerLeave(item, { pointerType: 'mouse' });
    const kept = document.activeElement === field;
    dialog.remove();
    expect(kept).toBe(true);
    // With no dialog, leaving an item gives the menu the keys, as Radix does.
    item.focus();
    fireEvent.pointerLeave(item, { pointerType: 'mouse' });
    expect(document.activeElement).toBe(screen.getByRole('menu'));
  });

  it('closes on Escape and returns focus to the trigger; checkbox items toggle', async () => {
    const onChecked = vi.fn();
    render(
      <Menu>
        <MenuTrigger asChild>
          <Button>View</Button>
        </MenuTrigger>
        <MenuContent>
          <MenuCheckboxItem checked={false} onCheckedChange={onChecked}>
            Ruler
          </MenuCheckboxItem>
        </MenuContent>
      </Menu>,
    );
    const trigger = screen.getByRole('button', { name: 'View' });
    await userEvent.click(trigger);
    const item = await screen.findByRole('menuitemcheckbox', { name: 'Ruler' });
    expect(item.getAttribute('aria-checked')).toBe('false');
    await userEvent.click(item);
    expect(onChecked).toHaveBeenCalledWith(true);
    await userEvent.click(trigger);
    await screen.findByRole('menu');
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});

describe('Popover', () => {
  it('opens on click, exposes a dialog and closes with Escape', async () => {
    render(
      <Popover>
        <PopoverTrigger asChild>
          <Button>Table</Button>
        </PopoverTrigger>
        <PopoverContent aria-label="Insert table">
          <button type="button">3×3</button>
        </PopoverContent>
      </Popover>,
    );
    const trigger = screen.getByRole('button', { name: 'Table' });
    await userEvent.click(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(await screen.findByRole('dialog', { name: 'Insert table' })).toBeTruthy();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('Tooltip', () => {
  it('renders title, shortcut and description and is linked with aria-describedby', async () => {
    render(
      <Tooltip content="Bold" shortcut="Ctrl+B" description="Make your text bold." defaultOpen>
        <button type="button">B</button>
      </Tooltip>,
    );
    const tip = await screen.findByRole('tooltip');
    expect(tip.textContent).toContain('Bold');
    expect(tip.textContent).toContain('Ctrl+B');
    expect(tip.textContent).toContain('Make your text bold.');
    expect(screen.getByRole('button', { name: 'B' }).getAttribute('aria-describedby')).toBeTruthy();
  });
});

describe('Dialog', () => {
  it('opened from something that has gone, closes to the document (File ▸ Options from the Backstage)', async () => {
    const back = vi.fn();
    window.addEventListener('kiln:return-to-document', back);
    function Opener() {
      const [from, setFrom] = useState(true);
      const [open, setOpen] = useState(false);
      return (
        <>
          {from ? (
            <div className="kiln-backstage">
              <Button
                onClick={() => {
                  setFrom(false);
                  setOpen(true);
                }}
              >
                Options
              </Button>
            </div>
          ) : null}
          <Dialog open={open} onOpenChange={setOpen}>
            <DialogContent title="Options">
              <input aria-label="Name" />
            </DialogContent>
          </Dialog>
        </>
      );
    }
    render(<Opener />);
    await userEvent.click(screen.getByRole('button', { name: 'Options' }));
    await screen.findByRole('dialog', { name: 'Options' });
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
    await waitFor(() => expect(back).toHaveBeenCalled());
    window.removeEventListener('kiln:return-to-document', back);
  });

  it('is modal, traps focus, closes on Escape and restores focus', async () => {
    render(
      <Dialog>
        <DialogTrigger asChild>
          <Button>Font…</Button>
        </DialogTrigger>
        <DialogContent
          title="Font"
          description="Character formatting."
          footer={
            <DialogClose asChild>
              <Button variant="primary">OK</Button>
            </DialogClose>
          }
        >
          <input aria-label="Size" />
        </DialogContent>
      </Dialog>,
    );
    const trigger = screen.getByRole('button', { name: 'Font…' });
    await userEvent.click(trigger);
    const dialog = await screen.findByRole('dialog', { name: 'Font' });
    expect(dialog.getAttribute('aria-describedby')).toBeTruthy();
    expect(dialog.contains(document.activeElement)).toBe(true);
    for (let i = 0; i < 6; i++) {
      await userEvent.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('closes through the footer button', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent
          title="Confirm"
          footer={
            <DialogClose asChild>
              <Button>Cancel</Button>
            </DialogClose>
          }
        />
      </Dialog>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('a dialog opened while another closes keeps the focus', async () => {
    function Chain() {
      const [open, setOpen] = useState<'first' | 'second' | null>('first');
      return (
        <>
          <input aria-label="Sheet" />
          {open === 'first' ? (
            <Dialog open onOpenChange={() => setOpen(null)}>
              <DialogContent
                title="Protect Sheet"
                // Like the Grid dialogs: closing hands the keys back to the document.
                onCloseAutoFocus={(e) => {
                  e.preventDefault();
                  screen.getByRole('textbox', { name: 'Sheet' }).focus();
                }}
                footer={
                  // The second dialog mounts in the same commit as the first one unmounts.
                  <Button onClick={() => setOpen('second')}>OK</Button>
                }
              >
                <input aria-label="Password" />
              </DialogContent>
            </Dialog>
          ) : null}
          {open === 'second' ? (
            <Dialog open onOpenChange={() => setOpen(null)}>
              <DialogContent
                title="Confirm Password"
                onCloseAutoFocus={(e) => {
                  e.preventDefault();
                  screen.getByRole('textbox', { name: 'Sheet' }).focus();
                }}
              >
                <input aria-label="Reenter password" />
              </DialogContent>
            </Dialog>
          ) : null}
        </>
      );
    }
    render(<Chain />);
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    const second = await screen.findByRole('dialog', { name: 'Confirm Password' });
    // Past the first dialog's close focus (a task after it unmounts).
    await new Promise((r) => setTimeout(r, 50));
    expect(second.contains(document.activeElement)).toBe(true);
    await userEvent.keyboard('{Escape}');
    expect(document.activeElement).toBe(screen.getByRole('textbox', { name: 'Sheet' }));
  });

  it('a dialog opened from inside another returns the focus to its trigger there', async () => {
    render(
      <Dialog defaultOpen>
        <DialogContent title="Format Cells">
          <Dialog>
            <DialogTrigger asChild>
              <Button>Colours…</Button>
            </DialogTrigger>
            <DialogContent title="Colours">
              <input aria-label="Hex" />
            </DialogContent>
          </Dialog>
        </DialogContent>
      </Dialog>,
    );
    const trigger = screen.getByRole('button', { name: 'Colours…' });
    await userEvent.click(trigger);
    await screen.findByRole('dialog', { name: 'Colours' });
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });
});

describe('SplitButton', () => {
  it('runs the main action and opens the menu from the arrow or Alt+↓', async () => {
    const onMain = vi.fn();
    const onSum = vi.fn();
    render(
      <SplitButton label="AutoSum" onClick={onMain}>
        <MenuItem onSelect={onSum}>Sum</MenuItem>
        <MenuItem>Average</MenuItem>
      </SplitButton>,
    );
    const main = screen.getByRole('button', { name: 'AutoSum' });
    await userEvent.click(main);
    expect(onMain).toHaveBeenCalledTimes(1);
    const arrow = screen.getByRole('button', { name: 'More options for AutoSum' });
    expect(arrow.getAttribute('aria-haspopup')).toBe('menu');
    main.focus();
    await userEvent.keyboard('{Alt>}{ArrowDown}{/Alt}');
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Sum' }));
    expect(onSum).toHaveBeenCalled();
  });

  it('hosts free panels in a popover when panel is set', async () => {
    render(
      <SplitButton label="Font Colour" iconOnly panel onClick={() => {}}>
        <button type="button">Red</button>
      </SplitButton>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'More options for Font Colour' }));
    expect(await screen.findByRole('dialog', { name: 'Font Colour' })).toBeTruthy();
  });
});
