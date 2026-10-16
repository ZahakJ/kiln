import { KilnError } from './errors';
import type { CoreInfo } from './generated/CoreInfo';
import type { ExecResult } from './generated/ExecResult';
import { PROTOCOL_VERSION } from './generated/index';

/**
 * Extracts the `Pong` of a `Ping` result and checks that the core speaks the protocol version this
 * UI was generated for.
 */
export function pingInfo(result: ExecResult): CoreInfo {
  const value = result.value;
  if (typeof value !== 'object' || !('Pong' in value)) {
    throw new KilnError({ kind: 'Internal', message: 'Ping did not return Pong' });
  }
  const info = value.Pong;
  if (info.protocol !== PROTOCOL_VERSION) {
    throw new KilnError({
      kind: 'Unsupported',
      message: `the core speaks protocol ${info.protocol}, this interface expects ${PROTOCOL_VERSION}`,
    });
  }
  return info;
}
