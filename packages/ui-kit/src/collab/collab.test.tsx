import type { CollabPeer, CollabState, VersionEntry } from '@kiln/protocol';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { PresenceAvatars, peerInk } from './PresenceAvatars.tsx';
import { RemoteFlags } from './RemoteFlags.tsx';
import { JoinDialog, ShareDialog } from './ShareDialog.tsx';
import { VersionBar, VersionHistoryPane, when } from './VersionHistoryPane.tsx';

vi.setConfig({ testTimeout: 20_000 });

const rana: CollabPeer = {
  peer: '42',
  name: 'Rana Okafor',
  initials: 'RO',
  colour: '1D8A60',
  caret: { shape: null, para: 3, offset: 4 },
  anchor: null,
  slide: null,
  shapes: [],
  at: { x: 10, y: 20, width: 1, height: 16 },
};

const shared: CollabState = {
  shared: true,
  status: 'Online',
  link: { server: 'ws://relay', room: 'k7p2q9xw', token: 't' },
  name: 'Ana Martín',
  initials: 'AM',
  colour: '3159C6',
  peers: [rana],
  queued: 0,
  preview: null,
  problem: null,
};

const NOW = Date.UTC(2026, 9, 23, 15, 0);
const versions: VersionEntry[] = [
  { id: 'v1', name: null, author: 'Ana Martín', time_ms: NOW - 3_600_000, origin: 'Shared' },
  { id: 'v2', name: 'Before the edits', author: 'Rana Okafor', time_ms: NOW - 600_000, origin: 'Save' },
];

describe('PresenceAvatars', () => {
  it('shows each person in their colour and goes to them', async () => {
    const user = userEvent.setup();
    const onGoTo = vi.fn();
    const onOpenShare = vi.fn();
    render(
      <PresenceAvatars
        peers={[rana]}
        me={{ name: 'Ana Martín', initials: 'AM', colour: '3159C6' }}
        status="Online"
        onGoTo={onGoTo}
        onOpenShare={onOpenShare}
      />,
    );
    const avatar = screen.getByRole('button', { name: 'RO: Rana Okafor, editing' });
    expect(avatar).toHaveTextContent('RO');
    expect(avatar.getAttribute('style')).toContain('#1D8A60');
    await user.click(avatar);
    expect(onGoTo).toHaveBeenCalledWith(rana);
    await user.click(screen.getByRole('button', { name: /AM: Ana Martín \(you\)/ }));
    expect(onOpenShare).toHaveBeenCalled();
    expect(screen.queryByRole('img', { name: /Offline/ })).not.toBeInTheDocument();
  });

  it('says when it is offline and folds many people into +n', () => {
    const many = Array.from({ length: 5 }, (_, i) => ({ ...rana, peer: String(i), name: `P${i}` }));
    render(
      <PresenceAvatars peers={many} me={{ name: '', initials: '', colour: '' }} status="Offline" max={3} />,
    );
    expect(screen.getByRole('img', { name: /Offline/ })).toBeInTheDocument();
    expect(screen.getByRole('img', { name: '2 more' })).toHaveTextContent('+2');
  });
});

describe('RemoteFlags', () => {
  it('puts each name at its caret', () => {
    render(<RemoteFlags placed={[{ peer: rana, rect: { left: 40, top: 60, width: 1, height: 16 } }]} />);
    const flag = screen.getByText('Rana Okafor');
    expect(flag.style.left).toBe('40px');
    expect(flag.style.top).toBe('60px');
  });
});

describe('ShareDialog', () => {
  const defaults = { server: '', token: '', name: 'Ana Martín' };

  it('shares with the server, token and name given', async () => {
    const user = userEvent.setup();
    const onShare = vi.fn().mockResolvedValue(undefined);
    render(
      <ShareDialog
        open
        onOpenChange={() => {}}
        title="Autumn firing plan"
        state={null}
        defaults={defaults}
        link={null}
        onShare={onShare}
        onStop={() => {}}
        onCopy={async () => {}}
      />,
    );
    const dialog = screen.getByRole('dialog', { name: 'Share “Autumn firing plan”' });
    const start = within(dialog).getByRole('button', { name: 'Share' });
    expect((start as HTMLButtonElement).disabled).toBe(true);
    await user.type(within(dialog).getByLabelText('Collaboration server'), 'ws://127.0.0.1:8640');
    await user.type(within(dialog).getByLabelText('Access token'), 's3cret');
    await user.click(start);
    expect(onShare).toHaveBeenCalledWith({
      server: 'ws://127.0.0.1:8640',
      token: 's3cret',
      name: 'Ana Martín',
    });
  });

  it('shows the link, who is here and stops sharing', async () => {
    const user = userEvent.setup();
    const onCopy = vi.fn().mockResolvedValue(undefined);
    const onStop = vi.fn();
    render(
      <ShareDialog
        open
        onOpenChange={() => {}}
        title="Plan"
        state={shared}
        defaults={defaults}
        link="https://kiln.example/#/write?doc=k7p2q9xw&server=ws%3A%2F%2Frelay&token=t"
        onShare={async () => {}}
        onStop={onStop}
        onCopy={onCopy}
      />,
    );
    expect(screen.getByTestId('share-link')).toHaveValue(
      'https://kiln.example/#/write?doc=k7p2q9xw&server=ws%3A%2F%2Frelay&token=t',
    );
    expect(screen.getByRole('status')).toHaveTextContent('Connected. 1 other person is here.');
    expect(screen.getByRole('list', { name: 'People in this document' })).toHaveTextContent('Rana Okafor');
    await user.click(screen.getByRole('button', { name: 'Copy link' }));
    expect(onCopy).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Copied' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Stop sharing' }));
    expect(onStop).toHaveBeenCalled();
  });

  it('opens a link with the name the others will see', async () => {
    const user = userEvent.setup();
    const onJoin = vi.fn();
    render(
      <JoinDialog
        open
        server="wss://collab.example.org"
        defaultName=""
        onJoin={onJoin}
        onCancel={() => {}}
      />,
    );
    expect(screen.getByRole('dialog')).toHaveTextContent('collab.example.org');
    await user.type(screen.getByLabelText('Your name'), 'Ben');
    await user.click(screen.getByRole('button', { name: 'Open' }));
    expect(onJoin).toHaveBeenCalledWith('Ben');
  });
});

describe('VersionHistoryPane', () => {
  it('lists the versions newest first and shows, compares, names and restores one', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onRestore = vi.fn();
    const onRename = vi.fn();
    const onCompare = vi.fn();
    const { rerender } = render(
      <VersionHistoryPane
        shared
        versions={versions}
        selected={null}
        onSelect={onSelect}
        onRestore={onRestore}
        onRename={onRename}
        onShare={() => {}}
        onCompare={onCompare}
        comparison={null}
        now={NOW}
      />,
    );
    const items = screen.getAllByRole('button').filter((b) => b.className.includes('kiln-versions__item'));
    expect(items.map((b) => b.textContent)).toEqual([
      'Current versionThe document as it is now',
      expect.stringContaining('Before the edits'),
      expect.stringContaining('Shared'),
    ]);
    await user.click(items[1] as HTMLElement);
    expect(onSelect).toHaveBeenCalledWith('v2');
    rerender(
      <VersionHistoryPane
        shared
        versions={versions}
        selected="v2"
        onSelect={onSelect}
        onRestore={onRestore}
        onRename={onRename}
        onShare={() => {}}
        onCompare={onCompare}
        comparison={[
          {
            change: 'Same',
            spans: [
              { change: 'Same', text: 'We load the ' },
              { change: 'Added', text: 'big' },
              { change: 'Removed', text: 'small' },
            ],
          },
          { change: 'Same', spans: [{ change: 'Same', text: 'Unchanged.' }] },
        ]}
        now={NOW}
      />,
    );
    const diff = screen.getByTestId('version-diff');
    expect(diff.querySelector('.kiln-versions__para ins')).toHaveTextContent('big');
    expect(diff.querySelector('.kiln-versions__para del')).toHaveTextContent('small');
    expect(diff).not.toHaveTextContent('Unchanged.');
    await user.click(screen.getByRole('button', { name: 'Hide changes' }));
    expect(onCompare).toHaveBeenCalledWith(null);
    await user.click(screen.getByRole('button', { name: 'Name' }));
    const box = screen.getByLabelText('Version name');
    await user.clear(box);
    await user.type(box, 'Final{Enter}');
    expect(onRename).toHaveBeenCalledWith('v2', 'Final');
    await user.click(screen.getByRole('button', { name: 'Restore' }));
    expect(onRestore).toHaveBeenCalledWith('v2');
  });

  it('asks to share a document that is not shared', async () => {
    const user = userEvent.setup();
    const onShare = vi.fn();
    render(
      <VersionHistoryPane
        shared={false}
        versions={[]}
        selected={null}
        onSelect={() => {}}
        onRestore={() => {}}
        onRename={() => {}}
        onShare={onShare}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Share…' }));
    expect(onShare).toHaveBeenCalled();
  });

  it('says when a version is from', () => {
    expect(when(NOW - 600_000, NOW)).toMatch(/^Today, \d\d:\d\d$/);
    expect(when(NOW - 86_400_000, NOW)).toMatch(/^Yesterday, /);
    expect(when(Date.UTC(2026, 0, 5, 12), NOW)).toMatch(/^5 Jan 2026, /);
  });

  it('bars the document while a version is shown', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(
      <VersionBar version={versions[1] as VersionEntry} onRestore={() => {}} onClose={onClose} now={NOW} />,
    );
    expect(screen.getByRole('status')).toHaveTextContent('Before the edits');
    await user.click(screen.getByRole('button', { name: 'Back to the document' }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('peerInk', () => {
  it('keeps the name on a person colour readable', () => {
    for (const dark of ['3159C6', '7A4A33', '8E3B8F', 'B4462A']) expect(peerInk(dark)).toBe('#fff');
    for (const light of ['C98A12', 'E05A27']) expect(peerInk(light)).toBe('#1b1712');
    expect(peerInk('')).toBe('#fff');
  });
});
