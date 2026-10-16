/**
 * openpyxl / python-docx / python-pptx as independent readers (dev-only oracle). Runs
 * `python/dump.py` in the project virtualenv (PLAN §4.5) and parses its JSON.
 */
import { execFile, execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { PYTHON_DIR } from '../paths.ts';

const run = promisify(execFile);

/** The interpreter: `KILN_PYTHON`, else `~/.venvs/kiln/bin/python`. */
export function pythonInterpreter(): string {
  return process.env.KILN_PYTHON ?? join(homedir(), '.venvs', 'kiln', 'bin', 'python');
}

let available: boolean | undefined;

/** True when the interpreter exists and can import python-docx, openpyxl and python-pptx. */
export function pythonAvailable(): boolean {
  if (available !== undefined) return available;
  const py = pythonInterpreter();
  if (!existsSync(py)) {
    available = false;
    return available;
  }
  try {
    execFileSync(py, ['-I', '-c', 'import docx, openpyxl, pptx'], { stdio: 'ignore', timeout: 30_000 });
    available = true;
  } catch {
    available = false;
  }
  return available;
}

export class PythonOracleError extends Error {
  override name = 'PythonOracleError';
}

export interface PythonDocxDump {
  format: 'docx';
  paragraphs: {
    style: string | null;
    text: string;
    runs: { text: string; bold: boolean | null; italic: boolean | null }[];
  }[];
  tables: string[][][];
  sections: { orientation: string; page_width: number | null; page_height: number | null }[];
  core: Record<string, string | number | null>;
}

export interface PythonXlsxDump {
  format: 'xlsx';
  sheets: {
    name: string;
    state: string;
    dimensions: string;
    cells: Record<string, { value: unknown; number_format: string; data_type: string }>;
    merged: string[];
  }[];
  defined_names: string[];
  epoch: string;
}

export interface PythonPptxDump {
  format: 'pptx';
  slide_width: number;
  slide_height: number;
  slides: {
    layout: string;
    hidden: boolean;
    shapes: {
      name: string;
      shape_type: string | null;
      text: string | null;
      left: number | null;
      top: number | null;
      width: number | null;
      height: number | null;
    }[];
    notes: string | null;
  }[];
}

export type PythonDump = PythonDocxDump | PythonXlsxDump | PythonPptxDump;

/** Dumps a .docx/.xlsx/.pptx file to JSON with the Python libraries. */
export async function readWithPython(path: string): Promise<PythonDump> {
  const ext = extname(path).slice(1).toLowerCase();
  if (!['docx', 'xlsx', 'pptx'].includes(ext)) throw new PythonOracleError(`unsupported file type .${ext}`);
  if (!pythonAvailable())
    throw new PythonOracleError(`no Python with docx/openpyxl/pptx at ${pythonInterpreter()}`);
  try {
    const { stdout } = await run(pythonInterpreter(), ['-I', join(PYTHON_DIR, 'dump.py'), resolve(path)], {
      timeout: 120_000,
      maxBuffer: 256 * 1024 * 1024,
    });
    return JSON.parse(stdout) as PythonDump;
  } catch (e) {
    throw new PythonOracleError(`python reader failed on ${path}: ${(e as Error).message}`);
  }
}
