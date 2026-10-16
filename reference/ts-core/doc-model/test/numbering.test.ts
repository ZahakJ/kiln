import { describe, expect, it } from 'vitest';
import {
  bulletDisplayText,
  formatNumber,
  toCardinalText,
  toLetters,
  toOrdinalText,
  toRoman,
} from '../src/numbering/formats.ts';
import { computeLabelsForParagraphs, computeListLabels } from '../src/numbering/labels.ts';
import { formatLevelText, Numbering, type NumberingLevel } from '../src/numbering/numbering.ts';
import { StyleSheet } from '../src/styles/stylesheet.ts';
import type { ParagraphProps } from '../src/types.ts';
import { cell, doc, p, row, table } from './helpers.ts';

const lvl = (
  level: number,
  format: string,
  textTpl: string,
  extra: Partial<NumberingLevel> = {},
): NumberingLevel => ({
  level,
  start: 1,
  format,
  text: textTpl,
  ...extra,
});

function styles(): StyleSheet {
  return new StyleSheet({ pPr: {}, rPr: {} }, [
    { id: 'Normal', type: 'paragraph', name: 'Normal', isDefault: true },
    {
      id: 'Heading1',
      type: 'paragraph',
      name: 'heading 1',
      basedOn: 'Normal',
      pPr: { numbering: { numId: 5, level: 0 } },
    },
    {
      id: 'Heading2',
      type: 'paragraph',
      name: 'heading 2',
      basedOn: 'Normal',
      pPr: { numbering: { numId: 5 } as never },
    },
  ]);
}

/** Numbering with: 1 = outline 1./1.1./1.1.1., 2 = second instance of the same abstract, 3 = bullets. */
function numbering(): Numbering {
  const n = new Numbering();
  n.addAbstract({
    id: 0,
    multiLevelType: 'multilevel',
    levels: [
      lvl(0, 'decimal', '%1.'),
      lvl(1, 'decimal', '%1.%2.'),
      lvl(2, 'decimal', '%1.%2.%3.'),
      lvl(3, 'lowerLetter', '(%4)'),
      lvl(4, 'lowerRoman', '%5.'),
    ],
  });
  n.addInstance(0, [], 1);
  n.addInstance(0, [], 2);
  n.addAbstract({
    id: 1,
    levels: [
      lvl(0, 'bullet', '', { rPr: { fonts: { ascii: 'Symbol', hAnsi: 'Symbol' } } }),
      lvl(1, 'bullet', 'o', { rPr: { fonts: { ascii: 'Courier New' } } }),
    ],
  });
  n.addInstance(1, [], 3);
  // Legal: roman level 0, legal level 1 shows "1.1" not "I.1".
  n.addAbstract({
    id: 2,
    levels: [lvl(0, 'upperRoman', 'Article %1.'), lvl(1, 'decimal', 'Section %1.%2', { isLegal: true })],
  });
  n.addInstance(2, [], 4);
  // Heading-linked outline numbering.
  n.addAbstract({
    id: 3,
    levels: [
      lvl(0, 'decimal', '%1', { pStyle: 'Heading1' }),
      lvl(1, 'decimal', '%1.%2', { pStyle: 'Heading2' }),
    ],
  });
  n.addInstance(3, [], 5);
  // Restart rule: level 1 never restarts (lvlRestart 0); level 2 restarts only after level 0.
  n.addAbstract({
    id: 4,
    levels: [
      lvl(0, 'decimal', '%1.'),
      lvl(1, 'decimal', '%2)', { restartAfter: 0 }),
      lvl(2, 'lowerLetter', '%3.', { restartAfter: 1 }),
    ],
  });
  n.addInstance(4, [], 6);
  // Start at 5, upperLetter, none-format middle level.
  n.addAbstract({
    id: 5,
    levels: [lvl(0, 'upperLetter', '%1.', { start: 5 }), lvl(1, 'none', ''), lvl(2, 'decimal', '%1.%2.%3')],
  });
  n.addInstance(5, [], 7);
  return n;
}

type Item = [numId: number, level: number] | ParagraphProps;

function labels(items: Item[], n: Numbering = numbering()): (string | undefined)[] {
  const paras = items.map((it) =>
    Array.isArray(it) ? p({ numbering: { numId: it[0], level: it[1] } }, 'x') : p(it, 'x'),
  );
  return computeLabelsForParagraphs(paras, { styles: styles(), numbering: n }).map((l) => l?.text);
}

interface LabelCase {
  name: string;
  items: Item[];
  expected: (string | undefined)[];
  setup?: (n: Numbering) => void;
}

const cases: LabelCase[] = [
  {
    name: 'simple decimal sequence',
    items: [
      [1, 0],
      [1, 0],
      [1, 0],
    ],
    expected: ['1.', '2.', '3.'],
  },
  {
    name: 'second level under first',
    items: [
      [1, 0],
      [1, 1],
      [1, 1],
    ],
    expected: ['1.', '1.1.', '1.2.'],
  },
  {
    name: 'deeper level restarts after shallower',
    items: [
      [1, 0],
      [1, 1],
      [1, 1],
      [1, 0],
      [1, 1],
    ],
    expected: ['1.', '1.1.', '1.2.', '2.', '2.1.'],
  },
  {
    name: 'three levels',
    items: [
      [1, 0],
      [1, 1],
      [1, 2],
      [1, 2],
      [1, 1],
      [1, 2],
    ],
    expected: ['1.', '1.1.', '1.1.1.', '1.1.2.', '1.2.', '1.2.1.'],
  },
  {
    name: 'level 3 lowerLetter in parens',
    items: [
      [1, 0],
      [1, 3],
      [1, 3],
    ],
    expected: ['1.', '(a)', '(b)'],
  },
  {
    name: 'level 4 lowerRoman',
    items: [
      [1, 4],
      [1, 4],
      [1, 4],
      [1, 4],
    ],
    expected: ['i.', 'ii.', 'iii.', 'iv.'],
  },
  {
    name: 'unused higher level renders its start',
    items: [
      [1, 1],
      [1, 1],
    ],
    expected: ['1.1.', '1.2.'],
  },
  {
    name: 'non-list paragraphs do not break the sequence',
    items: [[1, 0], {}, [1, 0]],
    expected: ['1.', undefined, '2.'],
  },
  {
    name: 'two instances of one abstract continue one sequence',
    items: [
      [1, 0],
      [2, 0],
      [1, 0],
    ],
    expected: ['1.', '2.', '3.'],
  },
  {
    name: 'startOverride instance restarts the sequence',
    setup: (n) => n.addInstance(0, [{ level: 0, startOverride: 1 }], 9),
    items: [
      [1, 0],
      [1, 0],
      [9, 0],
      [9, 0],
    ],
    expected: ['1.', '2.', '1.', '2.'],
  },
  {
    name: 'startOverride at 10',
    setup: (n) => n.addInstance(0, [{ level: 0, startOverride: 10 }], 9),
    items: [
      [9, 0],
      [9, 0],
    ],
    expected: ['10.', '11.'],
  },
  {
    name: 'restart via Numbering.restart()',
    setup: (n) => {
      expect(n.restart(1).id).toBe(8);
    },
    items: [
      [1, 0],
      [1, 0],
      [8, 0],
      [1, 0],
    ],
    expected: ['1.', '2.', '1.', '2.'],
  },
  {
    name: 'level override definition replaces the level',
    setup: (n) => n.addInstance(0, [{ level: 0, definition: lvl(0, 'upperRoman', '%1:') }], 9),
    items: [
      [9, 0],
      [9, 0],
      [9, 0],
    ],
    expected: ['I:', 'II:', 'III:'],
  },
  {
    name: 'bullets map Symbol PUA to •',
    items: [
      [3, 0],
      [3, 0],
    ],
    expected: ['•', '•'],
  },
  {
    name: 'second bullet level keeps o',
    items: [
      [3, 0],
      [3, 1],
    ],
    expected: ['•', 'o'],
  },
  {
    name: 'legal numbering shows decimal for roman parent',
    items: [
      [4, 0],
      [4, 1],
      [4, 1],
      [4, 0],
      [4, 1],
    ],
    expected: ['Article I.', 'Section 1.1', 'Section 1.2', 'Article II.', 'Section 2.1'],
  },
  {
    name: 'heading styles carry outline numbering',
    items: [
      { styleId: 'Heading1' },
      { styleId: 'Heading2' },
      { styleId: 'Heading2' },
      { styleId: 'Heading1' },
    ],
    expected: ['1', '1.1', '1.2', '2'],
  },
  {
    name: 'heading 2 level comes from the level pStyle link',
    items: [{ styleId: 'Heading2' }],
    expected: ['1.1'],
  },
  {
    name: 'numId 0 removes style numbering',
    items: [
      { styleId: 'Heading1' },
      { styleId: 'Heading1', numbering: { numId: 0, level: 0 } },
      { styleId: 'Heading1' },
    ],
    expected: ['1', undefined, '2'],
  },
  {
    name: 'lvlRestart 0: level never restarts',
    items: [
      [6, 0],
      [6, 1],
      [6, 1],
      [6, 0],
      [6, 1],
    ],
    expected: ['1.', '1)', '2)', '2.', '3)'],
  },
  {
    name: 'lvlRestart 1: restarts after level 0 only',
    items: [
      [6, 0],
      [6, 2],
      [6, 1],
      [6, 2],
      [6, 0],
      [6, 2],
    ],
    expected: ['1.', 'a.', '1)', 'b.', '2.', 'a.'],
  },
  {
    name: 'start value 5 with upperLetter',
    items: [
      [7, 0],
      [7, 0],
    ],
    expected: ['E.', 'F.'],
  },
  {
    name: 'format none renders empty for its placeholder',
    items: [
      [7, 0],
      [7, 2],
    ],
    expected: ['E.', 'E..1'],
  },
  { name: 'level clamped to 0..8', items: [[1, -3]], expected: ['1.'] },
  { name: 'unknown numId yields no label', items: [[42, 0]], expected: [undefined] },
  {
    name: 'decimal past 9',
    items: Array.from({ length: 12 }, () => [1, 0] as Item),
    expected: Array.from({ length: 12 }, (_, i) => `${i + 1}.`),
  },
  {
    name: 'decimalZero format',
    setup: (n) => {
      n.addAbstract({ id: 20, levels: [lvl(0, 'decimalZero', '%1.')] });
      n.addInstance(20, [], 20);
    },
    items: Array.from({ length: 11 }, () => [20, 0] as Item),
    expected: ['01.', '02.', '03.', '04.', '05.', '06.', '07.', '08.', '09.', '10.', '11.'],
  },
  {
    name: 'ordinal text format',
    setup: (n) => {
      n.addAbstract({ id: 20, levels: [lvl(0, 'ordinalText', '%1 item')] });
      n.addInstance(20, [], 20);
    },
    items: [
      [20, 0],
      [20, 0],
      [20, 0],
    ],
    expected: ['First item', 'Second item', 'Third item'],
  },
  {
    name: 'custom zero-padded format',
    setup: (n) => {
      n.addAbstract({ id: 20, levels: [lvl(0, 'custom', '%1', { customFormat: '001, 002, 003, ...' })] });
      n.addInstance(20, [], 20);
    },
    items: [
      [20, 0],
      [20, 0],
    ],
    expected: ['001', '002'],
  },
  {
    name: 'createNumberedList: 1. a. i.',
    setup: (n) => expect(n.createNumberedList().id).toBe(8),
    items: [
      [8, 0],
      [8, 1],
      [8, 2],
      [8, 2],
      [8, 0],
    ],
    expected: ['1.', 'a.', 'i.', 'ii.', '2.'],
  },
  {
    name: 'createMultilevelList legal: 1. 1.1. 1.1.1.',
    setup: (n) => expect(n.createMultilevelList({ legal: true }).id).toBe(8),
    items: [
      [8, 0],
      [8, 1],
      [8, 2],
      [8, 1],
    ],
    expected: ['1.', '1.1.', '1.1.1.', '1.2.'],
  },
  {
    name: 'createBulletList: •, o, ▪',
    setup: (n) => expect(n.createBulletList().id).toBe(8),
    items: [
      [8, 0],
      [8, 1],
      [8, 2],
    ],
    expected: ['•', 'o', '▪'],
  },
  {
    name: 'two independent abstracts keep separate counters',
    items: [
      [1, 0],
      [6, 0],
      [1, 0],
      [6, 0],
    ],
    expected: ['1.', '1.', '2.', '2.'],
  },
];

describe('list label computation', () => {
  it.each(cases)('$name', (c) => {
    const n = numbering();
    c.setup?.(n);
    expect(labels(c.items, n)).toEqual(c.expected);
  });

  it('computeListLabels walks the body in document order including table cells', () => {
    const n = numbering();
    const d = doc(
      p({ numbering: { numId: 1, level: 0 } }, 'one'),
      table(
        {},
        row(
          cell(p({ numbering: { numId: 1, level: 0 } }, 'two')),
          cell(p({ numbering: { numId: 1, level: 1 } }, 'two.one')),
        ),
      ),
      p({ numbering: { numId: 1, level: 0 } }, 'three'),
    );
    const result = computeListLabels(d, { styles: styles(), numbering: n });
    expect([...result.values()].map((l) => l.text)).toEqual(['1.', '2.', '2.1.', '3.']);
    for (const [pos, label] of result) {
      expect(d.nodeAt(pos)?.type.name).toBe('paragraph');
      expect(label.pos).toBe(pos);
    }
  });

  it('labels carry counters, suffix, alignment and label run props', () => {
    const d = doc(
      p({ numbering: { numId: 3, level: 0 } }, 'b'),
      p({ numbering: { numId: 1, level: 1 } }, 'x'),
    );
    const [bullet, numbered] = [
      ...computeListLabels(d, { styles: styles(), numbering: numbering() }).values(),
    ];
    expect(bullet?.isBullet).toBe(true);
    expect(bullet?.rawText).toBe('');
    expect(bullet?.rPr?.fonts?.ascii).toBe('Symbol');
    expect(numbered).toMatchObject({
      counters: [1, 1],
      suffix: 'tab',
      alignment: 'left',
      level: 1,
      abstractId: 0,
    });
  });

  it('returns nothing without numbering', () => {
    expect(computeListLabels(doc(p('a')), { styles: styles() }).size).toBe(0);
  });
});

describe('formatLevelText', () => {
  it('substitutes several placeholders with their formats', () => {
    const levels = [
      lvl(0, 'upperRoman', '%1'),
      lvl(1, 'upperLetter', '%1.%2'),
      lvl(2, 'decimal', '%1.%2.%3'),
    ];
    expect(formatLevelText(levels, 2, [4, 2, 7])).toBe('IV.B.7');
  });
  it('legal numbering forces decimal', () => {
    const levels = [lvl(0, 'upperRoman', '%1'), lvl(1, 'lowerLetter', '%1.%2', { isLegal: true })];
    expect(formatLevelText(levels, 1, [3, 2])).toBe('3.2');
  });
});

describe('number formats', () => {
  it.each([
    [1, 'I'],
    [4, 'IV'],
    [9, 'IX'],
    [14, 'XIV'],
    [40, 'XL'],
    [90, 'XC'],
    [400, 'CD'],
    [1994, 'MCMXCIV'],
    [3999, 'MMMCMXCIX'],
  ])('roman %i = %s', (n, r) => expect(toRoman(n)).toBe(r));

  it.each([
    [1, 'A'],
    [26, 'Z'],
    [27, 'AA'],
    [28, 'BB'],
    [53, 'AAA'],
  ])('letters %i = %s (Word repeats the letter)', (n, s) => expect(toLetters(n)).toBe(s));

  it('cardinal and ordinal text', () => {
    expect(toCardinalText(0)).toBe('zero');
    expect(toCardinalText(21)).toBe('twenty-one');
    expect(toCardinalText(1205)).toBe('one thousand two hundred five');
    expect(toOrdinalText(12)).toBe('twelfth');
    expect(toOrdinalText(20)).toBe('twentieth');
    expect(toOrdinalText(103)).toBe('one hundred third');
  });

  it('ordinal suffixes, hex, chicago, enclosed, full width', () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111].map((n) => formatNumber(n, 'ordinal'))).toEqual([
      '1st',
      '2nd',
      '3rd',
      '4th',
      '11th',
      '12th',
      '13th',
      '21st',
      '22nd',
      '101st',
      '111th',
    ]);
    expect(formatNumber(255, 'hex')).toBe('FF');
    expect([1, 2, 3, 4, 5, 9].map((n) => formatNumber(n, 'chicago'))).toEqual([
      '*',
      '†',
      '‡',
      '§',
      '**',
      '***',
    ]);
    expect(formatNumber(3, 'decimalEnclosedCircle')).toBe('③');
    expect(formatNumber(21, 'decimalEnclosedCircle')).toBe('21');
    expect(formatNumber(12, 'decimalFullWidth')).toBe('１２');
  });

  it('non-Latin formats', () => {
    expect(formatNumber(3, 'russianLower')).toBe('в');
    expect(formatNumber(15, 'hebrew1')).toBe('טו');
    expect(formatNumber(23, 'japaneseCounting')).toBe('二十三');
    expect(formatNumber(10, 'chineseCounting')).toBe('十');
    expect(formatNumber(42, 'hindiNumbers')).toBe('४२');
    expect(formatNumber(7, 'unknownFormat')).toBe('7');
  });

  it('bullet glyph mapping', () => {
    expect(bulletDisplayText('', 'Symbol')).toBe('•');
    expect(bulletDisplayText('', 'Wingdings')).toBe('▪');
    expect(bulletDisplayText('', 'Wingdings')).toBe('➢');
    expect(bulletDisplayText('', 'Wingdings')).toBe('✓');
    expect(bulletDisplayText('–', 'Calibri')).toBe('–');
    expect(bulletDisplayText('', undefined)).toBe('•');
  });
});

describe('Numbering registry', () => {
  it('round-trips JSON and keeps ids stable', () => {
    const n = numbering();
    const copy = Numbering.fromJSON(JSON.parse(JSON.stringify(n.toJSON())));
    expect(copy.toJSON()).toEqual(n.toJSON());
    expect(copy.level(1, 2)?.text).toBe('%1.%2.%3.');
  });

  it('follows numStyleLink through a list style', () => {
    const n = new Numbering();
    n.addAbstract({ id: 0, styleLink: 'MyList', levels: [lvl(0, 'upperLetter', '%1)')] });
    n.addInstance(0, [], 1);
    n.addAbstract({ id: 1, numStyleLink: 'MyList', levels: [] });
    n.addInstance(1, [], 2);
    n.listStyleResolver = (styleId) => (styleId === 'MyList' ? 1 : undefined);
    expect(n.level(2, 0)?.format).toBe('upperLetter');
  });

  it('rejects instances of unknown abstracts', () => {
    expect(() => new Numbering().addInstance(3)).toThrow(/unknown abstractNum/);
  });
});
