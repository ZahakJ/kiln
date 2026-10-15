import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import rootPackage from '../../../package.json' with { type: 'json' };
import { Hub, SUITE_VERSION } from './Hub.tsx';

afterEach(cleanup);

describe('Hub placeholder', () => {
  it('shows the suite name', () => {
    render(<Hub />);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toContain('Officina');
  });

  it('shows the suite version from the root package.json', () => {
    render(<Hub />);
    expect(SUITE_VERSION).toBe(rootPackage.version);
    expect(screen.getByTestId('suite-version').textContent).toBe(`Version ${rootPackage.version}`);
  });

  it('lists the four apps by their product names', () => {
    render(<Hub />);
    for (const name of ['Scriba', 'Tabula', 'Scena', 'Notula']) {
      expect(screen.getByRole('heading', { level: 3, name })).toBeTruthy();
    }
  });
});
