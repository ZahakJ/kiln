import { describe, expect, it } from 'vitest';
import { isShippable, isTierEnabled, parseTierList, resolveTierConfig } from './tiers.ts';

describe('tiers', () => {
  it('parses tier lists', () => {
    expect(parseTierList('P0,p1')).toEqual(new Set(['P0', 'P1']));
    expect(parseTierList('all')).toEqual(new Set(['P0', 'P1', 'P2']));
    expect(parseTierList('nonsense')).toBeUndefined();
    expect(parseTierList(undefined)).toBeUndefined();
  });

  it('shows everything in development and only P0 in release by default', () => {
    expect(resolveTierConfig({})).toEqual({ tiers: new Set(['P0', 'P1', 'P2']), release: false });
    expect(resolveTierConfig({ PROD: true })).toEqual({ tiers: new Set(['P0']), release: true });
    expect(resolveTierConfig({ NODE_ENV: 'production' }).release).toBe(true);
  });

  it('lets the environment override tiers and release mode', () => {
    const cfg = resolveTierConfig({ PROD: true, VITE_KILN_TIERS: 'P1' });
    expect(cfg.tiers).toEqual(new Set(['P0', 'P1'])); // P0 is never gated away
    expect(resolveTierConfig({ PROD: true, KILN_RELEASE: '0' }).release).toBe(false);
    expect(resolveTierConfig({ KILN_RELEASE: 'yes' }).release).toBe(true);
  });

  it('gates commands by tier and pending state', () => {
    const release = resolveTierConfig({ PROD: true });
    const dev = resolveTierConfig({});
    expect(isTierEnabled(undefined, release)).toBe(true);
    expect(isTierEnabled('P2', release)).toBe(false);
    expect(isShippable({ tier: 'P0', pending: true }, release)).toBe(false);
    expect(isShippable({ tier: 'P0', pending: true }, dev)).toBe(true);
    expect(isShippable({ tier: 'P1' }, dev)).toBe(true);
  });
});
