import type { ErrorInfo } from './generated/ErrorInfo';
import type { ErrorKind } from './generated/ErrorKind';

/** An error from the core: branch on `kind`, never on `message` (which is English, for logs). */
export class KilnError extends Error {
  override readonly name = 'KilnError';
  readonly kind: ErrorKind;

  constructor(info: ErrorInfo) {
    super(info.message);
    this.kind = info.kind;
  }

  /** The wire form. */
  toInfo(): ErrorInfo {
    return { kind: this.kind, message: this.message };
  }

  /**
   * Converts anything thrown by a bridge into a `KilnError`: an `ErrorInfo` object, an `Error`
   * whose message is `ErrorInfo` JSON (the wasm bridge), or anything else (as `Internal`).
   */
  static from(error: unknown): KilnError {
    if (error instanceof KilnError) return error;
    if (isErrorInfo(error)) return new KilnError(error);
    const message =
      error instanceof Error ? error.message : typeof error === 'string' ? error : String(error);
    try {
      const parsed: unknown = JSON.parse(message);
      if (isErrorInfo(parsed)) return new KilnError(parsed);
    } catch {
      // not JSON: fall through
    }
    return new KilnError({ kind: 'Internal', message });
  }
}

const KINDS: readonly ErrorKind[] = [
  'NotFound',
  'InvalidInput',
  'Unavailable',
  'Unsupported',
  'Format',
  'Io',
  'Cancelled',
  'Closed',
  'Internal',
];

/** Whether a value has the shape of an `ErrorInfo`. */
export function isErrorInfo(value: unknown): value is ErrorInfo {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.message === 'string' && KINDS.includes(v.kind as ErrorKind);
}
