/**
 * A small W3C WebDriver client for the desktop smoke tests: tauri-driver (which runs the
 * platform's WebDriver server, WebKitWebDriver on Linux) speaks the protocol over HTTP, and these
 * are the few endpoints the tests use.
 */

const ELEMENT = 'element-6066-11e4-a52e-4f735466cecf';

/** WebDriver's code points for the keys the tests press. */
export const Key = {
  Control: '',
  Shift: '',
  End: '',
  Enter: '',
  ArrowLeft: '',
  ArrowRight: '',
  Escape: '',
  F5: '',
} as const;

export class WebDriverError extends Error {
  override name = 'WebDriverError';
}

async function request<T>(base: string, method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${base}${path}`, {
    method,
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = (await res.json().catch(() => ({}))) as { value?: unknown };
  const value = json.value as { error?: string; message?: string } | undefined;
  if (!res.ok || (value && typeof value === 'object' && 'error' in value && value.error)) {
    throw new WebDriverError(
      `${method} ${path}: ${value?.error ?? res.status} ${value?.message ?? ''}`.trim(),
    );
  }
  return json.value as T;
}

/** One session: a running Kiln driven through its webviews. */
export class Session {
  readonly base: string;
  readonly id: string;

  constructor(base: string, id: string) {
    this.base = base;
    this.id = id;
  }

  /** Starts the application with its arguments. */
  static async start(base: string, application: string, args: string[]) {
    const value = await request<{ sessionId: string }>(base, 'POST', '/session', {
      capabilities: { alwaysMatch: { browserName: 'wry', 'tauri:options': { application, args } } },
    });
    return new Session(base, value.sessionId);
  }

  private call<T>(method: string, path: string, body?: unknown): Promise<T> {
    return request<T>(this.base, method, `/session/${this.id}${path}`, body);
  }

  /** Runs a script in the current window and returns its value. */
  run<T>(script: string, ...args: unknown[]): Promise<T> {
    return this.call<T>('POST', '/execute/sync', { script, args });
  }

  /** The first element matching a CSS selector, or `null`. */
  async find(css: string): Promise<string | null> {
    try {
      const v = await this.call<Record<string, string>>('POST', '/element', {
        using: 'css selector',
        value: css,
      });
      return v[ELEMENT] ?? null;
    } catch {
      return null;
    }
  }

  /** The first element matching an XPath, or `null`. */
  async findX(xpath: string): Promise<string | null> {
    try {
      const v = await this.call<Record<string, string>>('POST', '/element', { using: 'xpath', value: xpath });
      return v[ELEMENT] ?? null;
    } catch {
      return null;
    }
  }

  click(element: string): Promise<void> {
    return this.call('POST', `/element/${element}/click`, {});
  }

  /** Types into an element (it takes the focus first). */
  type(element: string, text: string): Promise<void> {
    return this.call('POST', `/element/${element}/value`, { text });
  }

  /** Presses a chord: every key down in order, then up in reverse. */
  async press(...keys: string[]): Promise<void> {
    const down = keys.map((value) => ({ type: 'keyDown', value }));
    const up = [...keys].reverse().map((value) => ({ type: 'keyUp', value }));
    await this.call('POST', '/actions', {
      actions: [{ type: 'key', id: 'keyboard', actions: [...down, ...up] }],
    });
    await this.call('DELETE', '/actions');
  }

  handles(): Promise<string[]> {
    return this.call<string[]>('GET', '/window/handles');
  }

  handle(): Promise<string> {
    return this.call<string>('GET', '/window');
  }

  switchTo(handle: string): Promise<void> {
    return this.call('POST', '/window', { handle });
  }

  /** Closes the current window, as its close button does. */
  async closeWindow(): Promise<void> {
    await this.call('DELETE', '/window');
  }

  /** A PNG of the current window. */
  async screenshot(): Promise<Buffer> {
    return Buffer.from(await this.call<string>('GET', '/screenshot'), 'base64');
  }

  async end(): Promise<void> {
    await this.call('DELETE', '').catch(() => undefined);
  }
}

/** Polls `probe` until it returns a value other than `undefined`/`false`/`null`. */
export async function until<T>(
  what: string,
  probe: () => Promise<T>,
  timeoutMs = 30_000,
): Promise<NonNullable<T>> {
  // A CI runner starts the debug build's windows several times slower than a development machine.
  const end = Date.now() + timeoutMs * (process.env.CI ? 3 : 1);
  let last: unknown = null;
  while (Date.now() < end) {
    try {
      const v = await probe();
      if (v !== undefined && v !== null && v !== false) return v as NonNullable<T>;
    } catch (error) {
      last = error;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new WebDriverError(`timed out waiting for ${what}${last ? ` (${String(last)})` : ''}`);
}
