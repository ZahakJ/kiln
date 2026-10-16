import type { ReactNode } from 'react';

/** The routes this module serves (hash routing, so they work on GitHub Pages too). */
export const DEV_ROUTES = { frame: '#/frame', scratch: '#/scratch' } as const;

/**
 * Chrome of the temporary bridge routes: a quiet bar with the routes and the bridge in use. The
 * real app frame (title bar, ribbon, status bar) is WP-58's.
 */
export function DevShell({ bridge, children }: { bridge: string | null; children: ReactNode }) {
  const here = typeof window === 'undefined' ? '' : window.location.hash;
  const link = (href: string, label: string) => (
    <a
      href={href}
      aria-current={here === href ? 'page' : undefined}
      className="rounded-md px-2.5 py-1 text-[13px] font-medium text-muted no-underline hover:bg-surface-2 hover:text-fg aria-[current=page]:bg-surface-2 aria-[current=page]:text-fg"
    >
      {label}
    </a>
  );
  return (
    <div className="flex min-h-screen flex-col">
      <header className="flex h-12 items-center gap-3 border-b border-line bg-surface px-4 sm:px-6">
        <a href="#/" className="text-[15px] font-semibold tracking-tight text-fg no-underline">
          Kiln
        </a>
        <span aria-hidden="true" className="h-4 w-px bg-line" />
        <nav aria-label="Bridge" className="flex items-center gap-1">
          {link(DEV_ROUTES.frame, 'Core')}
          {link(DEV_ROUTES.scratch, 'Scratch')}
        </nav>
        {bridge && (
          <span
            className="ml-auto inline-flex items-center gap-1.5 rounded-full border border-line bg-surface-2 px-2.5 py-0.5 text-[11px] font-medium text-muted"
            data-testid="bridge-kind"
          >
            <span aria-hidden="true" className="size-1.5 rounded-full bg-(--kiln-accent-grid)" />
            {bridge}
          </span>
        )}
      </header>
      <main className="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-6 px-4 py-8 sm:px-6 sm:py-12">
        {children}
      </main>
    </div>
  );
}

/** A labelled value row. */
export function Fact({ label, value, testId }: { label: string; value: ReactNode; testId?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line py-2.5 last:border-b-0">
      <dt className="text-[13px] text-muted">{label}</dt>
      <dd
        className="m-0 text-right text-[13px] font-medium [font-feature-settings:'tnum']"
        data-testid={testId}
      >
        {value}
      </dd>
    </div>
  );
}

/** The bridge's display name. */
export function bridgeName(bridge: 'wasm' | 'tauri'): string {
  return bridge === 'wasm' ? 'WebAssembly in a Worker' : 'Native (Tauri)';
}

/** An error panel for a bridge that failed to start. */
export function BridgeError({ message }: { message: string }) {
  return (
    <div
      role="alert"
      className="rounded-md border border-line bg-surface p-5 shadow-(--kiln-shadow-1)"
      data-testid="bridge-error"
    >
      <h1 className="m-0 text-[17px] font-semibold">The core did not start</h1>
      <p className="mb-0 mt-2 text-[13px] leading-relaxed text-muted">{message}</p>
    </div>
  );
}
