import { describe, expect, it } from 'vitest';
import { assignKeyTips, type KeyTipNode, KeyTipSession, normalizeKeyTip, TAB_KEYTIPS } from './keytips.ts';

const prefixFree = (tips: string[]) =>
  tips.every((a, i) => tips.every((b, j) => i === j || !b.startsWith(a)));

describe('Key Tips', () => {
  it('uses Office tab letters', () => {
    expect(TAB_KEYTIPS.write).toMatchObject({
      file: 'F',
      home: 'H',
      insert: 'N',
      layout: 'P',
      references: 'S',
      review: 'R',
      view: 'W',
    });
    expect(TAB_KEYTIPS.grid).toMatchObject({ formulas: 'M', data: 'A' });
    expect(TAB_KEYTIPS.deck).toMatchObject({
      design: 'G',
      transitions: 'K',
      animations: 'A',
      slideShow: 'S',
    });
  });

  it('normalises tips', () => {
    expect(normalizeKeyTip('fp')).toBe('FP');
    expect(normalizeKeyTip('')).toBeUndefined();
    expect(normalizeKeyTip('ABCD')).toBeUndefined();
  });

  it('honours preferred letters and assigns unique, prefix-free tips to the rest', () => {
    const tips = assignKeyTips(
      [
        { id: 'paste', label: 'Paste', preferred: 'V' },
        { id: 'cut', label: 'Cut', preferred: 'X' },
        { id: 'painter', label: 'Format Painter', preferred: 'FP' },
        { id: 'font', label: 'Font', preferred: 'FF' },
        { id: 'bold', label: 'Bold', preferred: '1' },
        { id: 'dupe', label: 'Duplicate', preferred: 'V' }, // taken
        { id: 'find', label: 'Find' },
        { id: 'fill', label: 'Fill' },
        { id: 'freeze', label: 'Freeze Panes' },
      ],
      ['Q'], // e.g. reserved for the title bar's search box
    );
    expect(tips.get('paste')).toBe('V');
    expect(tips.get('painter')).toBe('FP');
    expect(tips.get('bold')).toBe('1');
    expect(tips.get('dupe')).not.toBe('V');
    const all = [...tips.values(), 'Q'];
    expect(new Set(all).size).toBe(all.length);
    expect(prefixFree(all)).toBe(true);
  });

  it('never runs out of tips', () => {
    const many = Array.from({ length: 200 }, (_, i) => ({ id: `c${i}`, label: 'Same Label' }));
    const tips = [...assignKeyTips(many).values()];
    expect(new Set(tips).size).toBe(200);
    expect(prefixFree(tips)).toBe(true);
  });

  const tree: KeyTipNode[] = [
    { id: 'file', tip: 'F', label: 'File', kind: 'action' },
    {
      id: 'home',
      tip: 'H',
      label: 'Home',
      kind: 'container',
      children: [
        { id: 'bold', tip: '1', label: 'Bold', kind: 'action' },
        { id: 'painter', tip: 'FP', label: 'Format Painter', kind: 'action' },
        { id: 'font', tip: 'FF', label: 'Font', kind: 'action' },
      ],
    },
  ];

  it('walks tabs and commands: Alt, H, 1 = Bold', () => {
    const s = new KeyTipSession(tree);
    expect(s.visible.map((n) => n.tip)).toEqual(['F', 'H']);
    expect(s.press('h')).toMatchObject({ type: 'enter', node: { id: 'home' } });
    expect(s.depth).toBe(1);
    expect(s.press('1')).toMatchObject({ type: 'invoke', node: { id: 'bold' } });
    expect(s.active).toBe(false);
  });

  it('narrows multi-letter tips, ignores non-matching keys, and backs out with Esc', () => {
    const s = new KeyTipSession(tree);
    s.press('H');
    expect(s.press('F')).toEqual({ type: 'partial', prefix: 'F' });
    expect(s.visible.map((n) => n.tip)).toEqual(['FP', 'FF']);
    expect(s.press('Z')).toEqual({ type: 'none' });
    s.erase();
    expect(s.prefix).toBe('');
    s.press('F');
    expect(s.back()).toEqual({ type: 'partial', prefix: '' }); // clears the typed F
    expect(s.back()).toEqual({ type: 'partial', prefix: '' }); // up to the tabs
    expect(s.depth).toBe(0);
    expect(s.back()).toEqual({ type: 'exit' });
    expect(s.active).toBe(false);
    expect(s.press('H')).toEqual({ type: 'exit' });
  });

  it('can enter a level programmatically and ignores non-alphanumerics', () => {
    const s = new KeyTipSession(tree);
    expect(s.press('?')).toEqual({ type: 'none' });
    s.enter(tree[1]?.children ?? []);
    expect(s.press('F')).toMatchObject({ type: 'partial' });
    expect(s.press('F')).toMatchObject({ type: 'invoke', node: { id: 'font' } });
  });
});
