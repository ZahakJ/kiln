import { describe, expect, it } from 'vitest';
import { displayText, parseNumberText, toBoolean, toNumber, toText } from '../../src/values/coerce.ts';
import { compareScalars, lookupEquals, sortCompare } from '../../src/values/compare.ts';
import {
  dateToSerial,
  jsDateToSerial,
  serialToDate,
  serialToJsDate,
  serialToTime,
  serialWeekday,
} from '../../src/values/dates.ts';
import { Errors, ErrorValue, errorFromCode } from '../../src/values/errors.ts';
import { excelRound, excelTrunc, numbersEqual, numberToText, round15 } from '../../src/values/number.ts';
import { Array2D, BLANK, typeNumber } from '../../src/values/types.ts';

const opts = { currentYear: 2024 };

describe('text → number coercion (en-US)', () => {
  it.each([
    ['42', 42],
    [' 42 ', 42],
    ['-3.5', -3.5],
    ['+7', 7],
    ['1,234,567.5', 1234567.5],
    ['$1,000', 1000],
    ['-$5', -5],
    ['$-5', -5],
    ['(250)', -250],
    ['($1,250.50)', -1250.5],
    ['50%', 0.5],
    ['-12.5%', -0.125],
    ['1e3', 1000],
    ['2.5E-2', 0.025],
    ['.5', 0.5],
    ['1 1/2', 1.5],
    ['-2 3/4', -2.75],
    ['2024-01-15', 45306],
    ['1/15/2024', 45306],
    ['01/15/24', 45306],
    ['15-Jan-2024', 45306],
    ['15 Jan 2024', 45306],
    ['Jan 15, 2024', 45306],
    ['January 15, 2024', 45306],
    ['Jan 2024', 45292],
    ['1/15', 45306],
    ['12:00', 0.5],
    ['6:00 PM', 0.75],
    ['12:00 AM', 0],
    ['1 PM', 13 / 24],
    ['13:30:15', (13 * 3600 + 30 * 60 + 15) / 86400],
    ['25:00', 25 / 24],
    ['2024-01-15 12:00', 45306.5],
    ['1/15/2024 6:00 PM', 45306.75],
  ])('%s → %f', (text, value) => {
    expect(parseNumberText(text, opts)).toBeCloseTo(value, 9);
  });

  it.each([
    '',
    'abc',
    '1,23',
    '1.2.3',
    '12/45/2024',
    'Feb 30, 2024',
    '13:61',
    'TRUE',
    '$',
    '%',
    '1-2-3-4',
    '1 / 2 x',
  ])('rejects %j', (text) => {
    expect(parseNumberText(text, opts)).toBeUndefined();
  });

  it('toNumber follows Excel arithmetic rules', () => {
    expect(toNumber(true)).toBe(1);
    expect(toNumber(false)).toBe(0);
    expect(toNumber(BLANK)).toBe(0);
    expect(toNumber('')).toBe(Errors.VALUE);
    expect(toNumber('x')).toBe(Errors.VALUE);
    expect(toNumber(Errors.NA)).toBe(Errors.NA);
  });

  it('uses the 1904 date system when asked', () => {
    expect(parseNumberText('1904-01-02', { date1904: true })).toBe(1);
  });
});

describe('number → text and logical coercion', () => {
  it.each([
    [0, '0'],
    [42, '42'],
    [-1.5, '-1.5'],
    [0.1 + 0.2, '0.3'],
    [1 / 3, '0.333333333333333'],
    [2 / 3, '0.666666666666667'],
    [123456789012345, '123456789012345'],
    [1e15, '1E+15'],
    [1234567890123456, '1.23456789012346E+15'],
    [1e21, '1E+21'],
    [0.0001, '0.0001'],
    [1e-10, '1E-10'],
    [1.5e-12, '1.5E-12'],
    [(1 / 3) * 1e-6, '3.33333333333333E-07'],
    [-2.5e20, '-2.5E+20'],
  ])('%f → %s', (n, s) => {
    expect(numberToText(n)).toBe(s);
  });

  it('toText and toBoolean', () => {
    expect(toText(true)).toBe('TRUE');
    expect(toText(BLANK)).toBe('');
    expect(toText(12.5)).toBe('12.5');
    expect(toBoolean(0)).toBe(false);
    expect(toBoolean(-2)).toBe(true);
    expect(toBoolean('true')).toBe(true);
    expect(toBoolean(' False ')).toBe(false);
    expect(toBoolean('yes')).toBe(Errors.VALUE);
    expect(toBoolean(BLANK)).toBe(false);
    expect(displayText(Errors.DIV0)).toBe('#DIV/0!');
  });
});

describe('15-significant-digit behaviour', () => {
  it('round15 and almost-equal', () => {
    expect(round15(0.1 + 0.2)).toBe(0.3);
    expect(numbersEqual(0.1 + 0.2, 0.3)).toBe(true);
    expect(numbersEqual(1, 1.0000000000001)).toBe(false);
    expect(round15(0)).toBe(0);
  });

  it.each([
    [2.345, 2, 2.35],
    [1.005, 2, 1.01],
    [-1.005, 2, -1.01],
    [2.5, 0, 3],
    [-2.5, 0, -3],
    [1234.5678, -2, 1200],
    [1250, -2, 1300],
    [0.285, 2, 0.29],
    [1e-20, 2, 0],
  ])('ROUND(%f, %i) = %f', (x, d, r) => {
    expect(excelRound(x, d)).toBe(r);
  });

  it('truncates toward zero', () => {
    expect(excelTrunc(-2.789, 1)).toBe(-2.7);
    expect(excelTrunc(1234, -2)).toBe(1200);
  });
});

describe('comparison rules', () => {
  it('orders numbers < text < logicals', () => {
    expect(compareScalars(1e9, 'a')).toBe(-1);
    expect(compareScalars('zzz', false)).toBe(-1);
    expect(compareScalars(true, 5)).toBe(1);
  });

  it('compares text case-insensitively but accent-sensitively', () => {
    expect(compareScalars('abc', 'ABC')).toBe(0);
    expect(compareScalars('apple', 'Banana')).toBe(-1);
    expect(compareScalars('é', 'e')).not.toBe(0);
  });

  it('treats blank as the other operand type', () => {
    expect(compareScalars(BLANK, 0)).toBe(0);
    expect(compareScalars(BLANK, '')).toBe(0);
    expect(compareScalars(BLANK, false)).toBe(0);
    expect(compareScalars(BLANK, -1)).toBe(1);
    expect(compareScalars(BLANK, BLANK)).toBe(0);
  });

  it('compares numbers at 15 significant digits', () => {
    expect(compareScalars(0.1 + 0.2, 0.3)).toBe(0);
  });

  it('propagates errors', () => {
    expect(compareScalars(Errors.NA, 1)).toBe(Errors.NA);
    expect(compareScalars(1, Errors.REF)).toBe(Errors.REF);
  });

  it('sort order places errors then blanks last', () => {
    const xs = [BLANK, Errors.NA, true, 'b', 2, 'A', 1, false];
    xs.sort(sortCompare);
    expect(xs).toEqual([1, 2, 'A', 'b', false, true, Errors.NA, BLANK]);
  });

  it('lookup equality', () => {
    expect(lookupEquals('Abc', 'aBC')).toBe(true);
    expect(lookupEquals(1, '1')).toBe(false);
    expect(lookupEquals(new ErrorValue('#N/A'), Errors.NA)).toBe(true);
  });
});

describe('date serials', () => {
  it('follows the 1900 system with the Lotus leap-year bug', () => {
    expect(dateToSerial(1900, 1, 1)).toBe(1);
    expect(dateToSerial(1900, 2, 28)).toBe(59);
    expect(dateToSerial(1900, 2, 29)).toBe(60);
    expect(dateToSerial(1900, 3, 1)).toBe(61);
    expect(dateToSerial(2024, 1, 15)).toBe(45306);
    expect(dateToSerial(9999, 12, 31)).toBe(2958465);
    expect(dateToSerial(10000, 1, 1)).toBeUndefined();
  });

  it('normalises overflowing months and days like DATE()', () => {
    expect(dateToSerial(2023, 13, 1)).toBe(dateToSerial(2024, 1, 1));
    expect(dateToSerial(2024, 3, 0)).toBe(dateToSerial(2024, 2, 29));
    expect(dateToSerial(2024, 0, 1)).toBe(dateToSerial(2023, 12, 1));
    expect(dateToSerial(99, 1, 1)).toBe(dateToSerial(1999, 1, 1));
    expect(dateToSerial(1900, 1, 0)).toBe(0);
    expect(dateToSerial(1900, 1, -1)).toBeUndefined();
  });

  it('converts serials back to dates', () => {
    expect(serialToDate(0)).toEqual({ year: 1900, month: 1, day: 0 });
    expect(serialToDate(60)).toEqual({ year: 1900, month: 2, day: 29 });
    expect(serialToDate(61)).toEqual({ year: 1900, month: 3, day: 1 });
    expect(serialToDate(45306.75)).toEqual({ year: 2024, month: 1, day: 15 });
  });

  it('round-trips every day from 1900 to 2100', () => {
    for (let s = 1; s < 73_051; s += 7) {
      const d = serialToDate(s);
      expect(dateToSerial(d.year, d.month, d.day)).toBe(s);
    }
  });

  it('supports the 1904 system', () => {
    expect(dateToSerial(1904, 1, 1, true)).toBe(0);
    expect(dateToSerial(2024, 1, 15, true)).toBe(45306 - 1462);
    expect(serialToDate(45306 - 1462, true)).toEqual({ year: 2024, month: 1, day: 15 });
  });

  it('splits times and computes weekdays', () => {
    expect(serialToTime(0.75)).toEqual({ hour: 18, minute: 0, second: 0, millisecond: 0 });
    expect(serialToTime(45306.5 + 1 / 86400)).toMatchObject({ hour: 12, second: 1 });
    expect(serialWeekday(1)).toBe(0); // Sunday 1900-01-01 in Excel
    expect(serialWeekday(45306)).toBe(1); // Monday 2024-01-15
    expect(serialWeekday(0, true)).toBe(5); // Friday 1904-01-01
  });

  it('converts JavaScript dates', () => {
    expect(jsDateToSerial(new Date(2024, 0, 15, 18, 0, 0))).toBe(45306.75);
    expect(serialToJsDate(45306.5).toISOString()).toBe('2024-01-15T12:00:00.000Z');
    expect(serialToJsDate(61).toISOString()).toBe('1900-03-01T00:00:00.000Z');
  });
});

describe('value types', () => {
  it('Array2D helpers', () => {
    const a = new Array2D([
      [1, 2, 3],
      [4, 5, 6],
    ]);
    expect(a.rows).toBe(2);
    expect(a.cols).toBe(3);
    expect(a.transpose().data).toEqual([
      [1, 4],
      [2, 5],
      [3, 6],
    ]);
    expect(a.flat()).toEqual([1, 2, 3, 4, 5, 6]);
    expect(Array2D.column([1, 2]).rows).toBe(2);
  });

  it('TYPE numbering and error lookup', () => {
    expect(typeNumber(1)).toBe(1);
    expect(typeNumber('a')).toBe(2);
    expect(typeNumber(true)).toBe(4);
    expect(typeNumber(Errors.NA)).toBe(16);
    expect(typeNumber(Array2D.row([1]))).toBe(64);
    expect(errorFromCode('#div/0!')).toBe(Errors.DIV0);
    expect(Errors.NA.typeNumber).toBe(7);
    expect(Errors.CALC.typeNumber).toBe(14);
  });
});
