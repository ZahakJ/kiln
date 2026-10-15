import type { AppId } from '@officina/commands';

export const APP_NAMES: Readonly<Record<AppId, string>> = {
  write: 'Scriba',
  grid: 'Tabula',
  deck: 'Scena',
  notes: 'Notula',
  hub: 'Atrium',
};

/**
 * The Officina mark (the "o" opening into a doorway) on a tile in the app's accent colour.
 * Decorative unless `label` is given.
 */
export function AppMark({ size = 24, label }: { size?: number; label?: string }) {
  return (
    <svg
      className="of-appmark"
      width={size}
      height={size}
      viewBox="0 0 512 512"
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      <rect width="512" height="512" rx="120" fill="var(--of-accent-strong)" />
      <rect width="512" height="512" rx="120" fill="url(#of-appmark-sheen)" />
      <defs>
        <linearGradient id="of-appmark-sheen" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.18" />
          <stop offset="0.55" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d="M348 412.95A170 170 0 1 0 164 412.95V270A92 92 0 0 1 348 270Z" fill="#ffffff" />
    </svg>
  );
}
