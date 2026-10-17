import {
  COMMAND_CATALOG,
  CommandRegistry,
  createDefaultKeymap,
  MemoryStorage,
  RELEASE_TIERS,
  RecentCommands,
} from '@kiln/commands';
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { createHarness, renderWithCommands } from '../chrome/test-helpers.tsx';
import { button, dropdown, group, item, ribbon, row, spacer, tab, toggle } from './builders.ts';
import { Ribbon } from './Ribbon.tsx';
import { visibleGroup, visibleSpec, visibleTab } from './visibility.ts';

/** A release-build registry: the catalog declared (pending), `implemented` registered for real. */
function releaseRegistry(implemented: readonly string[]) {
  const registry = new CommandRegistry({
    context: { app: 'write' },
    tiers: RELEASE_TIERS,
    recent: new RecentCommands({ storage: new MemoryStorage() }),
  });
  registry.declare(COMMAND_CATALOG);
  for (const id of implemented) registry.register({ id, title: id, run: () => {} });
  return registry;
}

const spec = ribbon('write', [
  tab('home', 'Home', [
    group('clipboard', 'Clipboard', [button('suite.edit.cut'), button('suite.edit.copy')]),
    group(
      'font',
      'Font',
      [row(toggle('suite.format.bold'), toggle('suite.format.italic')), spacer(), button('suite.edit.undo')],
      { launcher: { command: 'suite.format.font_dialog' } },
    ),
    group('editing', 'Editing', [
      dropdown('suite.edit.select', [item('write.caret.select_all'), item('suite.arrange.selection_pane')]),
    ]),
  ]),
  tab('review', 'Review', [group('comments', 'Comments', [button('suite.review.new_comment')])]),
  tab('view', 'View', [group('zoom', 'Zoom', [button('suite.view.zoom')])]),
]);

describe('ribbon visibility in release builds', () => {
  const registry = releaseRegistry(['suite.edit.undo', 'suite.view.zoom', 'write.caret.select_all']);

  it('drops a group whose every item is hidden', () => {
    const home = spec.tabs[0];
    expect(home && visibleGroup(registry, home.groups[0] as never)).toBeNull();
  });

  it('keeps a group with one shown item, without its hidden row, spacer or launcher', () => {
    const font = visibleGroup(registry, spec.tabs[0]?.groups[1] as never);
    expect(font).not.toBeNull();
    expect(font?.items.map((it) => it.kind)).toEqual(['button']);
    expect(font?.launcher).toBeUndefined();
    // Thinned down to one command, it takes the full height instead of floating in the corner.
    expect(font?.items[0]).toMatchObject({ kind: 'button', size: 'large' });
  });

  it('shows a menu-only dropdown while any of its entries is implemented', () => {
    const editing = visibleGroup(registry, spec.tabs[0]?.groups[2] as never);
    expect(editing?.items).toHaveLength(1);
  });

  it('drops a tab whose every group is hidden and keeps the others', () => {
    expect(visibleTab(registry, spec.tabs[1] as never)).toBeNull();
    expect(visibleSpec(registry, spec).tabs.map((t) => t.id)).toEqual(['home', 'view']);
  });

  it('returns the same objects when nothing is hidden (stable memo inputs)', () => {
    const view = spec.tabs[2];
    expect(view && visibleTab(registry, view)).toBe(view);
  });

  it('renders no empty group slab and no empty tab', () => {
    const keymap = createDefaultKeymap({ platform: 'other', storage: new MemoryStorage() });
    renderWithCommands(<Ribbon spec={spec} layout="classic" />, {
      registry,
      keymap,
      recent: new RecentCommands({ storage: new MemoryStorage() }),
    });
    expect(screen.getAllByRole('tab').map((t) => t.textContent)).toEqual(['Home', 'View']);
    const groups = screen.getAllByRole('group').map((g) => g.getAttribute('aria-label'));
    expect(groups).toEqual(['Font', 'Editing']);
    for (const g of screen.getAllByRole('group')) {
      expect(g.querySelector('button, input, [role="combobox"]')).not.toBeNull();
    }
  });

  it('development builds keep pending slots (previewed) so ribbons can be reviewed', () => {
    const h = createHarness();
    expect(visibleSpec(h.registry, spec).tabs.map((t) => t.id)).toEqual(['home', 'review', 'view']);
  });
});
