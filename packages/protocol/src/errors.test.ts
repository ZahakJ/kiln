import { describe, expect, it } from 'vitest';
import { isErrorInfo, KilnError } from './errors';

describe('KilnError', () => {
  it('reads ErrorInfo objects, JSON messages and anything else', () => {
    const info = { kind: 'NotFound', message: 'doc#3 not found' } as const;
    expect(KilnError.from(info).kind).toBe('NotFound');
    const fromJs = KilnError.from(new Error(JSON.stringify(info)));
    expect(fromJs.kind).toBe('NotFound');
    expect(fromJs.message).toBe('doc#3 not found');
    expect(fromJs.toInfo()).toEqual(info);
    expect(KilnError.from(fromJs)).toBe(fromJs);
    expect(KilnError.from('boom').kind).toBe('Internal');
    expect(KilnError.from(new Error('not json')).message).toBe('not json');
    expect(KilnError.from(42).message).toBe('42');
  });

  it('recognises only well-formed ErrorInfo', () => {
    expect(isErrorInfo({ kind: 'Io', message: 'x' })).toBe(true);
    expect(isErrorInfo({ kind: 'Nope', message: 'x' })).toBe(false);
    expect(isErrorInfo({ kind: 'Io' })).toBe(false);
    expect(isErrorInfo(null)).toBe(false);
  });
});
