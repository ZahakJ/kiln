import type { InspectItem } from '@kiln/protocol';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import '../chrome/test-matchers.ts';
import { CompatibilityDialog } from './CompatibilityDialog.tsx';
import { InspectDialog } from './InspectDialog.tsx';
import { PropertiesDialog } from './PropertiesDialog.tsx';

vi.setConfig({ testTimeout: 20_000 });

describe('File ▸ Info dialogs', () => {
  it('Properties sends only the fields that changed', async () => {
    const user = userEvent.setup();
    const save = vi.fn(async () => {});
    render(
      <PropertiesDialog
        open
        onOpenChange={() => {}}
        app="grid"
        name="Book1"
        load={async () => ({
          title: 'Budget',
          subject: null,
          author: 'Ana',
          keywords: null,
          description: null,
          last_modified_by: null,
          created: '2026-10-01T09:00:00Z',
          modified: null,
          revision: null,
          application: null,
          pages: 0,
          words: 0,
          characters: 0,
          paragraphs: 0,
          category: null,
          status: null,
          company: null,
          manager: null,
          sheets: 3,
          slides: 0,
        })}
        save={save}
      />,
    );
    const title = await screen.findByLabelText('Title');
    expect(title).toHaveValue('Budget');
    expect(screen.getByText('3 worksheets')).toBeInTheDocument();
    await user.clear(title);
    await user.type(title, 'Budget 2027');
    await user.type(screen.getByLabelText('Company'), 'Kiln');
    await user.click(screen.getByRole('button', { name: 'OK' }));
    expect(save).toHaveBeenCalledWith({ title: 'Budget 2027', company: 'Kiln' });
  });

  it('Properties that cannot be read say so, with a way to try again', async () => {
    const user = userEvent.setup();
    const load = vi.fn().mockRejectedValueOnce(new Error('x')).mockResolvedValue({
      title: 'Again',
      pages: 1,
      words: 2,
      characters: 3,
      paragraphs: 1,
      sheets: 0,
      slides: 0,
    });
    render(
      <PropertiesDialog
        open
        onOpenChange={() => {}}
        app="write"
        name="Doc"
        load={load}
        save={async () => {}}
      />,
    );
    expect(await screen.findByRole('alert')).toHaveTextContent('could not read');
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByLabelText('Title')).toHaveValue('Again');
  });

  it('the Compatibility Checker groups what each format loses', async () => {
    const user = userEvent.setup();
    const check = vi.fn(async (format: string) =>
      format === 'Docx'
        ? [
            {
              code: 'macros.removed',
              severity: 'Loss' as const,
              message: 'Macros are removed',
              count: 1,
              origin: 'Save' as const,
            },
          ]
        : [],
    );
    render(
      <CompatibilityDialog
        open
        onOpenChange={() => {}}
        formats={[
          { format: 'Docx', label: 'Word document' },
          { format: 'Odt', label: 'OpenDocument text' },
        ]}
        check={check}
      />,
    );
    expect(await screen.findByText('Macros are removed')).toBeInTheDocument();
    expect(screen.getByText('Will be lost')).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /\.odt/ }));
    expect(await screen.findByText('Nothing will be lost in .odt')).toBeInTheDocument();
  });

  it('Inspect Document lists each kind and removes one', async () => {
    const user = userEvent.setup();
    let items: InspectItem[] = [
      { kind: 'Comments', count: 2 },
      { kind: 'HiddenSlides', count: 0 },
      { kind: 'SpeakerNotes', count: 1 },
    ];
    const remove = vi.fn(async (kind: string) => {
      items = items.map((i) => (i.kind === kind ? { ...i, count: 0 } : i));
    });
    render(
      <InspectDialog open onOpenChange={() => {}} app="deck" inspect={async () => items} remove={remove} />,
    );
    expect(await screen.findByText('2 comments found')).toBeInTheDocument();
    expect(screen.getByText('1 slide with notes found')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove All: Comments' }));
    expect(remove).toHaveBeenCalledWith('Comments');
    expect(await screen.findByText('Removed')).toBeInTheDocument();
  });
});
