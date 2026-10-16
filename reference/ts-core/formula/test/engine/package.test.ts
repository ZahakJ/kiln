import { describe, expect, it } from 'vitest';
import * as formula from '../../src/index.ts';

describe('@kiln-ref/formula public API', () => {
  it('exports the package name and the main entry points', () => {
    expect(formula.PACKAGE_NAME).toBe('@kiln-ref/formula');
    for (const name of [
      'Engine',
      'MemoryHost',
      'parse',
      'print',
      'tokenize',
      'shiftFormula',
      'copyFormula',
      'moveFormula',
      'toggleAbsolute',
      'toR1C1',
      'fromR1C1',
      'registerFunction',
      'listFunctions',
      'ErrorValue',
      'Array2D',
      'dateToSerial',
    ]) {
      expect(formula, name).toHaveProperty(name);
    }
  });

  it('works end to end through the package entry', () => {
    const e = new formula.Engine();
    e.addSheet('Sheet1');
    e.setCell('Sheet1', 0, 0, { value: 2 });
    e.setCell('Sheet1', 0, 1, { formula: '=A1*21' });
    e.recalc();
    expect(e.getValue('Sheet1', 0, 1)).toBe(42);
  });
});
