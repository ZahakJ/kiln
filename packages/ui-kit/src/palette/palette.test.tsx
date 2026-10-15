import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { CommandPalette } from './CommandPalette.tsx';

function Host({ initial = true }: { initial?: boolean }) {
  const [open, setOpen] = useState(initial);
  return <CommandPalette open={open} onOpenChange={setOpen} />;
}

function setup(app: 'write' | 'grid' = 'grid') {
  const h = createHarness(app);
  const merge = vi.fn();
  const freeze = vi.fn();
  h.registry.register({ id: 'grid.mergeCenter', title: 'Merge & Center', run: merge, apps: ['grid'] });
  h.registry.register({ id: 'grid.freezePanes', title: 'Freeze Panes', run: freeze, apps: ['grid'] });
  h.registry.register({ id: 'format.bold', title: 'Bold', run: () => {} });
  h.registry.addSynonyms({ 'grid.freezePanes': ['lock rows', 'sticky header'] });
  const r = renderWithCommands(<Host />, h, app);
  return { ...r, merge, freeze };
}

const options = () => within(screen.getByRole('listbox')).queryAllByRole('option');

describe('CommandPalette', () => {
  it('lists commands with their shortcuts, available ones first', () => {
    setup();
    const opts = options();
    expect(opts.length).toBeGreaterThan(50);
    expect(opts[0]).toHaveTextContent('Bold');
    expect(opts[0]).toHaveTextContent('Ctrl');
  });

  it('filters fuzzily over titles, Office names and synonyms', async () => {
    const user = userEvent.setup();
    setup();
    const input = screen.getByRole('combobox');
    await user.type(input, 'frz pns');
    expect(options()[0]).toHaveTextContent('Freeze Panes');
    await user.clear(input);
    await user.type(input, 'sticky header');
    expect(options()[0]).toHaveTextContent('Freeze Panes');
    expect(options()[0]).toHaveTextContent('Matches “sticky header”');
    await user.clear(input);
    await user.type(input, 'tell me');
    expect(
      options()
        .map((o) => o.textContent)
        .join('|'),
    ).toContain('Command Palette');
    await user.clear(input);
    await user.type(input, 'zzzzqqq');
    expect(options()).toHaveLength(0);
    expect(screen.getByText(/No commands match/)).toBeInTheDocument();
  });

  it('hides commands of other apps', async () => {
    const user = userEvent.setup();
    setup('write');
    await user.type(screen.getByRole('combobox'), 'Merge & Center');
    expect(options().some((o) => o.textContent?.includes('Merge & Center'))).toBe(false);
  });

  it('runs the chosen command with Enter, closes, and lists it under Recent next time', async () => {
    const user = userEvent.setup();
    const { merge, recent } = setup();
    await user.type(screen.getByRole('combobox'), 'merge center');
    await user.keyboard('{Enter}');
    await waitFor(() => expect(merge).toHaveBeenCalledOnce());
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(recent.list()).toEqual(['grid.mergeCenter']);
  });

  it('shows Recent first when the query is empty', () => {
    const h = createHarness('grid');
    h.registry.register({ id: 'grid.freezePanes', title: 'Freeze Panes', run: () => {}, apps: ['grid'] });
    h.recent.record('grid.freezePanes');
    renderWithCommands(<Host />, h, 'grid');
    const recentGroup = screen.getByRole('group', { name: 'Recent' });
    expect(within(recentGroup).getByRole('option')).toHaveTextContent('Freeze Panes');
  });

  it('marks not-yet-available commands and does not run them', async () => {
    const user = userEvent.setup();
    const { registry } = setup();
    const spy = vi.spyOn(registry, 'execute');
    await user.type(screen.getByRole('combobox'), 'goal seek');
    const opt = options()[0];
    expect(opt).toHaveTextContent('Coming soon');
    expect(opt).toHaveAttribute('aria-disabled', 'true');
    await user.keyboard('{Enter}');
    expect(spy).not.toHaveBeenCalled();
  });

  it('closes with Escape', async () => {
    const user = userEvent.setup();
    setup();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
