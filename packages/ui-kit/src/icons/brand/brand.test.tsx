import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Button, Icon, TabsList } from '../../index.ts';
import { Tabs } from '../../primitives/Tabs.tsx';
import { AppGlyph, AppTile, BRAND_MARK_SVG, Flame, Spark, tileLevel, tileUrl } from './index.ts';

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

describe('brand glyph set', () => {
  it('is generated from the committed brand kit (tools/import-brand.sh keeps it in sync)', () => {
    expect(() =>
      execFileSync(process.execPath, [here('./build.mjs'), '--check'], { stdio: 'pipe' }),
    ).not.toThrow();
  });

  it('has the five app marks, the spark and the flame', () => {
    expect(Object.keys(BRAND_MARK_SVG).sort()).toEqual([
      'deck',
      'flame',
      'grid',
      'notes',
      'spark',
      'suite',
      'write',
    ]);
  });

  it('renders app marks in currentColor, decorative unless labelled', () => {
    const { container, rerender } = render(<AppGlyph app="write" size={20} />);
    const svg = container.querySelector('svg');
    expect(svg?.getAttribute('aria-hidden')).toBe('true');
    expect(svg?.getAttribute('width')).toBe('20');
    expect(svg?.getAttribute('data-mark')).toBe('write');
    expect(svg?.querySelector('g')?.getAttribute('fill')).toBe('currentColor');
    rerender(<AppGlyph app="home" label="Kiln Home" />);
    const home = container.querySelector('svg');
    expect(home?.getAttribute('role')).toBe('img');
    expect(home?.getAttribute('aria-label')).toBe('Kiln Home');
    expect(home?.getAttribute('data-mark')).toBe('suite');
    expect(home?.querySelector('path')?.getAttribute('fill-rule')).toBe('evenodd');
  });

  it('keeps the flame in fire colours and in proportion', () => {
    const { container } = render(<Flame size={12} />);
    const svg = container.querySelector('svg');
    expect(svg?.getAttribute('height')).toBe('12');
    expect(svg?.getAttribute('width')).toBe('8');
    const fills = [...(svg?.querySelectorAll('path') ?? [])].map((p) => p.getAttribute('fill'));
    expect(fills.length).toBeGreaterThan(0);
    for (const f of fills) expect(f).toMatch(/^#/);
  });

  it('is an Icon source (`brand:*`), and unknown marks fail loudly', () => {
    const { container } = render(<Icon icon="brand:spark" size={14} />);
    expect(container.querySelector('svg')?.getAttribute('data-mark')).toBe('spark');
    expect(() => render(<Icon icon={'brand:nope' as 'brand:spark'} />)).toThrow(/unknown brand mark/);
    render(<Spark />);
  });

  it('picks the tile drawn for the rendered size (BRAND.md §2)', () => {
    expect(tileLevel(16)).toBe('-16');
    expect(tileLevel(20)).toBe('-32');
    expect(tileLevel(40)).toBe('-32');
    expect(tileLevel(48)).toBe('-48');
    expect(tileLevel(96)).toBe('-48');
    expect(tileLevel(128)).toBe('');
    expect(tileUrl('home', 16)).toMatch(/tiles\/suite-16\.svg$/);
    expect(tileUrl('deck', 256)).toMatch(/tiles\/deck\.svg$/);
    const { container } = render(<AppTile app="grid" size={32} />);
    const img = container.querySelector('img');
    expect(img?.getAttribute('src')).toMatch(/tiles\/grid-32\.svg$/);
    expect(img?.getAttribute('alt')).toBe('');
    // The tile files are the kit's, with an accessible title added.
    const tile = readFileSync(here('./tiles/grid-32.svg'), 'utf8');
    expect(tile).toContain('<title>Kiln Grid</title>');
  });
});

describe('kit component variants', () => {
  it('has the ember button for suite-level actions and the arch tabs', () => {
    const { container } = render(
      <>
        <Button variant="ember">New</Button>
        <Tabs defaultValue="a">
          <TabsList variant="arch" aria-label="Ribbon" />
        </Tabs>
      </>,
    );
    expect(container.querySelector('.kiln-btn--ember')).not.toBeNull();
    expect(container.querySelector('.kiln-tabs--arch')).not.toBeNull();
  });
});
