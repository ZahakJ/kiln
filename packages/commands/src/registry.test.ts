import { describe, expect, it, vi } from 'vitest';
import { createCommandRegistry } from './bootstrap.ts';
import { COMMAND_CATALOG } from './catalog/index.ts';
import { RecentCommands } from './recent.ts';
import { CommandRegistry } from './registry.ts';
import { MemoryStorage } from './storage.ts';
import { DEV_TIERS, RELEASE_TIERS } from './tiers.ts';
import type { RegistryEvent } from './types.ts';

describe('CommandRegistry', () => {
  it('registers, executes and passes context and args', async () => {
    const reg = new CommandRegistry({ context: { app: 'write', doc: 'a' } });
    const run = vi.fn((ctx, args) => `${ctx.doc}:${(args as { n: number }).n}`);
    reg.register({ id: 'test.run', title: 'Run', run });
    const result = await reg.execute('test.run', { n: 2 });
    expect(result).toEqual({ id: 'test.run', status: 'ran', value: 'a:2' });
    expect(run).toHaveBeenCalledOnce();
  });

  it('rejects malformed ids, empty titles and duplicate implementations', () => {
    const reg = new CommandRegistry();
    expect(() => reg.register({ id: 'Bad Id', title: 'x', run: () => {} })).toThrow(/Invalid command id/);
    expect(() => reg.register({ id: 'a.b', title: ' ', run: () => {} })).toThrow(/needs a title/);
    reg.register({ id: 'a.b', title: 'B', run: () => {} });
    expect(() => reg.register({ id: 'a.b', title: 'B', run: () => {} })).toThrow(/already registered/);
  });

  it('declared commands are pending no-ops until implemented, and come back when disposed', async () => {
    const reg = new CommandRegistry({ context: { app: 'grid' } });
    reg.declare({
      id: 'grid.view.freeze_panes',
      title: 'Freeze Panes',
      officeName: 'Freeze Panes',
      icon: 'FreezePanes',
      synonyms: ['lock rows'],
    });
    expect(reg.getState('grid.view.freeze_panes')).toMatchObject({
      exists: true,
      pending: true,
      enabled: false,
      visible: true,
    });
    expect((await reg.execute('grid.view.freeze_panes')).status).toBe('pending');

    const run = vi.fn();
    const d = reg.register({
      id: 'grid.view.freeze_panes',
      title: 'Freeze Panes',
      run,
      synonyms: ['sticky header'],
    });
    const meta = reg.meta('grid.view.freeze_panes');
    expect(meta).toMatchObject({ pending: false, icon: 'FreezePanes', officeName: 'Freeze Panes' });
    expect(meta?.synonyms).toEqual(['lock rows', 'sticky header']);
    expect((await reg.execute('grid.view.freeze_panes')).status).toBe('ran');

    d.dispose();
    expect(reg.getState('grid.view.freeze_panes').pending).toBe(true);
    d.dispose(); // idempotent
    expect(reg.has('grid.view.freeze_panes')).toBe(true);
  });

  it('reports unknown, disabled, hidden and failing commands without throwing', async () => {
    const onError = vi.fn();
    const reg = new CommandRegistry({ context: { app: 'write', selection: false }, onError });
    reg.register({ id: 'a.disabled', title: 'D', run: () => {}, isEnabled: (c) => !!c.selection });
    reg.register({ id: 'a.hidden', title: 'H', run: () => {}, when: () => false });
    reg.register({ id: 'a.grid_only', title: 'G', run: () => {}, apps: ['grid'] });
    reg.register({
      id: 'a.boom',
      title: 'B',
      run: () => {
        throw new Error('boom');
      },
    });
    expect((await reg.execute('nope.nope')).status).toBe('unknown');
    expect((await reg.execute('a.disabled')).status).toBe('disabled');
    expect((await reg.execute('a.hidden')).status).toBe('hidden');
    expect((await reg.execute('a.grid_only')).status).toBe('hidden');
    const failed = await reg.execute('a.boom');
    expect(failed.status).toBe('failed');
    expect(onError).toHaveBeenCalledWith('a.boom', expect.any(Error));
    reg.setContext({ selection: true });
    expect(reg.getState('a.disabled').enabled).toBe(true);
  });

  it('computes checked and value state and survives throwing state functions', () => {
    let bold = false;
    const reg = new CommandRegistry();
    reg.register({
      id: 'suite.format.bold',
      title: 'Bold',
      run: () => {
        bold = !bold;
      },
      isChecked: () => bold,
    });
    reg.register({ id: 'suite.format.font_size', title: 'Font Size', run: () => {}, value: () => 11 });
    reg.register({
      id: 'a.bad',
      title: 'Bad',
      run: () => {},
      isEnabled: () => {
        throw new Error('x');
      },
    });
    expect(reg.getState('suite.format.bold').checked).toBe(false);
    bold = true;
    expect(reg.getState('suite.format.bold').checked).toBe(true);
    expect(reg.getState('suite.format.font_size')).toMatchObject({ value: 11, checked: undefined });
    expect(reg.getState('a.bad').enabled).toBe(true);
    expect(reg.getState('missing.cmd')).toMatchObject({ exists: false, visible: false });
  });

  it('notifies per-id and global subscribers, and batches notifications', () => {
    const reg = new CommandRegistry();
    const events: RegistryEvent[] = [];
    reg.subscribe((e) => events.push(e));
    const bold = vi.fn();
    const italic = vi.fn();
    reg.subscribeTo('suite.format.bold', bold);
    const unsub = reg.subscribeTo('suite.format.italic', italic);
    reg.register({ id: 'suite.format.bold', title: 'Bold', run: () => {} });
    expect(bold).toHaveBeenCalledTimes(1);
    expect(italic).not.toHaveBeenCalled();
    reg.invalidate();
    expect(bold).toHaveBeenCalledTimes(2);
    expect(italic).toHaveBeenCalledTimes(1);
    unsub();
    reg.invalidate(['suite.format.italic']);
    expect(italic).toHaveBeenCalledTimes(1);

    events.length = 0;
    reg.batch(() => {
      reg.invalidate(['a.b']);
      reg.invalidate(['c.d']);
      reg.setContext({ x: 1 });
    });
    expect(events.filter((e) => e.type === 'state')).toHaveLength(1);
  });

  it('registerMany disposes everything at once', () => {
    const reg = new CommandRegistry();
    const d = reg.registerMany([
      { id: 'a.one', title: 'One', run: () => {} },
      { id: 'a.two', title: 'Two', run: () => {} },
    ]);
    expect(reg.size).toBe(2);
    d.dispose();
    expect(reg.size).toBe(0);
  });

  it('hides P1/P2 and pending commands in release builds', () => {
    const reg = new CommandRegistry({ tiers: RELEASE_TIERS, context: { app: 'grid' } });
    reg.declare([
      { id: 'grid.data.goal_seek', title: 'Goal Seek', tier: 'P1' },
      { id: 'grid.data.sort', title: 'Sort' },
    ]);
    reg.register({ id: 'grid.pivot', title: 'Pivot', tier: 'P1', run: () => {} });
    reg.register({ id: 'grid.filter', title: 'Filter', run: () => {} });
    expect(reg.list({ visibleOnly: true }).map((c) => c.id)).toEqual(['grid.filter']);
    reg.setTiers(DEV_TIERS);
    expect(reg.list({ visibleOnly: true })).toHaveLength(4);
  });

  it('records successful runs in the recent-commands store', async () => {
    const recent = new RecentCommands({ storage: new MemoryStorage() });
    const reg = new CommandRegistry({ recent });
    reg.register({ id: 'a.one', title: 'One', run: () => {} });
    reg.register({ id: 'a.off', title: 'Off', run: () => {}, isEnabled: () => false });
    await reg.execute('a.one');
    await reg.execute('a.off');
    expect(recent.list()).toEqual(['a.one']);
    expect(reg.recent).toBe(recent);
  });

  it('createCommandRegistry declares the whole catalog with Office synonyms', () => {
    const reg = createCommandRegistry({
      app: 'grid',
      recent: new RecentCommands({ storage: new MemoryStorage() }),
    });
    expect(reg.size).toBe(COMMAND_CATALOG.length);
    expect(reg.meta('grid.alignment.merge_center')?.synonyms).toContain('merge and center');
    expect(reg.context.app).toBe('grid');
  });
});
