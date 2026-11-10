import { describe, expect, it } from 'vitest';
import { gridKeyAction } from './keys.ts';

const enter = (shift = false) => ({
  key: 'Enter',
  shiftKey: shift,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
});

describe('Options ▸ Advanced ▸ After pressing Enter, move selection', () => {
  it('moves the way the option says, Shift+Enter the other way', () => {
    const right = { moves: true, direction: 'right' as const };
    expect(gridKeyAction(enter(), 'Ready', null, right)).toMatchObject({
      args: { backward: false, across: true },
    });
    expect(gridKeyAction(enter(true), 'Ready', null, right)).toMatchObject({
      args: { backward: true, across: true },
    });
    const up = { moves: true, direction: 'up' as const };
    expect(gridKeyAction(enter(), 'Ready', null, up)).toMatchObject({
      args: { backward: true, across: false },
    });
    expect(gridKeyAction(enter(), 'Enter', null, right)).toMatchObject({
      id: 'grid.editing.commit',
      args: { advance: 'Right' },
    });
    expect(gridKeyAction(enter(true), 'Enter', null, right)).toMatchObject({ args: { advance: 'Left' } });
  });

  it('stays on the cell when Enter does not move', () => {
    const stay = { moves: false, direction: 'down' as const };
    expect(gridKeyAction(enter(), 'Ready', null, stay)).toBeNull();
    expect(gridKeyAction(enter(), 'Enter', null, stay)).toMatchObject({
      id: 'grid.editing.commit',
      args: { advance: null },
    });
  });
});
