import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { StatusBar } from './StatusBar.tsx';

function Host({ onView = vi.fn() }: { onView?: (id: string) => void }) {
  const [zoom, setZoom] = useState(100);
  const [view, setView] = useState('print');
  return (
    <StatusBar
      left={[
        { id: 'page', label: 'Page Number', content: 'Page 3 of 12' },
        { id: 'words', label: 'Word Count', content: '1,284 words', onClick: () => {} },
        { id: 'lang', label: 'Language', content: 'English (US)', defaultHidden: true },
      ]}
      views={{
        value: view,
        onChange: (v) => {
          setView(v);
          onView(v);
        },
        options: [
          { id: 'read', label: 'Read Mode', icon: 'ReadMode' },
          { id: 'print', label: 'Print Layout', icon: 'PrintLayout' },
          { id: 'web', label: 'Web Layout', icon: 'WebLayout' },
        ],
      }}
      zoom={{
        value: zoom,
        onChange: setZoom,
        fits: [{ id: 'width', label: 'Page width', onSelect: () => setZoom(130) }],
      }}
      storageKey="test.statusbar"
    />
  );
}

describe('StatusBar', () => {
  it('shows items, hides default-hidden ones, and exposes the view switcher as radios', async () => {
    const user = userEvent.setup();
    const onView = vi.fn();
    render(<Host onView={onView} />);
    expect(screen.getByText('Page 3 of 12')).toBeInTheDocument();
    expect(screen.queryByText('English (US)')).toBeNull();
    const print = screen.getByRole('radio', { name: 'Print Layout' });
    expect(print).toHaveAttribute('aria-checked', 'true');
    print.focus();
    await user.keyboard('{ArrowRight}');
    expect(onView).toHaveBeenCalledWith('web');
    expect(screen.getByRole('radio', { name: 'Web Layout' })).toHaveAttribute('aria-checked', 'true');
  });

  it('zooms with − and + in Office steps', async () => {
    const user = userEvent.setup();
    render(<Host />);
    await user.click(screen.getByRole('button', { name: 'Zoom in' }));
    expect(screen.getByRole('button', { name: 'Zoom level 110%' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Zoom out' }));
    await user.click(screen.getByRole('button', { name: 'Zoom out' }));
    expect(screen.getByRole('button', { name: 'Zoom level 90%' })).toBeInTheDocument();
  });

  it('opens the zoom popover with presets, fits and a custom percentage', async () => {
    const user = userEvent.setup();
    render(<Host />);
    await user.click(screen.getByRole('button', { name: 'Zoom level 100%' }));
    await user.click(await screen.findByRole('button', { name: '200%' }));
    expect(screen.getByRole('button', { name: 'Zoom level 200%' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Zoom level 200%' }));
    await user.click(await screen.findByRole('button', { name: 'Page width' }));
    expect(screen.getByRole('button', { name: 'Zoom level 130%' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Zoom level 130%' }));
    const field = await screen.findByRole('spinbutton', { name: 'Percent' });
    await user.clear(field);
    await user.type(field, '65{Enter}');
    expect(screen.getByRole('button', { name: 'Zoom level 65%' })).toBeInTheDocument();
  });

  it('customises visible items from the right-click menu and remembers the choice', async () => {
    const user = userEvent.setup();
    const { unmount } = render(<Host />);
    fireEvent.contextMenu(screen.getByText('Page 3 of 12'));
    await user.click(await screen.findByRole('menuitemcheckbox', { name: 'Language' }));
    expect(screen.getByText('English (US)')).toBeInTheDocument();
    await user.click(screen.getByRole('menuitemcheckbox', { name: 'Page Number' }));
    expect(screen.queryByText('Page 3 of 12')).toBeNull();
    unmount();
    render(<Host />);
    expect(screen.queryByText('Page 3 of 12')).toBeNull();
    expect(screen.getByText('English (US)')).toBeInTheDocument();
  });
});
