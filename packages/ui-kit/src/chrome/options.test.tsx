import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OptionsDialog } from './OptionsDialog.tsx';
import {
  DEFAULT_OPTIONS,
  editingLanguage,
  loadOptions,
  mergeOptions,
  resetOptionsCache,
  saveOptions,
} from './options.ts';
import { createHarness, renderWithCommands } from './test-helpers.tsx';

// User-event flows open Radix overlays; on a loaded 4-CPU machine they need more than Vitest's 5 s.
vi.setConfig({ testTimeout: 20_000 });

describe('OptionsDialog', () => {
  it('lists the Options sections and persists changes', async () => {
    resetOptionsCache();
    const user = userEvent.setup();
    renderWithCommands(<OptionsDialog open onOpenChange={() => {}} appName="Kiln Write" />);
    const nav = screen.getByRole('navigation', { name: 'Options sections' });
    expect(
      within(nav)
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).toEqual(['General', 'Display', 'Proofing', 'Save', 'Language', 'Advanced', 'Keyboard', 'Privacy']);
    await user.click(screen.getByRole('checkbox', { name: /Classic Office look/ }));
    expect(loadOptions().general.classicLook).toBe(true);
    await user.click(within(nav).getByRole('button', { name: 'Save' }));
    const interval = screen.getByRole('spinbutton', { name: /Save after a pause/ });
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

describe('editingLanguage', () => {
  const ui = (lang: string, editing = 'auto') =>
    mergeOptions(DEFAULT_OPTIONS, { language: { ui: lang, editing } });

  it('follows the display language until one is chosen, as Office does', () => {
    expect(DEFAULT_OPTIONS.language.editing).toBe('auto');
    expect(editingLanguage(ui('en'), [])).toBe('en-US');
    expect(editingLanguage(ui('de'), [])).toBe('de-DE');
    expect(editingLanguage(ui('ar'), [])).toBe('ar-SA');
  });

  it('follows the system language on a first run that takes the interface from the system', () => {
    expect(editingLanguage(ui('system'), ['ar-EG', 'en-US'])).toBe('ar-SA');
    expect(editingLanguage(ui('system'), ['de-AT'])).toBe('de-DE');
    // A system language Kiln offers for editing is kept as it is.
    expect(editingLanguage(ui('system'), ['en-GB', 'en'])).toBe('en-GB');
    expect(editingLanguage(ui('system'), ['ja-JP'])).toBe('en-US');
  });

  it('keeps an explicit choice whatever the display language', () => {
    expect(editingLanguage(ui('ar', 'fr-FR'), [])).toBe('fr-FR');
    expect(editingLanguage(ui('de', 'en-US'), [])).toBe('en-US');
    expect(editingLanguage(ui('en', ''), [])).toBe('en-US');
  });
});

describe('OptionsDialog ▸ Language', () => {
  it('shows the effective editing language and persists an explicit choice', async () => {
    localStorage.clear();
    resetOptionsCache();
    saveOptions({ language: { ui: 'de' } });
    const user = userEvent.setup();
    renderWithCommands(<OptionsDialog open onOpenChange={() => {}} initialSection="language" />);
    const select = screen.getByTestId('options-editing-language') as HTMLSelectElement;
    expect(select.value).toBe('auto');
    expect(select.selectedOptions[0]?.textContent).toMatch(/^Match display language — German/);
    await user.selectOptions(select, 'fr-FR');
    expect(loadOptions().language.editing).toBe('fr-FR');
    expect(editingLanguage()).toBe('fr-FR');
    resetOptionsCache();
    expect(JSON.parse(localStorage.getItem('kiln.options') ?? '{}').language.editing).toBe('fr-FR');
    await user.selectOptions(select, 'auto');
    expect(editingLanguage()).toBe('de-DE');
  });
});
