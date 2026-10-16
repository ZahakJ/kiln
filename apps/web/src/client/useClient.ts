import { type KilnClient, KilnError } from '@kiln/protocol';
import { useEffect, useState } from 'react';
import { getClient } from './index';

/** The state of the bridge for a component. */
export type ClientState =
  | { status: 'loading' }
  | { status: 'ready'; client: KilnClient }
  | { status: 'error'; error: KilnError };

/** Boots (or reuses) the page's client. */
export function useClient(): ClientState {
  const [state, setState] = useState<ClientState>({ status: 'loading' });
  useEffect(() => {
    let live = true;
    getClient().then(
      (client) => live && setState({ status: 'ready', client }),
      (error: unknown) => live && setState({ status: 'error', error: KilnError.from(error) }),
    );
    return () => {
      live = false;
    };
  }, []);
  return state;
}
