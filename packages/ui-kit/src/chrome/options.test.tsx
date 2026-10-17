import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OptionsDialog } from './OptionsDialog.tsx';
import { loadOptions, resetOptionsCache } from './options.ts';
import { createHarness, renderWithCommands } from './test-helpers.tsx';

// User-event flows open Radix overlays; on a loaded 4-CPU machine they need more than Vitest's 5 s.
vi.setConfig({ testTimeout: 20_000 });

describe('OptionsDialog', () => {
  it('lists the §1.5 sections and persists changes', async () => {
    resetOptionsCache();
    const user = userEvent.setup();
    renderWithCommands(<OptionsDialog open onOpenChange={() => {}} appName="Kiln Write" />);
    const nav = screen.getByRole('navigation', { name: 'Options sections' });
    expect(
      within(nav)
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).toEqual(['General', 'Display', 'Proofing', 'Save', 'Language', 'Keyboard', 'Privacy']);
    await user.click(screen.getByRole('checkbox', { name: /Classic Office look/ }));
    expect(loadOptions().general.classicLook).toBe(true);
    await user.click(within(nav).getByRole('button', { name: 'Save' }));
    const interval = screen.getByRole('spinbutton', { name: /AutoSave every/ });
    await user.clear(interval);
    await user.type(interval, '30{Enter}');
    expect(loadOptions().save.autoSaveInterval).toBe(30);
    resetOptionsCache();
    expect(JSON.parse(localStorage.getItem('kiln.options') ?? '{}').save.autoSaveInterval).toBe(30);
  });

  it('remaps a shortcut live from the Keyboard section, Office-style', async () => {
    const user = userEvent.setup();
    const h = createHarness();
    renderWithCommands(<OptionsDialog open onOpenChange={() => {}} initialSection="keyboard" />, h);
    await user.type(screen.getByRole('searchbox', { name: /Search commands/ }), 'strikethrough');
    await user.click(screen.getByRole('option', { name: /Strikethrough/ }));
    const capture = screen.getByLabelText('Press new shortcut');
    await user.click(capture);
    await user.keyboard('{Control>}b{/Control}');
    expect(capture).toHaveValue('Ctrl+B');
    expect(screen.getByText(/Currently assigned to: Bold/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Assign' }));
    expect(screen.getByText(/Ctrl\+B now runs Strikethrough; removed from Bold/)).toBeInTheDocument();
    expect(h.keymap.resolve({ key: 'b', ctrlKey: true }, { app: 'write' })?.command).toBe(
      'suite.format.strikethrough',
    );
    await user.click(screen.getByRole('button', { name: 'Reset All' }));
    expect(h.keymap.resolve({ key: 'b', ctrlKey: true }, { app: 'write' })?.command).toBe(
      'suite.format.bold',
    );
  });
});
