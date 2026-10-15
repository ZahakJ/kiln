import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/templates', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/templates');
  });
});
