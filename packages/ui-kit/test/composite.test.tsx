import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import {
  applyTint,
  Button,
  ColorPicker,
  Gallery,
  type GalleryItem,
  getRecentColors,
  Icon,
  IconButton,
  ScrollArea,
  ToastProvider,
  ToggleButton,
  Toolbar,
  themeVariants,
  useToast,
} from '../src/index.ts';
import { resetRecentColorsCache } from '../src/primitives/color.ts';

describe('Toolbar', () => {
  it('is a single tab stop with roving focus, Home/End and wrap-around', async () => {
    render(
      <>
        <button type="button">before</button>
        <Toolbar aria-label="Font">
          <IconButton label="Bold" icon={<Icon icon="FontColor" />} tooltip={false} />
          <IconButton label="Italic" icon={<Icon icon="FontColor" />} tooltip={false} />
          <IconButton label="Disabled" icon={<Icon icon="FontColor" />} tooltip={false} disabled />
          <ToggleButton>Underline</ToggleButton>
        </Toolbar>
        <button type="button">after</button>
      </>,
    );
    expect(screen.getByRole('toolbar', { name: 'Font' }).getAttribute('aria-orientation')).toBe('horizontal');
    screen.getByRole('button', { name: 'before' }).focus();
    await userEvent.tab();
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Bold');
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Italic');
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement?.textContent).toBe('Underline');
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Bold');
    await userEvent.keyboard('{End}');
    expect(document.activeElement?.textContent).toBe('Underline');
    await userEvent.tab();
    expect(document.activeElement?.textContent).toBe('after');
    await userEvent.tab({ shift: true });
    expect(document.activeElement?.textContent).toBe('Underline');
  });

  it('leaves arrow keys to text fields', async () => {
    render(
      <Toolbar aria-label="Find">
        <input aria-label="Search" defaultValue="abc" />
        <Button>Go</Button>
      </Toolbar>,
    );
    const input = screen.getByRole('textbox', { name: 'Search' });
    input.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(document.activeElement).toBe(input);
  });
});

const ITEMS: GalleryItem[] = Array.from({ length: 12 }, (_, i) => ({ id: `s${i}`, label: `Style ${i}` }));

describe('Gallery', () => {
  it('shows one row, pages rows, and selects with Enter', async () => {
    const onSelect = vi.fn();
    const onPreview = vi.fn();
    render(
      <Gallery
        label="Styles"
        items={ITEMS}
        value="s0"
        onSelect={onSelect}
        onPreview={onPreview}
        columns={5}
      />,
    );
    const row = screen.getByRole('listbox', { name: 'Styles' });
    expect(row.querySelectorAll('[role="option"]')).toHaveLength(5);
    expect(screen.getByRole('option', { name: 'Style 0' }).getAttribute('aria-selected')).toBe('true');
    await userEvent.click(screen.getByRole('button', { name: 'Next row of Styles' }));
    expect(screen.getByRole('option', { name: 'Style 5' })).toBeTruthy();
    const item = screen.getByRole('option', { name: 'Style 6' });
    act(() => item.focus());
    expect(onPreview).toHaveBeenLastCalledWith('s6');
    fireEvent.keyDown(item, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('s6');
  });

  it('expands into a full grid panel', async () => {
    const onSelect = vi.fn();
    render(
      <Gallery
        label="Styles"
        items={ITEMS}
        onSelect={onSelect}
        columns={5}
        footer={<button type="button">Create a Style</button>}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'More Styles' }));
    const lists = await screen.findAllByRole('listbox', { name: 'Styles' });
    const panel = lists[lists.length - 1] as HTMLElement;
    expect(panel.querySelectorAll('[role="option"]')).toHaveLength(12);
    expect(screen.getByRole('button', { name: 'Create a Style' })).toBeTruthy();
    await userEvent.click(panel.querySelector('[data-index="11"]') as HTMLElement);
    expect(onSelect).toHaveBeenCalledWith('s11');
  });
});

describe('ColorPicker', () => {
  it('renders the 10 × 6 theme grid, standard colours, and Automatic', () => {
    render(<ColorPicker onChange={() => {}} allowAuto />);
    expect(screen.getByRole('group', { name: 'Theme Colors' }).querySelectorAll('button')).toHaveLength(60);
    expect(screen.getByRole('group', { name: 'Standard Colors' }).querySelectorAll('button')).toHaveLength(
      10,
    );
    expect(screen.getByRole('button', { name: 'Blue, Accent 1, Lighter 40%' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Automatic' })).toBeTruthy();
  });

  it('navigates swatches with arrow keys and applies theme colours with tint', async () => {
    const onChange = vi.fn();
    render(<ColorPicker onChange={onChange} />);
    const first = screen.getByRole('button', { name: 'White, Background 1' });
    expect(first.tabIndex).toBe(0);
    first.focus();
    await userEvent.keyboard('{ArrowRight}{ArrowRight}{ArrowRight}{ArrowRight}{ArrowDown}');
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Blue, Accent 1, Lighter 80%');
    await userEvent.keyboard('{Enter}');
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'theme', slot: 'accent1', tint: 0.8 }),
    );
  });

  it('adds custom colours to the shared recent list', async () => {
    resetRecentColorsCache();
    const onChange = vi.fn();
    render(<ColorPicker onChange={onChange} value={{ type: 'rgb', hex: '#123456' }} />);
    await userEvent.click(screen.getByRole('button', { name: 'More Colors…' }));
    await userEvent.click(screen.getByRole('button', { name: 'OK' }));
    expect(onChange).toHaveBeenCalledWith({ type: 'rgb', hex: '#123456' });
    expect(getRecentColors()[0]).toBe('#123456');
    expect(await screen.findByRole('group', { name: 'Recent Colors' })).toBeTruthy();
  });

  it('computes Office tint/shade rows', () => {
    expect(themeVariants('#FFFFFF')).toEqual([-0.05, -0.15, -0.25, -0.35, -0.5]);
    expect(themeVariants('#000000')).toEqual([0.5, 0.35, 0.25, 0.15, 0.05]);
    expect(themeVariants('#4472C4')).toEqual([0.8, 0.6, 0.4, -0.25, -0.5]);
    expect(applyTint('#FFFFFF', -0.5)).toBe('#808080');
    expect(applyTint('#000000', 0.5)).toBe('#808080');
  });
});

function Notifier() {
  const { toast } = useToast();
  return (
    <>
      <button type="button" onClick={() => toast({ title: 'Saved', tone: 'success', duration: 1000 })}>
        save
      </button>
      <button
        type="button"
        onClick={() =>
          toast({ title: 'Failed', tone: 'danger', action: { label: 'Retry', onClick: () => {} } })
        }
      >
        fail
      </button>
    </>
  );
}

describe('Toast', () => {
  it('announces politely, auto-dismisses, and errors use role=alert until dismissed', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(
      <ToastProvider>
        <Notifier />
      </ToastProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'save' }));
    expect(screen.getByRole('status').textContent).toContain('Saved');
    fireEvent.click(screen.getByRole('button', { name: 'fail' }));
    const alert = screen.getByRole('alert');
    expect(alert.textContent).toContain('Failed');
    act(() => {
      vi.advanceTimersByTime(1100);
    });
    expect(screen.queryByText('Saved')).toBeNull();
    expect(screen.getByRole('alert')).toBeTruthy();
    fireEvent.keyDown(alert, { key: 'Escape' });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('throws a helpful error outside the provider', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(() => render(<Notifier />)).toThrow(/ToastProvider/);
    spy.mockRestore();
  });
});

describe('ScrollArea', () => {
  it('is a focusable labelled region when labelled and marks hidden overflow edges', () => {
    render(
      <ScrollArea aria-label="Recent" maxHeight={50}>
        <div style={{ height: 200 }}>content</div>
      </ScrollArea>,
    );
    const region = screen.getByRole('region', { name: 'Recent' });
    expect(region.tabIndex).toBe(0);
    Object.defineProperty(region, 'scrollHeight', { value: 200, configurable: true });
    Object.defineProperty(region, 'clientHeight', { value: 50, configurable: true });
    Object.defineProperty(region, 'scrollTop', { value: 20, configurable: true, writable: true });
    fireEvent.scroll(region);
    expect(region.dataset.scrollTop).toBe('true');
    expect(region.dataset.scrollBottom).toBe('true');
  });
});
