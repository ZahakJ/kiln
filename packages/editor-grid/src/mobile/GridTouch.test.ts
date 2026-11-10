import { describe, expect, it } from 'vitest';
import { fitColumnsZoom } from './GridTouch.tsx';

describe('double tap: the columns in use fit the width', () => {
  it('fits the row headers and the columns up to the right edge', () => {
    expect(fitColumnsZoom({ header_w: 48 }, 400, 352)).toBe(1);
    expect(fitColumnsZoom({ header_w: 48 }, 400, 752)).toBe(0.5);
  });
  it('keeps to a readable range', () => {
    expect(fitColumnsZoom({ header_w: 48 }, 1000, 64)).toBe(2);
    expect(fitColumnsZoom({ header_w: 48 }, 400, 20000)).toBe(0.25);
  });
});
