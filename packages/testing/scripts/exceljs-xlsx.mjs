#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The exceljs oracle for the XLSX reader (kiln-io-xlsx, WP-70): writes
 * `fixtures/xlsx/read/<name>.exceljs.json` for every `fixtures/xlsx/features/*.xlsx` — per sheet the
 * merged cells and, per cell, the value (dates as serial numbers, rich text as its characters),
 * the formula and a summary of the format (number format, font, fill, alignment, borders). The
 * Rust test `crates/kiln-io-xlsx/tests/exceljs_oracle.rs` compares the reader with these files:
 * values and formulas must all agree, formats in at least 97 % of the properties.
 *
 *   node packages/testing/scripts/exceljs-xlsx.mjs           # regenerate
 *   node packages/testing/scripts/exceljs-xlsx.mjs --check   # fail when a file is stale
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const ExcelJS = require('exceljs');

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const features = join(repo, 'fixtures/xlsx/features');
const outDir = join(repo, 'fixtures/xlsx/read');
const check = process.argv.includes('--check');

/** Excel's serial number of a date exceljs made from a date-formatted number. */
function serial(date, date1904) {
  const base = date1904 ? Date.UTC(1904, 0, 1) : Date.UTC(1899, 11, 30);
  return Math.round(((date.getTime() - base) / 86400000) * 1e10) / 1e10;
}

function color(c) {
  if (!c) return undefined;
  if (c.argb) return `argb:${c.argb.toUpperCase()}`;
  if (c.theme !== undefined) return `theme:${c.theme}${c.tint ? `:${c.tint}` : ''}`;
  if (c.indexed !== undefined) return `indexed:${c.indexed}`;
  return undefined;
}

function prune(o) {
  if (o === null || typeof o !== 'object' || Array.isArray(o)) return o;
  const out = {};
  for (const k of Object.keys(o).sort()) {
    const v = prune(o[k]);
    if (v === undefined || (typeof v === 'object' && v !== null && Object.keys(v).length === 0)) continue;
    out[k] = v;
  }
  return out;
}

function styleSummary(cell) {
  const s = cell.style ?? {};
  const f = s.font ?? {};
  const a = s.alignment ?? {};
  const b = s.border ?? {};
  const fill = s.fill ?? {};
  return prune({
    numFmt: s.numFmt && s.numFmt !== 'General' ? s.numFmt : undefined,
    font: {
      bold: f.bold ? true : undefined,
      italic: f.italic ? true : undefined,
      underline: f.underline ? String(f.underline === true ? 'single' : f.underline) : undefined,
      strike: f.strike ? true : undefined,
      size: f.size,
      name: f.name,
      color: color(f.color),
    },
    fill:
      fill.type === 'pattern' && fill.pattern && fill.pattern !== 'none'
        ? { pattern: fill.pattern, fg: color(fill.fgColor) }
        : undefined,
    alignment: {
      horizontal: a.horizontal,
      vertical: a.vertical,
      wrapText: a.wrapText ? true : undefined,
      indent: a.indent || undefined,
      textRotation: a.textRotation || undefined,
    },
    border: {
      left: b.left?.style,
      right: b.right?.style,
      top: b.top?.style,
      bottom: b.bottom?.style,
    },
  });
}

function cellValue(v, date1904) {
  if (v === null || v === undefined) return { type: 'blank' };
  if (typeof v === 'number') return { type: 'number', value: v };
  if (typeof v === 'string') return { type: 'string', value: v };
  if (typeof v === 'boolean') return { type: 'boolean', value: v };
  if (v instanceof Date) return { type: 'number', value: serial(v, date1904) };
  if (v.richText) return { type: 'string', value: v.richText.map((r) => r.text).join('') };
  if (v.error) return { type: 'error', value: v.error };
  if (v.text !== undefined && v.hyperlink !== undefined) return cellValue(v.text, date1904);
  return { type: 'other' };
}

async function describe(path) {
  const wb = new ExcelJS.Workbook();
  await wb.xlsx.readFile(path);
  const date1904 = Boolean(wb.properties?.date1904);
  const sheets = [];
  wb.eachSheet((ws) => {
    const cells = {};
    ws.eachRow({ includeEmpty: false }, (row) => {
      row.eachCell({ includeEmpty: false }, (cell) => {
        const raw = cell.value;
        const isFormula = raw && typeof raw === 'object' && ('formula' in raw || 'sharedFormula' in raw);
        const entry = {};
        if (isFormula) {
          if (typeof raw.formula === 'string') entry.formula = raw.formula;
        } else {
          Object.assign(entry, cellValue(raw, date1904));
        }
        const style = styleSummary(cell);
        if (Object.keys(style).length > 0) entry.style = style;
        cells[cell.address] = entry;
      });
    });
    const merges = [...(ws.model?.merges ?? [])].sort();
    sheets.push({ name: ws.name, state: ws.state, merges, cells: prune(cells) });
  });
  return { date1904, sheets };
}

mkdirSync(outDir, { recursive: true });
let stale = 0;
for (const name of readdirSync(features)
  .filter((f) => f.endsWith('.xlsx'))
  .sort()) {
  // exceljs 4.4 cannot read some valid files (absolute relationship targets to drawings); the
  // file records that, and the Rust test reports the workbook as not checked by this oracle.
  const data = await describe(join(features, name)).catch((e) => ({
    error: `exceljs cannot read this file: ${String(e.message ?? e).split('\n')[0]}`,
  }));
  const text = `${JSON.stringify(data, null, 1)}\n`;
  const target = join(outDir, name.replace(/\.xlsx$/, '.exceljs.json'));
  if (check) {
    if (!existsSync(target) || readFileSync(target, 'utf8') !== text) {
      console.error(`stale: ${target}`);
      stale += 1;
    }
  } else {
    writeFileSync(target, text);
    console.log(`wrote ${target}`);
  }
}
if (stale > 0) process.exit(1);
