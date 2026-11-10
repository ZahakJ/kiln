/**
 * The desktop smoke test, run with `xvfb-run -a pnpm --filter @kiln/e2e tauri-smoke` on
 * Linux: tauri-driver starts the built `kiln` and drives its windows through WebKitWebDriver.
 *
 * 1. `kiln <file.docx>` opens the document in Kiln Write (the command line, as a file manager's
 *    Open With does); typing and Ctrl+S save it in place, and the file on disk has the new text.
 * 2. Ctrl+N and File ▸ Open in that window open windows of their own; the document with unsaved
 *    changes keeps its window, and closing it asks about the changes.
 * 3. Presenter View on a presentation opens the audience's window on the same core: a step in
 *    Presenter View moves the show there, and ending the show closes it.
 *
 * The app runs with its data in a temporary folder (`KILN_DATA_DIR`) and apart from a Kiln that
 * may be running (`--new-instance`). Screenshots of each step go to `e2e/screenshots/tauri/`.
 */
import { type ChildProcess, spawn, spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { Key, Session, until } from './webdriver.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shots = repo('e2e/screenshots/tauri');
const PORT = Number(process.env.KILN_TAURI_DRIVER_PORT ?? 4444);
const BASE = `http://127.0.0.1:${PORT}`;

/** The application under test: `KILN_BIN`, else the newest of the dist, release and debug builds. */
function application(): string {
  if (process.env.KILN_BIN) return process.env.KILN_BIN;
  const found = ['target/dist/kiln', 'target/release/kiln', 'target/debug/kiln']
    .map(repo)
    .filter((p) => existsSync(p))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  if (!found[0]) throw new Error('no kiln binary: build it with `pnpm --filter @kiln/desktop tauri build`');
  return found[0];
}

/** One entry of a zip package, inflated (enough for a .docx). */
function zipEntry(zip: Buffer, name: string): Buffer | null {
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
    if (zip.subarray(at + 46, at + 46 + nameLen).toString('utf8') === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 0 ? Buffer.from(data) : inflateRawSync(data);
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}

/** tauri-driver, whose environment the app inherits (its data in `work`). */
async function startDriver(work: string): Promise<ChildProcess> {
  const driver = spawn('tauri-driver', ['--port', String(PORT)], {
    stdio: ['ignore', 'inherit', 'inherit'],
    env: { ...process.env, KILN_DATA_DIR: join(work, 'data') },
  });
  await until('tauri-driver', async () => (await fetch(`${BASE}/status`)).ok, 15_000);
  return driver;
}

/**
 * Asks the window whose title starts with `title` to close, as its close button does: an X11
 * WM_DELETE_WINDOW message, which the app answers with CloseRequested. WebDriver's own close tears
 * the webview down without asking the app, so it cannot show the question about unsaved changes.
 */
function requestClose(title: string): void {
  const script = `
import sys
from Xlib import X, display, protocol
d = display.Display()
proto, delete = d.intern_atom('WM_PROTOCOLS'), d.intern_atom('WM_DELETE_WINDOW')
net_name, utf8 = d.intern_atom('_NET_WM_NAME'), d.intern_atom('UTF8_STRING')
def walk(w):
    yield w
    for c in w.query_tree().children:
        yield from walk(c)
def title(w):
    p = w.get_full_property(net_name, utf8)
    return p.value.decode('utf-8', 'replace') if p else ''
for w in walk(d.screen().root):
    if title(w).startswith(sys.argv[1]) and delete in (w.get_wm_protocols() or []):
        w.send_event(protocol.event.ClientMessage(window=w, client_type=proto, data=(32, [delete, X.CurrentTime, 0, 0, 0])))
        d.flush()
        sys.exit(0)
sys.exit(1)
`;
  const r = spawnSync('python3', ['-c', script, title], { stdio: 'inherit' });
  if (r.status !== 0) throw new Error(`no window titled "${title}…" to close`);
}

async function shot(s: Session, name: string): Promise<void> {
  mkdirSync(shots, { recursive: true });
  writeFileSync(join(shots, name), await s.screenshot());
}

function check(ok: boolean, what: string): void {
  if (!ok) throw new Error(what);
  console.log(`  ok  ${what}`);
}

/** Waits for the window to show a document of `app` and returns its route. */
function editorRoute(s: Session, app: string): Promise<string> {
  return until(`a ${app} document`, async () => {
    const hash = await s.run<string>('return window.location.hash');
    return new RegExp(`^#/${app}\\?doc=\\d+`).test(hash) ? hash : null;
  });
}

async function typeAndSave(app: string, work: string): Promise<void> {
  console.log('kiln <file.docx>: type, Ctrl+S, the file changes');
  const file = join(work, 'Smoke test.docx');
  copyFileSync(repo('fixtures/docx/text/carlito_left_11pt.docx'), file);
  const before = readFileSync(file);
  const s = await Session.start(BASE, app, ['--new-instance', file]);
  try {
    await editorRoute(s, 'write');
    const proxy = await until('the document', () => s.find('[data-testid="kiln-input-proxy"]'), 60_000);
    await until('the document title', async () =>
      ((await s.run<string>('return document.title')) ?? '').includes('Smoke test'),
    );
    await shot(s, 'write-opened.png');
    await s.run('document.querySelector(\'[data-testid="kiln-input-proxy"]\').focus()');
    await s.press(Key.Control, Key.End);
    await s.type(proxy, ' Typed in the Kiln desktop app.');
    await new Promise((r) => setTimeout(r, 500));
    await s.press(Key.Control, 's');
    const after = await until(
      'the file to be saved',
      async () => {
        const now = readFileSync(file);
        return now.equals(before) ? null : now;
      },
      30_000,
    );
    await shot(s, 'write-saved.png');
    check(!after.equals(before), 'the file on disk changed');
    const xml = zipEntry(after, 'word/document.xml')?.toString('utf8') ?? '';
    check(xml.includes('Typed in the Kiln desktop app.'), 'the saved document has the typed text');
    check(zipEntry(after, '[Content_Types].xml') !== null, 'the saved file is a Word package');
  } finally {
    await s.end();
  }
}

/**
 * Chinese, Japanese, Korean and emoji in a document draw with the platform's own fonts (the
 * desktop build bundles none for them): the screenshot shows real glyphs, not boxes.
 */
async function cjkAndEmojiDraw(app: string, work: string): Promise<void> {
  console.log('CJK and emoji draw with the platform fonts');
  const file = join(work, 'CJK and emoji.docx');
  copyFileSync(repo('fixtures/docx/features/cjk-document.docx'), file);
  const s = await Session.start(BASE, app, ['--new-instance', file]);
  try {
    await editorRoute(s, 'write');
    const proxy = await until('the document', () => s.find('[data-testid="kiln-input-proxy"]'), 60_000);
    await s.run('document.querySelector(\'[data-testid="kiln-input-proxy"]\').focus()');
    await s.press(Key.Control, Key.End);
    await s.type(proxy, ' 한국어 日本語 中文 かな ');
    // WebKitWebDriver types no character outside the Basic Multilingual Plane: the emoji go in as
    // the page's own text input does it.
    await s.run(
      "document.querySelector('[data-testid=\"kiln-input-proxy\"]').focus(); document.execCommand('insertText', false, '😀 🎉 ✅ ⭐ 🇯🇵 👨\u200d👩\u200d👧')",
    );
    await new Promise((r) => setTimeout(r, 1500));
    await shot(s, 'write-cjk-emoji.png');
    check(true, 'the document with CJK and emoji is drawn (write-cjk-emoji.png)');
  } finally {
    await s.end();
  }
}

/**
 * New and Open in a document window open their own windows, so the document with unsaved changes
 * keeps its window; closing that window asks about the changes instead of losing them.
 */
async function newAndOpenKeepTheirWindows(app: string, work: string): Promise<void> {
  console.log('New and Open in a document window: the document keeps its window');
  const file = join(work, 'Keep me.docx');
  copyFileSync(repo('fixtures/docx/text/carlito_left_11pt.docx'), file);
  const s = await Session.start(BASE, app, ['--new-instance', file]);
  try {
    const route = await editorRoute(s, 'write');
    const first = await s.handle();
    const proxy = await until('the document', () => s.find('[data-testid="kiln-input-proxy"]'), 60_000);
    await s.run('document.querySelector(\'[data-testid="kiln-input-proxy"]\').focus()');
    await s.type(proxy, ' Unsaved words.');
    await new Promise((r) => setTimeout(r, 500));
    // Ctrl+N: a new document in a window of its own.
    await s.press(Key.Control, 'n');
    const two = await until('the new document window', async () => {
      const h = await s.handles();
      return h.length === 2 ? h : null;
    });
    await s.switchTo(first);
    check(
      (await s.run<string>('return window.location.hash')) === route,
      'Ctrl+N leaves the document in its window',
    );
    const fresh = two.find((h) => h !== first) ?? '';
    await s.switchTo(fresh);
    await editorRoute(s, 'write');
    check(true, 'the new document has a window of its own');
    await shot(s, 'new-window.png');
    // File ▸ Open ▸ a recent file: a window of its own too.
    await s.switchTo(first);
    await s.click(await until('File', () => s.findX("//button[normalize-space()='File']")));
    await s.click(await until('Open', () => s.findX("//nav//button[normalize-space()='Open']")));
    const recent = await until('a recent file', () =>
      s.findX(
        "//*[contains(@class,'kiln-bs-open')]//*[self::button or self::a or @role='row' or @role='option'][contains(normalize-space(),'Smoke test')]",
      ),
    );
    await s.click(recent);
    await until('the opened document window', async () => (await s.handles()).length === 3);
    check(
      (await s.run<string>('return window.location.hash')) === route,
      'File ▸ Open leaves the document in its window',
    );
    // Closing the window with unsaved changes asks first.
    await s.press(Key.Escape);
    await s.switchTo(first);
    requestClose('Keep me');
    const asked = await until(
      'the question about unsaved changes',
      () => s.find('[data-testid="save-changes"]'),
      15_000,
    );
    check(asked !== null, 'closing the window asks about the unsaved changes');
    // The dialog fades in; the picture is taken once it is there.
    await new Promise((r) => setTimeout(r, 600));
    await shot(s, 'save-changes-on-close.png');
  } finally {
    await s.end();
  }
}

async function presenterView(app: string, work: string): Promise<void> {
  console.log('Presenter View: a second window on the same core');
  const file = join(work, 'Talk.pptx');
  copyFileSync(repo('fixtures/pptx/features/notes.pptx'), file);
  const s = await Session.start(BASE, app, ['--new-instance', file]);
  try {
    await editorRoute(s, 'deck');
    const presenter = await s.handle();
    const tab = await until(
      'the Slide Show tab',
      () => s.findX("//*[@role='tab'][normalize-space()='Slide Show']"),
      60_000,
    );
    await s.click(tab);
    const use = await until('Use Presenter View', () =>
      s.findX("//button[@aria-label='Use Presenter View' or normalize-space()='Use Presenter View']"),
    );
    await s.click(use);
    const begin = await until('From Beginning', () =>
      s.findX("//button[@aria-label='From Beginning' or normalize-space()='From Beginning']"),
    );
    await s.click(begin);
    await until('Presenter View', () => s.find('[data-testid="deck-presenter"]'), 30_000);
    const handles = await until('the audience window', async () => {
      const h = await s.handles();
      return h.length >= 2 ? h : null;
    });
    check(handles.length === 2, 'Presenter View opened a second window');
    const audience = handles.find((h) => h !== presenter) ?? '';
    await s.switchTo(audience);
    check(
      /#\/present\?doc=\d+&view=audience/.test(await s.run<string>('return window.location.hash')),
      'the second window shows the audience view of the same document',
    );
    const slide = () =>
      s.run<string | null>(
        'return document.querySelector(\'[data-testid="deck-show"]\')?.getAttribute("data-slide") ?? null',
      );
    await until('the show in the audience window', async () => (await slide()) === '0', 30_000);
    await shot(s, 'presenter-audience.png');
    await s.switchTo(presenter);
    await shot(s, 'presenter-view.png');
    await s.press(Key.ArrowRight);
    await s.switchTo(audience);
    await until('the audience window to follow the presenter', async () => (await slide()) === '1');
    check(true, 'a step in Presenter View moves the audience window (Event::Show)');
    await s.switchTo(presenter);
    const end = await until('End Show', () => s.find('[data-testid="deck-presenter-end"]'));
    await s.click(end);
    await until('the audience window to close', async () => (await s.handles()).length === 1);
    check(true, 'ending the show closes the audience window');
    await editorRoute(s, 'deck');
  } finally {
    await s.end();
  }
}

async function main(): Promise<void> {
  const app = application();
  console.log(`kiln: ${app}`);
  const work = mkdtempSync(join(tmpdir(), 'kiln-tauri-smoke-'));
  const driver = await startDriver(work);
  let failed = false;
  try {
    for (const scenario of [typeAndSave, newAndOpenKeepTheirWindows, presenterView, cjkAndEmojiDraw]) {
      try {
        await scenario(app, work);
      } catch (error) {
        failed = true;
        console.error(`  FAIL ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  } finally {
    driver.kill();
    rmSync(work, { recursive: true, force: true });
  }
  if (failed) process.exit(1);
  console.log('desktop smoke: passed');
}

await main();
