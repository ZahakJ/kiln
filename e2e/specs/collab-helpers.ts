/**
 * Helpers of the collaboration specs: a collaboration server started for the tests
 * (`cargo run -p kiln-collab-server`, on a free port, its data under `target/`), and sharing and
 * opening a document through the Share dialog and the link, as a person does.
 */
import { type ChildProcess, spawn } from 'node:child_process';
import { mkdirSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page } from '@playwright/test';

const repo = fileURLToPath(new URL('../..', import.meta.url));

export const TOKEN = 'e2e-collab-token';

/** A running collaboration server. */
export interface CollabServer {
  /** `ws://127.0.0.1:<port>` */
  url: string;
  stop(): Promise<void>;
}

/** Starts a collaboration server for the tests; `stop` ends exactly that process. */
export async function startServer(): Promise<CollabServer> {
  const data = `${repo}target/e2e-collab/${process.pid}-${Date.now()}`;
  mkdirSync(data, { recursive: true });
  const child: ChildProcess = spawn(
    'cargo',
    [
      'run',
      '-q',
      '-p',
      'kiln-collab-server',
      '--',
      '--token',
      TOKEN,
      '--bind',
      '127.0.0.1:0',
      '--data',
      data,
    ],
    {
      cwd: repo,
      env: {
        ...process.env,
        PATH: `${homedir()}/.cargo/bin:${process.env.PATH ?? ''}`,
        CARGO_BUILD_JOBS: '6',
      },
      stdio: ['ignore', 'ignore', 'pipe'],
    },
  );
  const url = await new Promise<string>((resolve, reject) => {
    let err = '';
    const timer = setTimeout(() => reject(new Error(`the server did not start: ${err}`)), 600_000);
    child.stderr?.on('data', (chunk: Buffer) => {
      err += chunk.toString();
      const m = / on (127\.0\.0\.1:\d+) /.exec(err);
      if (m) {
        clearTimeout(timer);
        resolve(`ws://${m[1]}`);
      }
    });
    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`the server exited (${code}): ${err}`));
    });
  });
  return {
    url,
    stop: async () => {
      if (child.exitCode === null) {
        const exited = new Promise((r) => child.once('exit', r));
        child.kill('SIGTERM');
        await Promise.race([exited, new Promise((r) => setTimeout(r, 10_000))]);
        if (child.exitCode === null) child.kill('SIGKILL');
      }
      rmSync(data, { recursive: true, force: true });
    },
  };
}

/** Shares the window's document through the Share dialog; returns the link. */
export async function share(page: Page, server: CollabServer, name: string): Promise<string> {
  await page.getByRole('button', { name: 'Share', exact: true }).click();
  const dialog = page.getByTestId('share-dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByTestId('share-name').fill(name);
  await dialog.getByTestId('share-server').fill(server.url);
  await dialog.getByTestId('share-token').fill(TOKEN);
  await dialog.getByTestId('share-start').click();
  const link = dialog.getByTestId('share-link');
  await expect(link).toHaveValue(/#\/(write|deck)\?doc=[a-z]/, { timeout: 30_000 });
  await expect(dialog.getByTestId('share-status')).toContainText('Connected', { timeout: 30_000 });
  const value = await link.inputValue();
  await dialog.getByRole('button', { name: 'Done' }).click();
  await expect(dialog).toBeHidden();
  return value;
}

/** Opens a shared document's link as `name`. */
export async function join(page: Page, link: string, name: string): Promise<void> {
  await page.goto(link);
  const dialog = page.getByTestId('join-dialog');
  await expect(dialog).toBeVisible({ timeout: 60_000 });
  await dialog.getByTestId('join-name').fill(name);
  await dialog.getByTestId('join-open').click();
  await expect(dialog).toBeHidden();
  await expect(page).toHaveURL(/#\/(write|deck)\?doc=\d+$/, { timeout: 60_000 });
}

/** The window's collaboration state, as the core has it. */
export async function collabState(
  page: Page,
  handle: '__kilnWrite' | '__kilnDeck',
): Promise<{ status: string; peers: { name: string; initials: string }[]; queued: number }> {
  return page.evaluate(async (h) => {
    const w = (
      window as unknown as Record<
        string,
        { doc: number; client: { query(d: number, q: unknown): Promise<unknown> } }
      >
    )[h];
    if (!w) throw new Error('no editor');
    const r = (await w.client.query(w.doc, 'Collab')) as {
      Collab: { status: string; peers: []; queued: number };
    };
    return r.Collab;
  }, handle);
}

/** The bytes of the first `.docx` the page saved to its private file system. */
export async function savedDocx(page: Page): Promise<Buffer> {
  let bytes: number[] | null = null;
  await expect
    .poll(
      async () => {
        bytes = await page.evaluate(async () => {
          try {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('documents');
            // biome-ignore lint/suspicious/noExplicitAny: entries() is not in every DOM typing
            for await (const [name, handle] of (dir as any).entries()) {
              if (!String(name).endsWith('.docx')) continue;
              const file = await (handle as FileSystemFileHandle).getFile();
              return Array.from(new Uint8Array(await file.arrayBuffer()));
            }
            return null;
          } catch {
            return null;
          }
        });
        return bytes !== null && Buffer.from(bytes).lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06])) > 0;
      },
      { message: 'the document is saved', timeout: 20_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

/** One entry of a zip package, inflated. */
export function zipEntry(zip: Buffer, name: string): Buffer | null {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return null;
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const entry = zip.subarray(at + 46, at + 46 + nameLen).toString('utf8');
    if (entry === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 0 ? Buffer.from(data) : inflateRawSync(data);
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}
