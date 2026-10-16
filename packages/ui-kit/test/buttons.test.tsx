import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Bold } from 'lucide-react';
import { describe, expect, it, vi } from 'vitest';
import { Button, Icon, IconButton, Separator, Spinner, ToggleButton, VisuallyHidden } from '../src/index.ts';

describe('Button', () => {
  it('renders a native button with variant classes and activates with Enter and Space', async () => {
    const onClick = vi.fn();
    render(
      <Button variant="primary" size="lg" shortcut="Mod+S" onClick={onClick}>
        Save
      </Button>,
    );
    const btn = screen.getByRole('button', { name: 'Save' });
    expect(btn.getAttribute('type')).toBe('button');
    expect(btn.className).toContain('kiln-btn--primary');
    expect(btn.className).toContain('kiln-btn--lg');
    expect(btn.getAttribute('aria-keyshortcuts')).toMatch(/^(Control|Meta)\+S$/);
    btn.focus();
    await userEvent.keyboard('{Enter}');
    await userEvent.keyboard(' ');
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it('does not fire when disabled or busy', async () => {
    const onClick = vi.fn();
    render(
      <>
        <Button disabled onClick={onClick}>
          A
        </Button>
        <Button busy onClick={onClick}>
          B
        </Button>
      </>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'A' }));
    await userEvent.click(screen.getByRole('button', { name: 'B' }));
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'B' }).getAttribute('aria-busy')).toBe('true');
  });

  it('supports the stacked ribbon layout and split variant', () => {
    render(
      <>
        <Button layout="stacked" variant="toolbar" icon={<Icon icon="InsertTable" />}>
          Table
        </Button>
        <Button variant="split">Split</Button>
      </>,
    );
    expect(screen.getByRole('button', { name: 'Table' }).className).toContain('kiln-btn--stacked');
    expect(screen.getByRole('button', { name: 'Split' }).className).toContain('kiln-btn--split');
  });
});

describe('IconButton', () => {
  it('is labelled and shows a tooltip with the shortcut on keyboard focus', async () => {
    render(<IconButton label="Bold" shortcut="Ctrl+B" icon={<Icon icon={Bold} />} />);
    const btn = screen.getByRole('button', { name: 'Bold' });
    await userEvent.tab();
    expect(document.activeElement).toBe(btn);
    const tip = await screen.findByRole('tooltip');
    expect(tip.textContent).toContain('Bold');
    expect(tip.textContent).toContain('Ctrl+B');
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('tooltip')).toBeNull();
  });
});

describe('ToggleButton', () => {
  it('toggles aria-pressed uncontrolled and reports changes', async () => {
    const onChange = vi.fn();
    render(<ToggleButton label="Italic" icon={<Icon icon="TextEffects" />} onPressedChange={onChange} />);
    const btn = screen.getByRole('button', { name: 'Italic' });
    expect(btn.getAttribute('aria-pressed')).toBe('false');
    btn.focus();
    await userEvent.keyboard(' ');
    expect(btn.getAttribute('aria-pressed')).toBe('true');
    expect(onChange).toHaveBeenLastCalledWith(true);
  });

  it('respects the controlled value', async () => {
    const onChange = vi.fn();
    render(
      <ToggleButton pressed={false} onPressedChange={onChange}>
        Ruler
      </ToggleButton>,
    );
    const btn = screen.getByRole('button', { name: 'Ruler' });
    await userEvent.click(btn);
    expect(onChange).toHaveBeenCalledWith(true);
    expect(btn.getAttribute('aria-pressed')).toBe('false');
  });
});

describe('Separator, Spinner, VisuallyHidden', () => {
  it('Separator exposes orientation unless decorative', () => {
    render(
      <>
        <Separator orientation="vertical" data-testid="a" />
        <Separator decorative data-testid="b" />
      </>,
    );
    expect(screen.getByRole('separator').getAttribute('aria-orientation')).toBe('vertical');
    expect(screen.getByTestId('b').getAttribute('role')).toBe('none');
  });

  it('Spinner announces its label as a status', () => {
    render(<Spinner label="Saving" />);
    expect(screen.getByRole('status').textContent).toBe('Saving');
  });

  it('VisuallyHidden keeps text in the accessibility tree', () => {
    render(
      <button type="button">
        <Icon icon="Paste" />
        <VisuallyHidden>Paste</VisuallyHidden>
      </button>,
    );
    expect(screen.getByRole('button', { name: 'Paste' })).toBeTruthy();
  });
});
