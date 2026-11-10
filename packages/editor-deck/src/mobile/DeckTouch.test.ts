import { describe, expect, it } from 'vitest';
import { shapeHandles } from './DeckTouch.tsx';

describe('the shape handles a finger takes', () => {
  it('puts eight handles round the box and the rotation handles above it', () => {
    const h = shapeHandles({ x: 100, y: 100, width: 200, height: 100, rotation: 0 }, 1);
    expect(h.grips.map(([g]) => g)).toEqual(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']);
    expect(Object.fromEntries(h.grips).se).toEqual({ x: 300, y: 200 });
    // The core's at 22 screen px, the finger's further out, both over the top edge's middle.
    expect(h.rotateCore).toEqual({ x: 200, y: 78 });
    expect(h.rotateTouch).toEqual({ x: 200, y: 56 });
  });
  it('turns them with the box and keeps only the corners of a small one', () => {
    const h = shapeHandles({ x: 0, y: 0, width: 100, height: 100, rotation: 90 }, 1);
    const nw = Object.fromEntries(h.grips).nw;
    expect(nw?.x).toBeCloseTo(100);
    expect(nw?.y).toBeCloseTo(0);
    const small = shapeHandles({ x: 0, y: 0, width: 40, height: 10, rotation: 0 }, 1);
    expect(small.grips.map(([g]) => g)).toEqual(['nw', 'ne', 'se', 'sw']);
  });
});
