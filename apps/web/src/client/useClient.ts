import { type KilnClient, KilnError } from '@kiln/protocol';
import { useEffect, useState } from 'react';
import { type CoreApp, getClient } from './index';

/** The state of the bridge for a component. */
export type ClientState =
  | { status: 'loading' }
  | { status: 'ready'; client: KilnClient }
  | { status: 'error'; error: KilnError };

/** Boots (or reuses) the page's client and, with `app`, starts that app's web core. */
export function useClient(app?: CoreApp): ClientState {
  const [state, setState] = useState<ClientState>({ status: 'loading' });
  useEffect(() => {
    let live = true;
    getClient(app).then(
      (client) => live && setState({ status: 'ready', client }),
      (error: unknown) => live && setState({ status: 'error', error: KilnError.from(error) }),
    );
    return () => {
      live = false;
    };
  }, [app]);
  return state;
}
