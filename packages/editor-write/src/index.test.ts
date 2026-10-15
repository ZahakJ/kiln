import { describe, expect, it } from 'vitest';
import { PACKAGE_NAME } from './index.ts';

describe('@officina/editor-write', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/editor-write');
  });

  it('runs in a DOM environment', () => {
    const el = document.createElement('div');
    el.textContent = PACKAGE_NAME;
    document.body.append(el);
    expect(document.body.textContent).toContain('@officina/editor-write');
  });
});
