/**
 * LibreOffice headless as a test oracle (PLAN §4.5): format conversion through `tools/oracle-soffice.sh`
 * and PDF rasterisation through `pdftoppm`.
 */
import { execFile } from 'node:child_process';
import {
  accessSync,
  constants,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, extname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { TOOLS_DIR } from '../paths.ts';

const run = promisify(execFile);

export const SOFFICE = process.env.SOFFICE ?? '/usr/bin/soffice';
export const ORACLE_SCRIPT = join(TOOLS_DIR, 'oracle-soffice.sh');
const PROFILE = process.env.KILN_LO_PROFILE ?? '/tmp/kiln-lo-profile';

export class SofficeError extends Error {
  override name = 'SofficeError';
}

function executable(path: string): boolean {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** True when LibreOffice is installed (tests that need it are skipped otherwise, e.g. on CI). */
export function sofficeAvailable(): boolean {
  return executable(SOFFICE) && existsSync(ORACLE_SCRIPT);
}

/** True when poppler's `pdftoppm` is on the PATH. */
export function pdftoppmAvailable(): boolean {
  return ['/usr/bin/pdftoppm', '/usr/local/bin/pdftoppm', '/opt/homebrew/bin/pdftoppm'].some(executable);
}

export type ConvertInput = string | { bytes: Uint8Array; name: string };

export interface ConvertOptions {
  /** Output directory; a fresh temporary directory by default. */
  outDir?: string;
  /** Kill LibreOffice after this many milliseconds. Default 180 000. */
  timeoutMs?: number;
}

export interface ConvertResult {
  path: string;
  bytes: Uint8Array;
}

/** The file extension LibreOffice gives a conversion target (`csv:Text - txt - csv…` → `csv`). */
export function targetExtension(format: string): string {
  return (format.split(':')[0] ?? format).trim();
}

function materialise(input: ConvertInput): string {
  if (typeof input === 'string') return resolve(input);
  const dir = mkdtempSync(join(tmpdir(), 'kiln-in-'));
  const path = join(dir, basename(input.name));
  writeFileSync(path, input.bytes);
  return path;
}

/**
 * Converts one document with LibreOffice (`pdf`, `csv`, `docx`, `xlsx`, `pptx`, `odt`, …). Throws
 * `SofficeError` when LibreOffice fails or produces no output — a document LibreOffice cannot load
 * produces no output file.
 */
export async function convertWithSoffice(
  input: ConvertInput,
  format: string,
  options: ConvertOptions = {},
): Promise<ConvertResult> {
  if (!sofficeAvailable()) throw new SofficeError(`LibreOffice not found at ${SOFFICE}`);
  const inPath = materialise(input);
  const outDir = options.outDir ?? mkdtempSync(join(tmpdir(), 'kiln-oracle-'));
  mkdirSync(outDir, { recursive: true });
  const ext = targetExtension(format);
  const outPath = join(outDir, `${basename(inPath, extname(inPath))}.${ext}`);
  rmSync(outPath, { force: true });
  let stderr = '';
  try {
    const r = await run('bash', [ORACLE_SCRIPT, inPath, format, outDir], {
      timeout: options.timeoutMs ?? 180_000,
      maxBuffer: 16 * 1024 * 1024,
    });
    stderr = r.stderr;
  } catch (e) {
    throw new SofficeError(`LibreOffice failed on ${basename(inPath)}: ${(e as Error).message}`);
  }
  if (!existsSync(outPath) || statSync(outPath).size === 0) {
    throw new SofficeError(
      `LibreOffice produced no ${ext} for ${basename(inPath)}${stderr ? `: ${stderr.trim()}` : ''}`,
    );
  }
  return { path: outPath, bytes: new Uint8Array(readFileSync(outPath)) };
}

export interface BatchResult {
  input: string;
  output?: string;
  error?: string;
}

/**
 * Converts many documents with one LibreOffice start-up per input extension, which is far faster than
 * one process per file. The first conversion goes through `oracle-soffice.sh` so the shared profile is
 * seeded exactly as the script seeds it; the rest reuse that profile under the same lock. Outputs go
 * to `<outDir>/<input extension>/`.
 */
export async function convertBatchWithSoffice(
  inputs: readonly string[],
  format: string,
  outDir: string,
  options: { timeoutMs?: number } = {},
): Promise<BatchResult[]> {
  if (!sofficeAvailable()) throw new SofficeError(`LibreOffice not found at ${SOFFICE}`);
  const ext = targetExtension(format);
  const byExt = new Map<string, string[]>();
  for (const i of inputs) {
    const e = extname(i).slice(1).toLowerCase();
    byExt.set(e, [...(byExt.get(e) ?? []), resolve(i)]);
  }
  const results: BatchResult[] = [];
  for (const [inExt, files] of byExt) {
    const dir = join(outDir, inExt);
    mkdirSync(dir, { recursive: true });
    const [first, ...rest] = files;
    if (!first) continue;
    try {
      await convertWithSoffice(first, format, { outDir: dir, timeoutMs: options.timeoutMs ?? 180_000 });
    } catch {
      // Recorded below from the missing output.
    }
    if (rest.length) {
      try {
        await run(
          'flock',
          [
            `${PROFILE}.lock`,
            SOFFICE,
            `-env:UserInstallation=file://${PROFILE}`,
            '--headless',
            '--norestore',
            '--convert-to',
            format,
            '--outdir',
            dir,
            ...rest,
          ],
          { timeout: options.timeoutMs ?? 600_000, maxBuffer: 64 * 1024 * 1024 },
        );
      } catch (e) {
        // Per-file outputs are checked below; a crash leaves the remaining files without output.
        results.push({ input: '(batch)', error: (e as Error).message });
      }
    }
    for (const f of files) {
      const out = join(dir, `${basename(f, extname(f))}.${ext}`);
      if (existsSync(out) && statSync(out).size > 0) results.push({ input: f, output: out });
      else results.push({ input: f, error: `no ${ext} output` });
    }
  }
  return results;
}

/**
 * Rasterises PDF pages to PNG with `pdftoppm` (default 60 dpi, the visual-score resolution of §5).
 * Returns one PNG buffer per page, in page order.
 */
export async function renderPdfPages(
  pdf: string | Uint8Array,
  options: { dpi?: number; firstPage?: number; lastPage?: number } = {},
): Promise<Buffer[]> {
  const dir = mkdtempSync(join(tmpdir(), 'kiln-pages-'));
  try {
    let pdfPath: string;
    if (typeof pdf === 'string') pdfPath = resolve(pdf);
    else {
      pdfPath = join(dir, 'in.pdf');
      writeFileSync(pdfPath, pdf);
    }
    const args = ['-r', String(options.dpi ?? 60), '-png'];
    if (options.firstPage) args.push('-f', String(options.firstPage));
    if (options.lastPage) args.push('-l', String(options.lastPage));
    args.push(pdfPath, join(dir, 'page'));
    try {
      await run('pdftoppm', args, { timeout: 120_000 });
    } catch (e) {
      throw new SofficeError(`pdftoppm failed: ${(e as Error).message}`);
    }
    const pages = readdirSync(dir)
      .filter((f) => /^page-\d+\.png$/.test(f))
      .sort((a, b) => Number(a.match(/\d+/)?.[0]) - Number(b.match(/\d+/)?.[0]));
    return pages.map((p) => readFileSync(join(dir, p)));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
