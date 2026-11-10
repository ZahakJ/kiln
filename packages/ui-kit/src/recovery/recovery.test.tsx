import type { RecoveryEntry } from '@kiln/protocol';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { DocumentRecoveryPane, problemOf } from './DocumentRecoveryPane.tsx';
import { FileChangedDialog } from './FileChangedDialog.tsx';
import { SaveChangesDialog } from './SaveChangesDialog.tsx';

vi.setConfig({ testTimeout: 20_000 });

const NOW = new Date(2026, 9, 23, 15, 0).getTime();

const entries: RecoveryEntry[] = [
  {
    key: 'a1',
    kind: 'Write',
    title: 'Document1',
    path: null,
    format: null,
    saved_at_ms: NOW - 60_000,
    problem: null,
  },
  {
    key: 'b2',
    kind: 'Grid',
    title: 'Budget',
    path: '/data/ana/Studio/Accounts/Budget.xlsx',
    format: 'Xlsx',
    saved_at_ms: NOW - 3_600_000,
    problem: null,
  },
  {
    key: 'c3',
    kind: 'Deck',
    title: 'Pitch',
    path: 'opfs:documents/Pitch.kdeck',
    format: 'KDeck',
    saved_at_ms: 0,
    problem: 'written by a newer version of Kiln (journal format 9; this version reads up to 1)',
  },
];

describe('Document Recovery', () => {
  it('lists each document with its file, place and time, and acts on the one chosen', async () => {
    const onOpen = vi.fn();
    const onSaveAs = vi.fn();
    const onDelete = vi.fn();
    const onClose = vi.fn();
    render(
      <DocumentRecoveryPane
        entries={entries}
        onOpen={onOpen}
        onSaveAs={onSaveAs}
        onDelete={onDelete}
        onClose={onClose}
        now={NOW}
      />,
    );
    const items = screen.getAllByRole('listitem');
    expect(items).toHaveLength(3);
    const first = within(items[0] as HTMLElement);
    expect(first.getByText('Document1')).toBeInTheDocument();
    // Never saved: its untitled name, with no extension it never had.
    expect(first.queryByText('.docx')).toBeNull();
    expect(first.getByText('Never saved to a file')).toBeInTheDocument();
    expect(first.getByText(/^Recovery copy from Today/)).toBeInTheDocument();
    const second = within(items[1] as HTMLElement);
    expect(second.getByText('.xlsx')).toBeInTheDocument();
    expect(second.getByText('Studio › Accounts')).toBeInTheDocument();
    // A copy Kiln cannot open says why and can only be deleted.
    const third = within(items[2] as HTMLElement);
    expect(
      third.getByText(/A newer version of Kiln kept this copy. Update Kiln to open it./),
    ).toBeInTheDocument();
    expect(third.queryByText(/journal format/)).toBeNull();
    expect(third.getByText('This browser')).toBeInTheDocument();
    expect(third.queryByRole('button', { name: /Open/ })).toBeNull();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Open Document1' }));
    expect(onOpen).toHaveBeenCalledWith(entries[0]);
    await user.click(screen.getByRole('button', { name: 'Save As… Budget.xlsx' }));
    expect(onSaveAs).toHaveBeenCalledWith(entries[1]);
    await user.click(screen.getByRole('button', { name: 'Delete Pitch.kdeck' }));
    expect(onDelete).toHaveBeenCalledWith(entries[2]);
    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('a copy Kiln cannot open', () => {
  it('says why in written words, never the reason the core gives', () => {
    const cases: [string, RegExp][] = [
      ['written by a newer version of Kiln (journal format 9; this version reads up to 4)', /newer version/],
      [
        'Kiln closed before the first recovery copy of this document was written, so its changes could not be kept.',
        /closed before/,
      ],
      ['the marker: expected value at line 1 column 1', /damaged/],
      ['', /damaged/],
    ];
    for (const [core, want] of cases) {
      const said = problemOf(core);
      expect(said).toMatch(want);
      if (core) expect(said).not.toContain(core);
      expect(said).not.toMatch(/marker|journal format|line 1/);
    }
  });
});

describe('a file changed by another program', () => {
  it('asks to reload, saying what is lost', async () => {
    const onReload = vi.fn();
    const onKeep = vi.fn();
    render(
      <FileChangedDialog
        open
        name="Report.docx"
        appName="Kiln Write"
        dirty
        onReload={onReload}
        onKeep={onKeep}
      />,
    );
    const dialog = screen.getByRole('alertdialog');
    expect(within(dialog).getByText('Report.docx')).toBeInTheDocument();
    expect(
      within(dialog).getByText(/changes you made here since the last save will be lost/),
    ).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(within(dialog).getByRole('button', { name: 'Reload' }));
    expect(onReload).toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Keep my version' }));
    expect(onKeep).toHaveBeenCalled();
  });

  it('decides nothing when it is closed without an answer', async () => {
    const onKeep = vi.fn();
    const onDismiss = vi.fn();
    render(
      <FileChangedDialog
        open
        name="Report.docx"
        appName="Kiln Write"
        dirty={false}
        onReload={vi.fn()}
        onKeep={onKeep}
        onDismiss={onDismiss}
      />,
    );
    await userEvent.setup().keyboard('{Escape}');
    expect(onDismiss).toHaveBeenCalled();
    expect(onKeep).not.toHaveBeenCalled();
  });
});

describe('closing a document with unsaved changes', () => {
  it("asks Save, Don't Save or Cancel", async () => {
    const onSave = vi.fn();
    const onDontSave = vi.fn();
    const onCancel = vi.fn();
    render(
      <SaveChangesDialog
        open
        name="Report.docx"
        appName="Kiln Write"
        onSave={onSave}
        onDontSave={onDontSave}
        onCancel={onCancel}
      />,
    );
    const dialog = screen.getByRole('alertdialog');
    expect(within(dialog).getByText('Report.docx')).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await user.click(within(dialog).getByRole('button', { name: "Don't Save" }));
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect([onSave, onDontSave, onCancel].map((f) => f.mock.calls.length)).toEqual([1, 1, 1]);
  });
});
