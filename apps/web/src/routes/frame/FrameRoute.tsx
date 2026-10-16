import { type CoreInfo, KilnError } from '@kiln/protocol';
import { useCallback, useEffect, useState } from 'react';
import { useClient } from '../../client/useClient';
import { BridgeError, bridgeName, DEV_ROUTES, DevShell, Fact } from './DevShell';

interface Probe {
  info: CoreInfo;
  roundTripMs: number;
}

/** Shows which core the page talks to, from a `Ping` round trip. */
export function FrameRoute() {
  const state = useClient();
  const [probe, setProbe] = useState<Probe | null>(null);
  const [error, setError] = useState<string | null>(null);

  const ping = useCallback(async () => {
    if (state.status !== 'ready') return;
    try {
      const started = performance.now();
      const info = await state.client.ping();
      setProbe({ info, roundTripMs: performance.now() - started });
      setError(null);
    } catch (e) {
      setError(KilnError.from(e).message);
    }
  }, [state]);

  useEffect(() => {
    void ping();
  }, [ping]);

  const bridge = state.status === 'ready' ? bridgeName(state.client.bridge) : null;
  return (
    <DevShell bridge={bridge}>
      {state.status === 'error' && <BridgeError message={state.error.message} />}
      {error && <BridgeError message={error} />}
      <section aria-labelledby="core-heading" className="flex flex-col gap-2">
        <p className="m-0 text-[12px] font-semibold uppercase tracking-wide text-muted">Core bridge</p>
        <h1
          id="core-heading"
          className="m-0 text-[28px] font-semibold tracking-tight"
          data-testid="core-version"
        >
          {probe
            ? `Kiln core ${probe.info.version}`
            : state.status === 'error'
              ? 'Kiln core unavailable'
              : 'Starting the core…'}
        </h1>
        <p className="m-0 max-w-xl text-[14px] leading-relaxed text-muted">
          The interface talks to the Rust core through one typed protocol. This page checks the round trip;
          the document surfaces arrive with the rendering pipeline.
        </p>
      </section>
      <dl
        className="m-0 rounded-md border border-line bg-surface px-5 py-1.5 shadow-(--kiln-shadow-1)"
        aria-busy={!probe}
      >
        <Fact label="Bridge" value={bridge ?? '—'} />
        <Fact label="Core version" value={probe?.info.version ?? '—'} testId="core-version-value" />
        <Fact label="Protocol" value={probe ? `v${probe.info.protocol}` : '—'} testId="core-protocol" />
        <Fact label="Target" value={probe?.info.target ?? '—'} />
        <Fact label="Round trip" value={probe ? `${probe.roundTripMs.toFixed(1)} ms` : '—'} />
      </dl>
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => void ping()}
          disabled={state.status !== 'ready'}
          className="rounded-md border border-line bg-surface px-3 py-1.5 text-[13px] font-medium text-fg shadow-(--kiln-shadow-1) hover:bg-surface-2 disabled:opacity-50"
        >
          Ping again
        </button>
        <a
          href={DEV_ROUTES.scratch}
          className="text-[13px] font-medium text-hub underline-offset-2 hover:underline"
        >
          Open a scratch document
        </a>
      </div>
    </DevShell>
  );
}
