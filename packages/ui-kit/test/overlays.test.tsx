import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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
