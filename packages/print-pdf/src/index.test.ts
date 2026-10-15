import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/print-pdf', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/print-pdf');
  });
});
