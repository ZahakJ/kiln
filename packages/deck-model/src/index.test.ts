import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/deck-model', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/deck-model');
  });
});
