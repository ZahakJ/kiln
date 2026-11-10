import type { RecentFile } from '@kiln/protocol';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { greeting } from './greeting.ts';
import { KilnHome, type KilnHomeProps } from './KilnHome.tsx';
import { appKind, kindApp, relativeTime } from './model.ts';
import type { HomeTemplate } from './templates/TemplateGallery.tsx';

const NOW = new Date(2026, 9, 16, 19, 30);
const ago = (ms: number) => NOW.getTime() - ms;

const FILES: RecentFile[] = [
  {
    path: '/Studio/Co-op/Autumn firing plan.docx',
    name: 'Autumn firing plan.docx',
    kind: 'Write',
    opened_at_ms: ago(10 * 60_000),
    pinned: true,
    thumbnail: null,
  },
  {
    path: '/Studio/Accounts/Studio budget 2026.xlsx',
    name: 'Studio budget 2026.xlsx',
    kind: 'Grid',
    opened_at_ms: ago(3_600_000),
    pinned: false,
    thumbnail: null,
  },
  {
    path: '/Desktop/Open studio weekend.pptx',
    name: 'Open studio weekend.pptx',
    kind: 'Deck',
    opened_at_ms: ago(26 * 3_600_000),
    pinned: false,
    thumbnail: null,
  },
];

const TEMPLATES: HomeTemplate[] = [
  {
    id: 'write-letter',
    app: 'write',
    name: 'Business letter',
    file: 'write/letter.kwrite',
    description: 'A one-page letter with a letterhead',
    featured: true,
  },
  {
    id: 'write-resume',
    app: 'write',
    name: 'Résumé',
    file: 'write/resume.kwrite',
    description: 'Two columns',
  },
  {
    id: 'grid-budget',
    app: 'grid',
    name: 'Monthly budget',
    file: 'grid/budget.kgrid',
    description: 'Income, spending and savings',
    featured: true,
  },
  {
    id: 'grid-loan',
    app: 'grid',
    name: 'Loan calculator',
    file: 'grid/loan.kgrid',
    description: 'Repayments',
  },
  {
    id: 'deck-pitch',
    app: 'deck',
    name: 'Pitch',
    file: 'deck/pitch.kdeck',
    description: 'Ten slides for investors',
    featured: true,
  },
  { id: 'notes-log', app: 'notes', name: 'Daily log', file: 'notes/log.kwrite', featured: true },
];

const SHARED: RecentFile[] = [
  {
    path: '#/write?doc=k7p2q9xw&server=wss%3A%2F%2Fcollab.example.org&token=t',
    name: 'Kiln roadmap',
    kind: 'Write',
    opened_at_ms: ago(5 * 60_000),
    pinned: false,
    thumbnail: null,
  },
];

function setup(props: Partial<KilnHomeProps> = {}) {
  const handlers = {
    onCreate: vi.fn(),
    onOpenFile: vi.fn(),
    onOpenRecent: vi.fn(),
    onTogglePin: vi.fn(),
    onOpenSettings: vi.fn(),
    onOpenTemplate: vi.fn(),
    onOpenShared: vi.fn(),
  };
  render(<KilnHome recent={FILES} now={NOW} creatable={['write', 'grid']} {...handlers} {...props} />);
  return handlers;
}

describe('Kiln Home', () => {
  it('greets, titles the page and shows a door per app of the release', () => {
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

describe('Kiln Home', () => {
  it('shows no trace of Kiln Notes: three app doors, the New menu and the filters', async () => {
    const user = userEvent.setup();
    setup({ templates: TEMPLATES });
    expect(
      document.querySelectorAll(
        '.kiln-door[data-app="write"], .kiln-door[data-app="grid"], .kiln-door[data-app="deck"]',
      ),
    ).toHaveLength(3);
    expect(document.querySelector('.kiln-door[data-app="notes"]')).toBeNull();
    await user.click(document.querySelector('.kiln-home__new-btn') as HTMLElement);
    expect(within(screen.getByRole('menu')).getAllByRole('menuitem')).toHaveLength(3);
    expect(document.body.textContent).not.toMatch(/Kiln Notes|Notebook|Daily log/);
  });

  it('features templates in the strip and searches every template', async () => {
    const user = userEvent.setup();
    const h = setup({ templates: TEMPLATES });
    const strip = screen.getByRole('region', { name: 'Fresh from the kiln' });
    expect(
      within(strip).getAllByRole('button', { name: /Business letter|Monthly budget|Pitch/ }),
    ).toHaveLength(3);
    await user.type(screen.getByRole('searchbox'), 'loan');
    const found = screen.getByRole('region', { name: 'Templates' });
    await user.click(within(found).getByRole('button', { name: /Loan calculator/ }));
    expect(h.onOpenTemplate).toHaveBeenCalledWith(TEMPLATES[3]);
  });

  it('opens the template gallery from the templates door, filters it and starts blank or from a template', async () => {
    const user = userEvent.setup();
    const h = setup({ templates: TEMPLATES, creatable: ['write', 'grid', 'deck'] });
    await user.click(screen.getByRole('button', { name: /^Browse templates:/ }));
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Start from a template');
    expect(screen.getByText('5 templates')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Workbooks/, pressed: false }));
    expect(screen.getByText('2 templates')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Blank workbook/ }));
    expect(h.onCreate).toHaveBeenCalledWith('grid');
    await user.click(screen.getByRole('button', { name: 'All', pressed: false }));
    await user.type(screen.getByRole('searchbox', { name: 'Search templates' }), 'zzz');
    expect(screen.getByText('No templates match “zzz”')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show every template' }));
    await user.click(screen.getByRole('button', { name: /Résumé/ }));
    expect(h.onOpenTemplate).toHaveBeenCalledWith(TEMPLATES[1]);
    await user.click(screen.getByRole('button', { name: 'Back to Home' }));
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('What are we making today?');
  });

  it('moves between template cards and files with the arrow keys', async () => {
    const user = userEvent.setup();
    setup({ templates: TEMPLATES, initialView: 'templates' });
    const cards = [...document.querySelectorAll<HTMLElement>('.kiln-tcard')];
    cards[0]?.focus();
    await user.keyboard('{ArrowRight}');
    expect(document.activeElement).toBe(cards[1]);
    await user.keyboard('{End}');
    expect(document.activeElement).toBe(cards.at(-1));
    await user.keyboard('{Home}');
    expect(document.activeElement).toBe(cards[0]);
    await user.click(screen.getByRole('button', { name: 'Back to Home' }));
    // The view's heading takes the focus on the next frame.
    await new Promise((r) => requestAnimationFrame(r));
    const files = [...document.querySelectorAll<HTMLElement>('.kiln-home__file')];
    files[0]?.focus();
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(files[1]);
    await user.keyboard('{ArrowUp}');
    expect(document.activeElement).toBe(files[0]);
  });

  it('lists the documents shared with the user and opens them by their link', async () => {
    const user = userEvent.setup();
    const h = setup({ shared: SHARED });
    await user.click(screen.getByRole('tab', { name: 'Shared with me' }));
    const table = screen.getByRole('table');
    expect(within(table).getAllByText('Shared through collab.example.org').length).toBeGreaterThan(0);
    expect(within(table).queryByRole('button', { name: /^Pin / })).toBeNull();
    await user.click(within(table).getByRole('button', { name: /^Kiln roadmap/ }));
    expect(h.onOpenShared).toHaveBeenCalledWith(SHARED[0]);
  });

  it('offers a way forward from the empty states', async () => {
    const user = userEvent.setup();
    const h = setup({ recent: [], templates: TEMPLATES });
    const empty = screen.getByRole('status');
    expect(empty).toHaveTextContent('Your recent files will gather here');
    await user.click(within(empty).getByRole('button', { name: 'Open file' }));
    expect(h.onOpenFile).toHaveBeenCalled();
    await user.click(within(empty).getByRole('button', { name: 'Browse templates' }));
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Start from a template');
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
