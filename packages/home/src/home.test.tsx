import type { RecentFile } from '@kiln/protocol';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { greeting } from './greeting.ts';
import { KilnHome, type KilnHomeProps } from './KilnHome.tsx';
import { appKind, kindApp, relativeTime } from './model.ts';

const NOW = new Date(2026, 9, 16, 19, 30);
const ago = (ms: number) => NOW.getTime() - ms;

const FILES: RecentFile[] = [
  {
    path: '/Studio/Co-op/Autumn firing plan.docx',
    name: 'Autumn firing plan.docx',
    kind: 'Write',
    opened_at_ms: ago(10 * 60_000),
    pinned: true,
  },
  {
    path: '/Studio/Accounts/Studio budget 2026.xlsx',
    name: 'Studio budget 2026.xlsx',
    kind: 'Grid',
    opened_at_ms: ago(3_600_000),
    pinned: false,
  },
  {
    path: '/Desktop/Open studio weekend.pptx',
    name: 'Open studio weekend.pptx',
    kind: 'Deck',
    opened_at_ms: ago(26 * 3_600_000),
    pinned: false,
  },
];

function setup(props: Partial<KilnHomeProps> = {}) {
  const handlers = {
    onCreate: vi.fn(),
    onOpenFile: vi.fn(),
    onOpenRecent: vi.fn(),
    onTogglePin: vi.fn(),
    onOpenSettings: vi.fn(),
  };
  render(<KilnHome recent={FILES} now={NOW} creatable={['write', 'grid']} {...handlers} {...props} />);
  return handlers;
}

describe('Kiln Home', () => {
  it('greets, titles the page and shows the four app doors', () => {
    setup({ userName: 'Muthana', version: '0.1.0' });
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(
      'Kiln Home. What are we making today?',
    );
    expect(screen.getByText('Good evening, Muthana. The kiln is warm.')).toBeInTheDocument();
    for (const name of [/^New document in Kiln Write:/, /^New workbook in Kiln Grid:/]) {
      expect(screen.getByRole('button', { name })).toBeEnabled();
    }
    expect(screen.getByRole('button', { name: /^New deck in Kiln Deck \(not/ })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByTestId('suite-version')).toHaveTextContent('Version 0.1.0');
  });

  it('creates from a door and from the New menu, only for apps the core can create', async () => {
    const user = userEvent.setup();
    const h = setup();
    await user.click(screen.getByRole('button', { name: /^New workbook in Kiln Grid:/ }));
    expect(h.onCreate).toHaveBeenCalledWith('grid');
    await user.click(screen.getByRole('button', { name: /^New deck in Kiln Deck \(not/ }));
    expect(h.onCreate).toHaveBeenCalledTimes(1);
    await user.click(document.querySelector('.kiln-home__new-btn') as HTMLElement);
    const menu = screen.getByRole('menu');
    expect(within(menu).getByRole('menuitem', { name: /Blank deck/ })).toBeDisabled();
    await user.click(within(menu).getByRole('menuitem', { name: /Blank document/ }));
    expect(h.onCreate).toHaveBeenLastCalledWith('write');
  });

  it('lists recent files with dates, opens and pins them', async () => {
    const user = userEvent.setup();
    const h = setup();
    const table = screen.getByRole('table');
    expect(within(table).getAllByRole('row')).toHaveLength(4);
    expect(within(table).getByText('10 minutes ago')).toBeInTheDocument();
    await user.click(within(table).getByRole('button', { name: /^Studio budget 2026/ }));
    expect(h.onOpenRecent).toHaveBeenCalledWith(FILES[1]);
    await user.click(within(table).getByRole('button', { name: 'Unpin Autumn firing plan.docx' }));
    expect(h.onTogglePin).toHaveBeenCalledWith(FILES[0]);
  });

  it('switches between Recent, Pinned and Shared with me', async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole('tab', { name: 'Pinned' }));
    expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(2);
    await user.click(screen.getByRole('tab', { name: 'Shared with me' }));
    expect(screen.getByText('Nothing shared yet')).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole('tab', { name: 'Shared with me' }), { key: 'ArrowRight' });
    expect(screen.getByRole('tab', { name: 'Recent' })).toHaveAttribute('aria-selected', 'true');
  });

  it('filters by app and by search', async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole('button', { name: 'Grid', pressed: false }));
    expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(2);
    await user.click(screen.getByRole('button', { name: /Grid only/ }));
    await user.type(screen.getByRole('searchbox'), 'weekend');
    expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(2);
    await user.clear(screen.getByRole('searchbox'));
    await user.type(screen.getByRole('searchbox'), 'zzz');
    expect(screen.getByText('No matches')).toBeInTheDocument();
  });

  it('shows the loading and empty states, and Ctrl+K / Ctrl+O', async () => {
    const user = userEvent.setup();
    const h = setup({ recent: null });
    expect(screen.getByRole('status')).toHaveTextContent('Looking for your files');
    await user.keyboard('{Control>}k{/Control}');
    expect(screen.getByRole('searchbox')).toHaveFocus();
    await user.keyboard('{Control>}o{/Control}');
    expect(h.onOpenFile).toHaveBeenCalled();
  });

  it('dismisses the Office banner and shows storage use', async () => {
    const user = userEvent.setup();
    const onDismissSwitcher = vi.fn();
    setup({ onDismissSwitcher, storage: { used: 7.6e9, quota: 64e9 } });
    expect(screen.getByText('Coming from Microsoft Office?')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(onDismissSwitcher).toHaveBeenCalled();
    expect(screen.getByText('7.6 GB of 64 GB')).toBeInTheDocument();
    expect(screen.getByRole('meter', { name: 'Storage used' })).toHaveAttribute('aria-valuenow', '12');
  });
});

describe('Home model', () => {
  it('formats relative times', () => {
    expect(relativeTime(ago(20_000), NOW)).toBe('Just now');
    expect(relativeTime(ago(60_000), NOW)).toBe('1 minute ago');
    expect(relativeTime(ago(2 * 3_600_000), NOW)).toBe('2 hours ago');
    expect(relativeTime(ago(26 * 3_600_000), NOW)).toMatch(/^Yesterday, \d\d:\d\d$/);
    expect(relativeTime(0, NOW)).toBe('');
  });

  it('maps document kinds and apps both ways', () => {
    expect(kindApp('Grid')).toBe('grid');
    expect(kindApp('Scratch')).toBe('write');
    expect(appKind('deck')).toBe('Deck');
  });

  it('greets by the time of day', () => {
    expect(greeting(new Date(2026, 0, 1, 9), null)).toBe('Good morning. The kiln is warm.');
    expect(greeting(new Date(2026, 0, 1, 14), 'Rana')).toBe('Good afternoon, Rana. The kiln is warm.');
  });
});
