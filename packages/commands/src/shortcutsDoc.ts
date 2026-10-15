/**
 * Reads the shortcut tables of docs/SHORTCUTS.md, the human-readable specification of PLAN.md §1.7.
 * Used by the keymap conformance test, and available to tools that render the cheat sheet.
 */

export interface ShortcutDocRow {
  /** First column, verbatim (`Ctrl+N / O / S / P / W`). */
  shortcut: string;
  write: string;
  grid: string;
  deck: string;
}

export interface SlideShowDocRow {
  key: string;
  action: string;
}

export interface ShortcutDoc {
  rows: ShortcutDocRow[];
  slideShow: SlideShowDocRow[];
}

function cells(line: string): string[] {
  const inner = line.trim().replace(/^\|/, '').replace(/\|$/, '');
  return inner.split('|').map((c) => c.trim().replace(/^`(.*)`$/, '$1'));
}

/** Parses the main table (Shortcut | Scriba | Tabula | Scena) and the "During a slide show" table. */
export function parseShortcutDoc(markdown: string): ShortcutDoc {
  const doc: ShortcutDoc = { rows: [], slideShow: [] };
  const lines = markdown.split(/\r?\n/);
  let mode: 'none' | 'main' | 'show' = 'none';
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? '';
    if (!line.trim().startsWith('|')) {
      mode = 'none';
      continue;
    }
    const cols = cells(line);
    if (mode === 'none') {
      const header = cols.map((c) => c.toLowerCase());
      if (header[0] === 'shortcut' && cols.length >= 4) mode = 'main';
      else if (header[0] === 'key' && cols.length >= 2) mode = 'show';
      else continue;
      i++; // skip the |---| separator
      continue;
    }
    if (mode === 'main') {
      doc.rows.push({
        shortcut: cols[0] ?? '',
        write: cols[1] ?? '',
        grid: cols[2] ?? '',
        deck: cols[3] ?? '',
      });
    } else {
      doc.slideShow.push({ key: cols[0] ?? '', action: cols[1] ?? '' });
    }
  }
  return doc;
}

/** `—` / `-` / empty: the shortcut does nothing in that app. */
export function isNoneCell(cell: string): boolean {
  return /^[—–-]?$/.test(cell.trim());
}

const MODIFIER_TOKENS = new Set(['ctrl', 'mod', 'alt', 'shift', 'cmd', 'meta']);
const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];

function splitPlus(s: string): string[] {
  const t = s.trim();
  if (t.endsWith('++')) return [...t.slice(0, -2).split('+'), '+'];
  return t.split('+');
}

function expandArrows(shortcut: string): string[] {
  return /(^|\+)Arrows$/i.test(shortcut) ? ARROWS.map((a) => shortcut.replace(/Arrows$/i, a)) : [shortcut];
}

/**
 * Expands a shortcut-column expression into individual shortcuts:
 * `Ctrl+N / O / S` → `Ctrl+N`, `Ctrl+O`, `Ctrl+S` (later parts inherit the modifiers),
 * `,` and ` or ` separate alternatives, `(also X)` adds X, `Arrows` expands to the four arrows,
 * `number, then Enter` → digits 0–9 and Enter. Other parentheticals and ★ are ignored.
 */
export function expandShortcutExpression(expr: string): string[] {
  let text = expr.replace(/★/g, ' ');
  const extra: string[] = [];
  text = text.replace(/\(([^)]*)\)/g, (_m, inner: string) => {
    const also = inner.match(/^\s*also\s+(.*)$/i);
    if (also?.[1]) extra.push(also[1]);
    return ' ';
  });
  const out: string[] = [];
  for (const group of [...text.split(/,\s*|\s+or\s+/i), ...extra]) {
    const g = group.trim().replace(/^then\s+/i, '');
    if (!g) continue;
    if (/^number$/i.test(g)) {
      for (let d = 0; d <= 9; d++) out.push(String(d));
      continue;
    }
    let mods: string[] = [];
    for (const part of g.split(/\s+\/\s+/)) {
      const tokens = splitPlus(part).map((t) => t.trim());
      const leadingMods = tokens.slice(0, -1);
      const key = tokens[tokens.length - 1] ?? '';
      if (leadingMods.length > 0 && leadingMods.every((m) => MODIFIER_TOKENS.has(m.toLowerCase()))) {
        mods = leadingMods;
        out.push(...expandArrows([...mods, key].join('+')));
      } else {
        out.push(...expandArrows([...mods, part.trim()].join('+')));
      }
    }
  }
  return [...new Set(out)];
}

/** Shortcuts mentioned in parentheses inside an app cell: `Go To (also F5)`, `(Shift+F2: note)`. */
export function cellShortcuts(cell: string): string[] {
  const out: string[] = [];
  for (const m of cell.matchAll(/\(([^)]*)\)/g)) {
    const inner =
      (m[1] ?? '')
        .replace(/^\s*also\s+/i, '')
        .split(':')[0]
        ?.trim() ?? '';
    if (/^(ctrl|alt|shift|f\d+)/i.test(inner)) out.push(...expandShortcutExpression(inner));
  }
  return out;
}
