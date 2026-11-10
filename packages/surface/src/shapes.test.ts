import { describe, expect, it } from 'vitest';
import {
  blurredInterval,
  caretSpark,
  erf,
  isValidPath,
  roundedRect,
  spark,
  squiggle,
  teardrop,
  tracePath,
  underlineSegments,
  VERB,
} from './shapes';

function bounds(p: { coords: number[] }) {
  const xs = p.coords.filter((_, i) => i % 2 === 0);
  const ys = p.coords.filter((_, i) => i % 2 === 1);
  return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
}

describe('shapes (twins of kiln_scene::shapes)', () => {
  it('builds valid closed paths', () => {
    for (const p of [
      roundedRect({ x: 0, y: 0, w: 10, h: 6 }, 4),
      roundedRect({ x: 0, y: 0, w: 10, h: 6 }, 0),
      spark(10, 10, 8),
      teardrop(5, 5, 20, 'Start'),
      teardrop(5, 5, 20, 'End'),
      teardrop(5, 5, 20, 'Caret'),
    ]) {
      expect(isValidPath(p)).toBe(true);
      expect(p.verbs.at(-1)).toBe(VERB.CLOSE);
    }
  });

  it('matches the Rust geometry', () => {
    // kiln-scene: spark(10, 10, 24) spans y -1…21 and starts at x 1.86.
    const b = bounds(spark(10, 10, 24));
    expect(b.y0).toBeCloseTo(-1, 4);
    expect(b.y1).toBeCloseTo(21, 4);
    expect(b.x0).toBeCloseTo(1.86, 4);
    const end = bounds(teardrop(100, 50, 20, 'End'));
    expect([end.x0, end.y0]).toEqual([100, 50]);
    expect(bounds(teardrop(100, 50, 20, 'Start')).x1).toBeCloseTo(100, 4);
    const caret = bounds(teardrop(100, 50, 20, 'Caret'));
    expect((caret.x0 + caret.x1) / 2).toBeCloseTo(100, 0);
    const sq = squiggle(10, 20, 9);
    expect(sq.verbs).toEqual([VERB.MOVE, VERB.QUAD, VERB.QUAD, VERB.QUAD, VERB.QUAD, VERB.QUAD]);
    expect(bounds(sq).x1).toBeCloseTo(19);
    expect(squiggle(0, 0, Number.NaN).verbs).toEqual([]);
    const cs = bounds(caretSpark({ x: 60, y: 20, w: 2, h: 20 }));
    expect((cs.x0 + cs.x1) / 2).toBeCloseTo(61, 1);
  });

  it('computes the same erf and blur profile', () => {
    expect(erf(0)).toBeCloseTo(0, 8);
    expect(erf(0.5)).toBeCloseTo(0.5204999, 6);
    expect(erf(1)).toBeCloseTo(0.8427008, 6);
    expect(erf(-2)).toBeCloseTo(-0.9953223, 6);
    expect(blurredInterval(5, 0, 10, 0)).toBe(1);
    expect(blurredInterval(0, 0, 1000, 4)).toBeCloseTo(0.5, 6);
  });

  it('splits underlines into dashes', () => {
    expect(underlineSegments(0, 10, 1, 'Solid')).toEqual([[0, 10]]);
    expect(underlineSegments(0, 5, 1, 'Dotted')).toEqual([
      [0, 1],
      [2, 1],
      [4, 1],
    ]);
    expect(underlineSegments(0, 4, 1, 'Dashed')).toEqual([[0, 3]]);
    expect(underlineSegments(0, Number.NaN, 1, 'Dotted')).toEqual([]);
  });

  it('replays paths and refuses malformed ones', () => {
    const calls: string[] = [];
    const target = {
      moveTo: () => calls.push('M'),
      lineTo: () => calls.push('L'),
      quadraticCurveTo: () => calls.push('Q'),
      bezierCurveTo: () => calls.push('C'),
      closePath: () => calls.push('Z'),
    };
    expect(tracePath(roundedRect({ x: 0, y: 0, w: 4, h: 4 }, 1), target)).toBe(true);
    expect(calls.join('')).toBe('MLCLCLCLCZ');
    expect(tracePath({ verbs: [0, 3], coords: [0, 0, 1] }, target)).toBe(false);
    expect(tracePath({ verbs: [9], coords: [] }, target)).toBe(false);
    expect(tracePath({ verbs: [0], coords: [Number.NaN, 0] }, target)).toBe(false);
  });
});
