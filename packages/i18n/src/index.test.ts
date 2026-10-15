import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/i18n', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/i18n');
  });
});
