import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OptionsDialog } from '../chrome/OptionsDialog.tsx';
import {
  coreOptionsOf,
  DEFAULT_OPTIONS,
  limitRecents,
  loadOptions,
  mergeOptions,
  resetOptionsCache,
  saveOptions,
} from '../chrome/options.ts';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { Gallery } from '../primitives/Gallery.tsx';
import { Tooltip } from '../primitives/Tooltip.tsx';
import { changedCommands, keyboardFile, parseKeyboardFile } from './keyboardFile.ts';

vi.setConfig({ testTimeout: 20_000 });

describe('Options completeness', () => {
  it('maps the options the core applies', () => {
    const o = mergeOptions(DEFAULT_OPTIONS, {
      save: { autoSaveInterval: 900, keepRecoveryInfo: false },
      proofing: { ignoreUppercase: false },
    });
    expect(coreOptionsOf(o)).toEqual({
      autosave_delay_s: 600,
      recovery: false,
      spell_ignore_uppercase: false,
      spell_ignore_numbers: true,
      spell_ignore_addresses: true,
      ui_language: 'en',
    });
    expect(coreOptionsOf(DEFAULT_OPTIONS).autosave_delay_s).toBe(2);
    // The interface language, for what the core draws in it (Deck's placeholder prompts).
    expect(coreOptionsOf(mergeOptions(DEFAULT_OPTIONS, { language: { ui: 'ar' } })).ui_language).toBe('ar');
  });

  it('shows the pinned files and the most recent others up to the count', () => {
    const files = [
      { path: 'a', pinned: false },
      { path: 'b', pinned: true },
      { path: 'c', pinned: false },
      { path: 'd', pinned: false },
    ];
    expect(limitRecents(files, 2).map((f) => f.path)).toEqual(['a', 'b', 'c']);
    expect(limitRecents(files, 0).map((f) => f.path)).toEqual(['b']);
  });

  it('round-trips a keyboard file and refuses anything else', () => {
    const overrides = { version: 1 as const, layers: { write: { 'suite.format.bold': ['Mod+Shift+B'] } } };
    const text = keyboardFile(overrides);
    expect(parseKeyboardFile(text)).toEqual(overrides);
    expect(parseKeyboardFile(JSON.stringify(overrides))).toEqual(overrides);
    expect(changedCommands(overrides)).toBe(1);
    for (const bad of [
      '',
      'not json',
      '{"version":2,"layers":{}}',
      '{"version":1,"layers":{"write":{"x":[1]}}}',
      '[]',
    ])
      expect(parseKeyboardFile(bad)).toBeNull();
  });

  it('has an Advanced page whose options persist and a Privacy statement', async () => {
    resetOptionsCache();
    const user = userEvent.setup();
    // On Home every app's own options show (Write's drag and drop, Grid's Enter).
    renderWithCommands(
      <OptionsDialog open onOpenChange={() => {}} appName="Kiln" />,
      createHarness('hub'),
      'hub',
    );
    const nav = screen.getByRole('navigation', { name: 'Options sections' });
    await user.click(within(nav).getByRole('button', { name: 'Advanced' }));
    expect(screen.getByRole('heading', { name: 'Advanced' })).toBeInTheDocument();
    await user.click(screen.getByRole('checkbox', { name: /Allow text to be dragged and dropped/ }));
    expect(loadOptions().advanced.dragAndDrop).toBe(false);
    await user.selectOptions(screen.getByTestId('options-enter-direction'), 'right');
    expect(loadOptions().advanced.enterDirection).toBe('right');
    await user.click(screen.getByRole('checkbox', { name: /After pressing Enter, move selection/ }));
    expect(loadOptions().advanced.enterMoves).toBe(false);
    expect((screen.getByTestId('options-enter-direction') as HTMLSelectElement).disabled).toBe(true);
    await user.click(within(nav).getByRole('button', { name: 'Privacy' }));
    expect(screen.getByText(/Kiln has no telemetry/)).toBeInTheDocument();
    await user.click(within(nav).getByRole('button', { name: 'Save' }));
    await user.click(screen.getByRole('checkbox', { name: /Keep AutoRecover information/ }));
    expect(loadOptions().save.keepRecoveryInfo).toBe(false);
    saveOptions(DEFAULT_OPTIONS);
  });

  it('Live Preview off pointing at a gallery item previews nothing', async () => {
    resetOptionsCache();
    saveOptions({ general: { livePreview: false } });
    const user = userEvent.setup();
    const preview = vi.fn();
    render(
      <Gallery
        label="Styles"
        items={[
          { id: 'a', label: 'A' },
          { id: 'b', label: 'B' },
        ]}
        onSelect={() => {}}
        onPreview={preview}
      />,
    );
    await user.hover(screen.getAllByRole('option')[0] as HTMLElement);
    expect(preview).not.toHaveBeenCalledWith('a');
    saveOptions({ general: { livePreview: true } });
    await user.hover(screen.getAllByRole('option')[1] as HTMLElement);
    expect(preview).toHaveBeenCalledWith('b');
  });

  it('ScreenTip style off: the trigger has no tooltip', async () => {
    resetOptionsCache();
    saveOptions({ display: { screenTips: 'off' } });
    const user = userEvent.setup();
    render(
      <Tooltip content="Bold" description="Make text bold">
        <button type="button">B</button>
      </Tooltip>,
    );
    await user.hover(screen.getByRole('button', { name: 'B' }));
    await new Promise((r) => setTimeout(r, 700));
    expect(screen.queryByText('Make text bold')).toBeNull();
    saveOptions({ display: { screenTips: 'full' } });
  });
});
