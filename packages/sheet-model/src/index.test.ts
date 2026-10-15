import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/sheet-model', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/sheet-model');
  });
});
