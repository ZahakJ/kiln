import { describe, expect, it } from 'vitest';
import { rotationAt } from './rotate.tsx';

describe('the rotation handle', () => {
  const c = { x: 100, y: 100 };
  it('reads the turn clockwise from straight up', () => {
    expect(rotationAt(c, { x: 100, y: 20 })).toBe(0);
    expect(rotationAt(c, { x: 180, y: 100 })).toBe(90);
    expect(rotationAt(c, { x: 100, y: 180 })).toBe(180);
    expect(rotationAt(c, { x: 20, y: 100 })).toBe(270);
  });
  it('snaps to every 45° within 4°', () => {
    expect(rotationAt(c, { x: 100 + 80 * Math.sin(0.75), y: 100 - 80 * Math.cos(0.75) })).toBe(45);
    expect(rotationAt(c, { x: 100 + 80 * Math.sin(0.5), y: 100 - 80 * Math.cos(0.5) })).toBeCloseTo(28.6, 1);
  });
});
