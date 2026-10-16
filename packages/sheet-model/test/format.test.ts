import { describe, expect, it } from 'vitest';
import {
  BUILTIN_NUMFMTS,
  errorValue,
  formatCell,
  formatGeneral,
  isDateFormat,
  isValidFormat,
} from '../src/index.ts';

/**
 * Excel's General format at the standard column width (11 characters). Expectations from Excel's
 * documented behaviour ("General" switches to scientific notation for numbers of 12+ digits, shows at
 * most 11 characters, rounds) and checked against Excel 365.
 */
const GENERAL: [number, string][] = [
  [0, '0'],
  [1, '1'],
  [-1, '-1'],
  [42, '42'],
  [3.5, '3.5'],
  [-3.25, '-3.25'],
  [0.1, '0.1'],
  [0.1 + 0.2, '0.3'],
  [1 / 3, '0.333333333'],
  [2 / 3, '0.666666667'],
  [-1 / 3, '-0.333333333'],
  [10 / 3, '3.333333333'],
  [123.456, '123.456'],
  [123456789, '123456789'],
  [1234567890, '1234567890'],
  [12345678901, '12345678901'],
  [99999999999, '99999999999'],
  [123456789012, '1.23457E+11'],
  [999999999999, '1E+12'],
  [99999999999.6, '1E+11'],
  [1234567890.5, '1234567891'],
  [1234567890.4, '1234567890'],
  [-1234567890.5, '-1234567891'],
  [123456.7890123, '123456.789'],
  [1e15, '1E+15'],
  [1.5e20, '1.5E+20'],
  [-2.5e30, '-2.5E+30'],
  [1e100, '1E+100'],
  [1.23456789e100, '1.2346E+100'],
  [0.0001, '0.0001'],
  [0.000123456789, '0.000123457'],
  [0.00001, '0.00001'],
  [0.00001234, '0.00001234'],
  [0.0000123456789, '1.23457E-05'],
  [0.000001, '0.000001'],
  [1.234e-7, '1.234E-07'],
  [1e-10, '1E-10'],
  [-1e-10, '-1E-10'],
  [9.99999999999, '10'],
  [0.99999999999, '1'],
  [45000.5, '45000.5'],
  [Math.PI, '3.141592654'],
  [Math.E, '2.718281828'],
  [1e-300, '1E-300'],
];

describe('General', () => {
  it.each(GENERAL)('%d → %s', (v, expected) => {
    expect(formatGeneral(v)).toBe(expected);
    expect(formatCell(v, 'General').text).toBe(expected);
  });

  it('uses the locale decimal separator', () => {
    expect(formatCell(1234.5, 'General', { locale: 'de-DE' }).text).toBe('1234,5');
    expect(formatCell(0.25, 'General', { locale: 'fr' }).text).toBe('0,25');
  });

  it('honours narrower widths', () => {
    expect(formatGeneral(123456.789, 6)).toBe('123457');
    expect(formatGeneral(1234567, 6)).toBe('1E+06');
    expect(formatGeneral(0.123456, 5)).toBe('0.123');
  });

  it('right-aligns numbers and left-aligns text', () => {
    expect(formatCell(5).align).toBe('right');
    expect(formatCell('abc').align).toBe('left');
    expect(formatCell(true).align).toBe('center');
    expect(formatCell(errorValue('#DIV/0!')).align).toBe('center');
  });
});

/** [format code, value, expected text] — numbers. */
const NUMBERS: [string, number, string][] = [
  ['0', 1234.567, '1235'],
  ['0', -0.4, '-0'],
  ['0.0', -0.04, '-0.0'],
  ['0;(0)', -0.4, '(0)'],
  ['0', 0.5, '1'],
  ['0', 2.5, '3'],
  ['0.00', 1234.567, '1234.57'],
  ['0.00', 0.005, '0.01'],
  ['0.00', -1.005, '-1.01'],
  ['#,##0', 1234567.89, '1,234,568'],
  ['#,##0', -1234, '-1,234'],
  ['#,##0', 0, '0'],
  ['#,##0.00', 1234.5, '1,234.50'],
  ['#,##0.00', 0.1, '0.10'],
  ['#,##0,', 1234567, '1,235'],
  ['#,##0,,', 1234567890, '1,235'],
  ['0.0,,"M"', 12345678, '12.3M'],
  ['#.##', 1.5, '1.5'],
  ['#.##', 0.5, '.5'],
  ['#.00', 0.5, '.50'],
  ['0.0#', 1, '1.0'],
  ['0.0#', 1.234, '1.23'],
  ['00000', 123, '00123'],
  ['000-00-0000', 123456789, '123-45-6789'],
  ['(###) ###-####', 5551234567, '(555) 123-4567'],
  ['"$"#,##0.00', 1234.5, '$1,234.50'],
  ['"$"#,##0.00', -1234.5, '-$1,234.50'],
  ['$#,##0.00', 99.999, '$100.00'],
  ['"Total: "0', 3, 'Total: 3'],
  ['0" units"', 12, '12 units'],
  ['0 "kg"', 5, '5 kg'],
  ['\\$0.00', 3, '$3.00'],
  ['0%', 0.256, '26%'],
  ['0%', 1, '100%'],
  ['0.00%', 0.12345, '12.35%'],
  ['0.0%', -0.05, '-5.0%'],
  ['0%', 0, '0%'],
  ['0.00E+00', 12345, '1.23E+04'],
  ['0.00E+00', 0.000123, '1.23E-04'],
  ['0.0E+0', 1234567, '1.2E+6'],
  ['##0.0E+0', 12345, '12.3E+3'],
  ['##0.0E+0', 1234567, '1.2E+6'],
  ['0.00E+00', -5, '-5.00E+00'],
  ['# ?/?', 1.25, '1 1/4'],
  ['# ?/?', 0.5, ' 1/2'],
  ['# ?/?', 3, '3    '],
  ['# ??/??', 2.3125, '2  5/16'],
  ['# ???/???', Math.PI, '3  16/113'],
  ['?/?', 0.75, '3/4'],
  ['# ?/4', 1.3, '1 1/4'],
  ['# ?/8', 2.6, '2 5/8'],
  ['# ??/100', 0.37, ' 37/100'],
  ['0;-0;"zero"', 0, 'zero'],
  ['0;-0;"zero"', -3, '-3'],
  ['0;(0)', -3, '(3)'],
  ['0.00;[Red]-0.00', -2, '-2.00'],
  ['#,##0_);(#,##0)', 1000, '1,000 '],
  ['#,##0_);(#,##0)', -1000, '(1,000)'],
  ['[<0]"neg";[>0]"pos";"zero"', -1, 'neg'],
  ['[<0]"neg";[>0]"pos";"zero"', 1, 'pos'],
  ['[<0]"neg";[>0]"pos";"zero"', 0, 'zero'],
  ['[>=1000]#,##0,"K";0', 2500, '3K'],
  ['[>=1000]#,##0,"K";0', 999, '999'],
  ['[=1]"one";[=2]"two";0', 2, 'two'],
  ['0;-0;;@', 0, ''],
  [';;;', 123, ''],
  ['0.0', 1e15, '1000000000000000.0'],
  ['#', 0, ''],
  ['?', 0, ' '],
  ['0.???', 1.5, '1.5  '],
  ['???.???', 12.34, ' 12.34 '],
];

describe('number formats', () => {
  it.each(NUMBERS)('%s with %d → %j', (code, v, expected) => {
    expect(formatCell(v, code).text).toBe(expected);
  });
});

/** Dates and times (1900 system). 45000 = 2023-03-15; 45000.75 = 18:00. */
const DATES: [string, number, string][] = [
  ['m/d/yyyy', 45000, '3/15/2023'],
  ['mm/dd/yyyy', 45000, '03/15/2023'],
  ['d-mmm-yy', 45000, '15-Mar-23'],
  ['d-mmm', 45000, '15-Mar'],
  ['mmm-yy', 45000, 'Mar-23'],
  ['mmmm d, yyyy', 45000, 'March 15, 2023'],
  ['dddd', 45000, 'Wednesday'],
  ['ddd', 45000, 'Wed'],
  ['dddd, mmmm dd, yyyy', 45000, 'Wednesday, March 15, 2023'],
  ['yyyy-mm-dd', 45000, '2023-03-15'],
  ['mmmmm', 45000, 'M'],
  ['yy', 45000, '23'],
  ['h:mm AM/PM', 45000.75, '6:00 PM'],
  ['h:mm:ss AM/PM', 0.5, '12:00:00 PM'],
  ['h:mm AM/PM', 0, '12:00 AM'],
  ['h:mm', 45000.75, '18:00'],
  ['hh:mm:ss', 0.123456, '02:57:47'],
  ['h:mm:ss', 1 / 86400, '0:00:01'],
  ['[h]:mm:ss', 1.5, '36:00:00'],
  ['[h]:mm', 2.25, '54:00'],
  ['[mm]:ss', 0.5, '720:00'],
  ['mm:ss', 0.000694444, '01:00'],
  ['mm:ss.0', 0.0000115740741, '00:01.0'],
  ['m/d/yyyy h:mm', 45000.5, '3/15/2023 12:00'],
  ['yyyy-mm-dd hh:mm:ss', 45000.999988426, '2023-03-15 23:59:59'],
  ['m/d/yyyy', 1, '1/1/1900'],
  ['m/d/yyyy', 60, '2/29/1900'],
  ['m/d/yyyy', 61, '3/1/1900'],
  ['m/d/yyyy', 2958465, '12/31/9999'],
  ['dd/mm/yyyy', 36526, '01/01/2000'],
  ['d mmmm yyyy', 44927, '1 January 2023'],
  ['h AM/PM', 0.25, '6 AM'],
];

describe('dates and times', () => {
  it.each(DATES)('%s with %d → %j', (code, v, expected) => {
    const r = formatCell(v, code);
    expect(r.text).toBe(expected);
    expect(r.kind).toBe('date');
  });

  it('shows ##### for negative and out-of-range dates', () => {
    expect(formatCell(-1, 'm/d/yyyy').overflow).toBe(true);
    expect(formatCell(2958466, 'm/d/yyyy').overflow).toBe(true);
    expect(formatCell(-1, 'm/d/yyyy').runs).toEqual([{ type: 'fill', char: '#' }]);
  });

  it('supports the 1904 date system', () => {
    expect(formatCell(0, 'm/d/yyyy', { date1904: true }).text).toBe('1/1/1904');
    expect(formatCell(43538, 'yyyy-mm-dd', { date1904: true }).text).toBe('2023-03-15');
  });

  it('localises month and day names', () => {
    expect(formatCell(45000, 'dddd d. mmmm yyyy', { locale: 'de' }).text).toBe('Mittwoch 15. März 2023');
    expect(formatCell(45000, 'mmmm', { locale: 'fr' }).text).toBe('mars');
  });

  it('classifies date formats', () => {
    expect(isDateFormat('m/d/yyyy')).toBe(true);
    expect(isDateFormat('[h]:mm')).toBe(true);
    expect(isDateFormat('0.00')).toBe(false);
    expect(isDateFormat('General')).toBe(false);
    expect(isDateFormat('"d"0')).toBe(false);
  });
});

describe('text, booleans, errors', () => {
  it.each([
    ['@', 'hello', 'hello'],
    ['"Name: "@', 'Ann', 'Name: Ann'],
    ['0;-0;0;"<"@">"', 'x', '<x>'],
    ['0.00', 'text', 'text'],
    ['General', 'abc', 'abc'],
    ['@', '', ''],
  ])('%s with %j → %j', (code, v, expected) => {
    expect(formatCell(v, code).text).toBe(expected);
  });

  it('formats numbers with a text format as General text', () => {
    const r = formatCell(12.5, '@');
    expect(r.text).toBe('12.5');
    expect(r.align).toBe('left');
  });

  it.each([
    [true, 'TRUE'],
    [false, 'FALSE'],
    [errorValue('#N/A'), '#N/A'],
    [errorValue('#DIV/0!'), '#DIV/0!'],
    [errorValue('#SPILL!'), '#SPILL!'],
    [null, ''],
  ] as const)('%o → %s', (v, expected) => {
    expect(formatCell(v, '0.00').text).toBe(expected);
  });
});

describe('colours', () => {
  it.each([
    ['[Red]0', 5, '#FF0000'],
    ['[Blue]0;[Red]-0', -5, '#FF0000'],
    ['[Blue]0;[Red]-0', 5, '#0000FF'],
    ['[Green]0', 1, '#00FF00'],
    ['[Magenta]0', 1, '#FF00FF'],
    ['[Cyan]0', 1, '#00FFFF'],
    ['[Yellow]0', 1, '#FFFF00'],
    ['[White]0', 1, '#FFFFFF'],
    ['[Black]0', 1, '#000000'],
    ['[Color 10]0', 1, '#008000'],
    ['[Color 3]0', 1, '#FF0000'],
    ['#,##0.00_);[Red](#,##0.00)', -1, '#FF0000'],
  ] as const)('%s with %d → %s', (code, v, color) => {
    expect(formatCell(v, code).color).toBe(color);
  });

  it('has no colour when the section has none', () => {
    expect(formatCell(5, '0;[Red]-0').color).toBeUndefined();
    expect(formatCell(5, '0').color).toBeUndefined();
  });

  it('colours text through the text section', () => {
    expect(formatCell('x', '0;0;0;[Blue]@').color).toBe('#0000FF');
  });
});

describe('accounting layout', () => {
  const ACC = BUILTIN_NUMFMTS[44] as string;

  it('separates the currency symbol from the number with a fill run', () => {
    const r = formatCell(1234.5, ACC);
    expect(r.runs).toEqual([
      { type: 'pad', char: '(' },
      { type: 'text', text: '$' },
      { type: 'fill', char: ' ' },
      { type: 'text', text: '1,234.50' },
      { type: 'pad', char: ')' },
    ]);
    expect(r.text).toBe(' $1,234.50 ');
  });

  it('puts negatives in parentheses without padding', () => {
    const r = formatCell(-1234.5, ACC);
    expect(r.text).toBe(' $(1,234.50)');
    expect(r.runs.at(-1)).toEqual({ type: 'text', text: '(1,234.50)' });
  });

  it('shows a dash for zero', () => {
    expect(formatCell(0, ACC).text).toBe(' $-   ');
  });

  it('pads text in the text section', () => {
    expect(formatCell('n/a', ACC).text).toBe(' n/a ');
  });

  it.each([
    [41, 1234, ' 1,234 '],
    [41, -1234, ' (1,234)'],
    [42, 1234, ' $1,234 '],
    [43, 0.5, ' 0.50 '],
    [43, 0, ' -   '],
    [37, -5, '(5)'],
    [38, 5, '5 '],
    [39, 1.5, '1.50 '],
    [40, -1.5, '(1.50)'],
  ] as const)('built-in %i with %d → %j', (id, v, expected) => {
    expect(formatCell(v, BUILTIN_NUMFMTS[id] as string).text).toBe(expected);
  });

  it('repeats an explicit fill character', () => {
    const r = formatCell(5, '**0');
    expect(r.runs).toEqual([
      { type: 'fill', char: '*' },
      { type: 'text', text: '5' },
    ]);
  });
});

describe('built-in formats', () => {
  it.each([
    [0, 1234.5, '1234.5'],
    [1, 1234.5, '1235'],
    [2, 1234.5, '1234.50'],
    [3, 1234.5, '1,235'],
    [4, 1234.5, '1,234.50'],
    [5, -1234.5, '($1,235)'],
    [6, 1234.5, '$1,235 '],
    [7, 1234.5, '$1,234.50 '],
    [8, -1234.5, '($1,234.50)'],
    [9, 0.5, '50%'],
    [10, 0.5, '50.00%'],
    [11, 1234.5, '1.23E+03'],
    [12, 0.5, ' 1/2'],
    [13, 0.25, '  1/4 '],
    [14, 45000, '3/15/2023'],
    [15, 45000, '15-Mar-23'],
    [16, 45000, '15-Mar'],
    [17, 45000, 'Mar-23'],
    [18, 0.75, '6:00 PM'],
    [19, 0.75, '6:00:00 PM'],
    [20, 0.75, '18:00'],
    [21, 0.75, '18:00:00'],
    [22, 45000.75, '3/15/2023 18:00'],
    [45, 0.001, '01:26'],
    [46, 1.25, '30:00:00'],
    [47, 0.001, '01:26.4'],
    [48, 12345, '12.3E+3'],
    [49, 12, '12'],
  ] as const)('numFmtId %i with %d → %j', (id, v, expected) => {
    expect(formatCell(v, BUILTIN_NUMFMTS[id] as string).text).toBe(expected);
  });
});

describe('locales', () => {
  it.each([
    ['#,##0.00', 1234.5, 'de', '1.234,50'],
    ['#,##0.00', 1234.5, 'fr', '1 234,50'],
    ['0.00%', 0.5, 'de', '50,00%'],
    ['#,##0', 1234567, 'en-GB', '1,234,567'],
    ['0.0', 2.25, 'es', '2,3'],
  ] as const)('%s %d in %s → %j', (code, v, locale, expected) => {
    expect(formatCell(v, code, { locale }).text.replace(/ | /g, ' ')).toBe(expected);
  });
});

describe('invalid formats', () => {
  it('fall back to General', () => {
    expect(isValidFormat('0.0.0"')).toBe(false);
    expect(formatCell(1.5, '0.0.0"').text).toBe('1.5');
    expect(isValidFormat('General')).toBe(true);
    expect(isValidFormat('#,##0.00')).toBe(true);
  });
});
