/**
 * Tier gating (PLAN.md §1.4, §2). P1/P2 commands have ribbon slots and palette entries from day one,
 * but release builds never show greyed "coming soon" items: they hide commands of disabled tiers and
 * every command that is still only declared (`pending`).
 *
 * Configuration, read at build time (Vite `import.meta.env`) or run time (`process.env`):
 * - `OFFICINA_TIERS` / `VITE_OFFICINA_TIERS`: `P0,P1`, `all`. Default: all tiers in development,
 *   `P0` in release builds.
 * - `OFFICINA_RELEASE` / `VITE_OFFICINA_RELEASE`: `1`/`true` forces a release build, `0`/`false`
 *   forces development. Default: release when Vite's `PROD` is true or `NODE_ENV=production`.
 */
import type { Tier } from './types.ts';

export const TIERS: readonly Tier[] = ['P0', 'P1', 'P2'];

export interface TierConfig {
  /** Tiers whose commands are visible. */
  tiers: ReadonlySet<Tier>;
  /** Release build: pending (unimplemented) commands are hidden. */
  release: boolean;
}

export type EnvLike = Record<string, string | boolean | undefined>;

function isTier(s: string): s is Tier {
  return (TIERS as readonly string[]).includes(s);
}

function truthy(v: string | boolean | undefined): boolean | undefined {
  if (v === undefined || v === '') return undefined;
  if (typeof v === 'boolean') return v;
  const s = v.trim().toLowerCase();
  if (['1', 'true', 'yes', 'on'].includes(s)) return true;
  if (['0', 'false', 'no', 'off'].includes(s)) return false;
  return undefined;
}

/** Parses `P0,P1` / `p0 p2` / `all`. Unknown entries are ignored; an empty result means "not set". */
export function parseTierList(value: string | undefined): Set<Tier> | undefined {
  if (!value) return undefined;
  if (value.trim().toLowerCase() === 'all') return new Set(TIERS);
  const out = new Set<Tier>();
  for (const part of value.split(/[\s,;]+/)) {
    const t = part.trim().toUpperCase();
    if (isTier(t)) out.add(t);
  }
  return out.size > 0 ? out : undefined;
}

/** Merges the Vite build-time env and `process.env` (the former wins). */
export function readEnv(): EnvLike {
  const env: EnvLike = {};
  const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  if (proc?.env) Object.assign(env, proc.env);
  const meta = (import.meta as unknown as { env?: EnvLike }).env;
  if (meta && typeof meta === 'object') Object.assign(env, meta);
  return env;
}

export function resolveTierConfig(env: EnvLike = readEnv()): TierConfig {
  const forced = truthy(env.VITE_OFFICINA_RELEASE ?? env.OFFICINA_RELEASE);
  const release =
    forced ??
    (env.PROD === true || env.PROD === 'true' || env.NODE_ENV === 'production' || env.MODE === 'production');
  const listed = parseTierList(
    (env.VITE_OFFICINA_TIERS as string | undefined) ?? (env.OFFICINA_TIERS as string | undefined),
  );
  // P0 is always on: the core product is never gated away.
  const tiers = listed ?? new Set<Tier>(release ? ['P0'] : TIERS);
  tiers.add('P0');
  return { tiers, release };
}

/** Development defaults: every tier visible, pending commands shown (flagged). */
export const DEV_TIERS: TierConfig = { tiers: new Set(TIERS), release: false };
/** Release defaults: P0 only, pending commands hidden. */
export const RELEASE_TIERS: TierConfig = { tiers: new Set<Tier>(['P0']), release: true };

export function isTierEnabled(tier: Tier | undefined, config: TierConfig): boolean {
  return config.tiers.has(tier ?? 'P0');
}

/** Whether a command may appear in this build at all (before its own `when`). */
export function isShippable(cmd: { tier?: Tier; pending?: boolean }, config: TierConfig): boolean {
  if (!isTierEnabled(cmd.tier, config)) return false;
  if (cmd.pending && config.release) return false;
  return true;
}
